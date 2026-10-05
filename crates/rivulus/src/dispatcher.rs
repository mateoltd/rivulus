//! Multi-listener dispatcher (BoxFuture object-safe form, frozen P1a).
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Broadcast capacity for handler fan-out (per plans/05 §2).
pub const BROADCAST_CAPACITY: usize = 2048;

/// Object-safe event handler.
pub trait EventHandler: Send + Sync + 'static {
    /// Handle a dispatch event.
    fn on_dispatch(
        &self,
        ctx: crate::Context,
        ev: model::Event,
    ) -> futures::future::BoxFuture<'_, ()>;
}

/// Blanket implementation for closures and functions.
impl<F, Fut> EventHandler for F
where
    F: Fn(crate::Context, model::Event) -> Fut + Send + Sync + 'static,
    Fut: futures::Future<Output = ()> + Send + 'static,
{
    fn on_dispatch(
        &self,
        ctx: crate::Context,
        ev: model::Event,
    ) -> futures::future::BoxFuture<'_, ()> {
        Box::pin((self)(ctx, ev))
    }
}

/// Lag policy for a dropped (`Lagged`) broadcast event.
///
/// Mirrors plans/05 §2: cacheable events can be refetched by id after a
/// lag (plus `warn!`); ephemeral events cannot be reconstructed from cache
/// (drop + count); anything else is counted only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LagPolicy {
    /// Cacheable (message/guild/member/channel/role): refetch by id + `warn!`.
    Refetch,
    /// Ephemeral (typing/presence/reaction-remove/voice): drop + count.
    Drop,
    /// Unknown/synthetic: count only.
    Count,
}

/// Classify an event for the `Lagged` policy.
///
/// Cacheable: `MessageCreate`/`MessageUpdate`, `GuildCreate`/`GuildUpdate`,
/// `MemberAdd`/`MemberUpdate`/`MembersChunk`, `ChannelCreate`/`ChannelUpdate`,
/// `RoleCreate`/`RoleUpdate`, `UserUpdate`. Ephemeral: `TypingStart`,
/// `PresenceUpdate`, `ReactionRemove`/`ReactionRemoveAll`/`ReactionRemoveEmoji`,
/// `VoiceStateUpdate`/`VoiceServerUpdate`. Everything else (including
/// `Unknown`, `Raw`, `RateLimited`, `CacheSwept`, `Resumed`) counts only.
#[must_use]
pub fn lag_policy(ev: &model::Event) -> LagPolicy {
    match ev {
        model::Event::MessageCreate(_)
        | model::Event::MessageUpdate(..)
        | model::Event::GuildCreate(_)
        | model::Event::GuildUpdate(..)
        | model::Event::GuildAvailable(_)
        | model::Event::MemberAdd(_)
        | model::Event::MemberUpdate(..)
        | model::Event::MembersChunk(_)
        | model::Event::ChannelCreate(_)
        | model::Event::ChannelUpdate(..)
        | model::Event::RoleCreate { .. }
        | model::Event::RoleUpdate(..)
        | model::Event::UserUpdate(_) => LagPolicy::Refetch,
        model::Event::TypingStart { .. }
        | model::Event::PresenceUpdate(_)
        | model::Event::ReactionRemove { .. }
        | model::Event::ReactionRemoveAll { .. }
        | model::Event::ReactionRemoveEmoji { .. }
        | model::Event::VoiceStateUpdate(_)
        | model::Event::VoiceServerUpdate(_) => LagPolicy::Drop,
        _ => LagPolicy::Count,
    }
}

/// Dispatcher (fan-out gateway to cache to standby to handlers).
pub struct Dispatcher {
    handlers: Vec<Arc<dyn EventHandler>>,
    broadcaster: tokio::sync::broadcast::Sender<Arc<model::Event>>,
    lag_dropped: AtomicU64,
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl Dispatcher {
    /// New with handlers (broadcast channel sized [`BROADCAST_CAPACITY`]).
    #[must_use]
    pub fn new(handlers: Vec<Arc<dyn EventHandler>>) -> Self {
        let (tx, _rx) = tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
        Self {
            handlers,
            broadcaster: tx,
            lag_dropped: AtomicU64::new(0),
        }
    }
    /// Add a handler.
    pub fn add_handler(&mut self, h: impl EventHandler) {
        self.handlers.push(Arc::new(h));
    }
    /// Subscribe to the broadcast fan-out.
    ///
    /// Receivers that fall behind observe `Lagged(n)` on `recv`; classify
    /// the missed event with [`lag_policy`] and report via
    /// [`Self::record_lag`] (never silent: `warn!` + counter).
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Arc<model::Event>> {
        self.broadcaster.subscribe()
    }
    /// Total lag-dropped events (maps to the `standby_lag_total` metric).
    #[must_use]
    pub fn lag_dropped(&self) -> u64 {
        self.lag_dropped.load(Ordering::Relaxed)
    }
    /// Record a `Lagged(n)` skip for an event: `warn!` + counter bump.
    ///
    /// Policy per kind (plans/05 §2): cacheable => refetch-by-id path
    /// (`warn!` names the refetch; the caller re-`fetch`es from REST);
    /// ephemeral => drop + count (reconstruction from cache is impossible);
    /// unknown/synthetic => count only. Never panics.
    pub fn record_lag(&self, ev: &model::Event, lagged_by: u64) {
        let n = lagged_by.max(1);
        self.lag_dropped.fetch_add(n, Ordering::Relaxed);
        match lag_policy(ev) {
            LagPolicy::Refetch => tracing::warn!(
                kind = ev.kind(),
                lagged_by = n,
                "broadcast lagged: cacheable event dropped, refetch by id"
            ),
            LagPolicy::Drop => tracing::warn!(
                kind = ev.kind(),
                lagged_by = n,
                "broadcast lagged: ephemeral event dropped"
            ),
            LagPolicy::Count => tracing::warn!(
                kind = ev.kind(),
                lagged_by = n,
                "broadcast lagged: event counted"
            ),
        }
    }
    /// Broadcast fan-out (`tokio::sync::broadcast`, capacity 2048).
    ///
    /// Best-effort `send`; a full buffer evicts the oldest message and slow
    /// receivers observe `Lagged` on `recv` (report via
    /// [`Self::record_lag`]). No-receiver sends return `0` and are not lag.
    /// Never panics, never blocks.
    pub fn dispatch_broadcast(&self, ev: Arc<model::Event>) -> usize {
        self.broadcaster.send(ev).unwrap_or_default()
    }
    /// Fallible broadcast fan-out (same channel, `Result` form).
    ///
    /// `Ok(n)` receivers notified; `Err(0)` when nobody subscribes (not
    /// lag, not an error for dispatch). Overflow never surfaces here as
    /// `Full` (broadcast evicts oldest instead); receivers detect it as
    /// `Lagged` and must call [`Self::record_lag`] so the drop is counted,
    /// never silent. Never panics.
    pub fn try_dispatch_broadcast(
        &self,
        ev: Arc<model::Event>,
    ) -> Result<usize, tokio::sync::broadcast::error::SendError<Arc<model::Event>>> {
        match self.broadcaster.send(ev) {
            Ok(n) => Ok(n),
            Err(e) => {
                let _ = e.0.kind();
                Err(e)
            }
        }
    }
    /// Dispatch one event (cache first, then standby feed, then handlers).
    ///
    /// Also best-effort feeds the broadcast fan-out (ignored when nobody
    /// subscribes); broadcast overflow never fails direct dispatch.
    pub async fn dispatch(&self, ctx: &crate::Context, ev: model::Event) {
        ctx.cache.update_cache(&ev);
        #[cfg(feature = "standby")]
        ctx.standby.feed(Arc::new(ev.clone()));
        let _ = self.broadcaster.send(Arc::new(ev.clone()));
        for h in &self.handlers {
            h.on_dispatch(ctx.clone(), ev.clone()).await;
        }
    }
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dispatcher").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Counter {
        n: Arc<AtomicUsize>,
    }

    impl EventHandler for Counter {
        fn on_dispatch(
            &self,
            _ctx: crate::Context,
            _ev: model::Event,
        ) -> futures::future::BoxFuture<'_, ()> {
            let n = self.n.clone();
            Box::pin(async move {
                n.fetch_add(1, Ordering::Relaxed);
            })
        }
    }

    struct Marker {
        n: Arc<AtomicUsize>,
        step: usize,
    }

    impl EventHandler for Marker {
        fn on_dispatch(
            &self,
            _ctx: crate::Context,
            _ev: model::Event,
        ) -> futures::future::BoxFuture<'_, ()> {
            let n = self.n.clone();
            let step = self.step;
            Box::pin(async move {
                n.fetch_add(step, Ordering::Relaxed);
            })
        }
    }

    fn test_ctx() -> crate::Context {
        let http = Arc::new(
            rest::Client::builder(secrecy::SecretString::from(String::from("t")))
                .build()
                .expect("rest build"),
        );
        let cache: Arc<dyn cache::Cache> =
            Arc::new(cache::InMemoryCache::new(cache::CacheConfig::minimal()));
        #[cfg(feature = "standby")]
        let standby = Arc::new(standby::Standby::new());
        crate::Context {
            http,
            cache,
            #[cfg(feature = "standby")]
            standby,
            shard: gateway::ShardMessenger {
                shard: gateway::ShardId { id: 0, total: 1 },
                latency: Arc::new(AtomicU64::new(0)),
            },
        }
    }

    #[tokio::test]
    async fn multi_handler_two_structs_plus_closure() {
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let c = Arc::new(AtomicUsize::new(0));
        let d = Dispatcher::new(vec![
            Arc::new(Counter { n: a.clone() }) as Arc<dyn EventHandler>,
            Arc::new(Marker {
                n: b.clone(),
                step: 10,
            }) as Arc<dyn EventHandler>,
            {
                let c = c.clone();
                Arc::new(move |_ctx: crate::Context, _ev: model::Event| {
                    let c = c.clone();
                    async move {
                        c.fetch_add(100, Ordering::Relaxed);
                    }
                }) as Arc<dyn EventHandler>
            },
        ]);
        let ctx = test_ctx();
        d.dispatch(&ctx, model::Event::Resumed).await;
        assert_eq!(a.load(Ordering::Relaxed), 1);
        assert_eq!(b.load(Ordering::Relaxed), 10);
        assert_eq!(c.load(Ordering::Relaxed), 100);
    }

    #[tokio::test]
    async fn builder_add_handler_multi() {
        let mut d = Dispatcher::new(Vec::new());
        let a = Arc::new(AtomicUsize::new(0));
        let b = Arc::new(AtomicUsize::new(0));
        let aa = a.clone();
        d.add_handler(Counter { n: a.clone() });
        d.add_handler(move |_ctx: crate::Context, _ev: model::Event| {
            let aa = aa.clone();
            async move {
                aa.fetch_add(1, Ordering::Relaxed);
            }
        });
        let _ = b;
        let ctx = test_ctx();
        d.dispatch(&ctx, model::Event::Resumed).await;
        assert_eq!(a.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn lag_policy_kinds() {
        let ch = model::ChannelId::new(3).unwrap();
        let uid = model::UserId::new(2).unwrap();
        assert_eq!(
            lag_policy(&model::Event::TypingStart {
                channel_id: ch,
                user_id: uid,
            }),
            LagPolicy::Drop
        );
        assert_eq!(
            lag_policy(&model::Event::Unknown {
                kind: Box::from("X"),
                seq: None,
                payload: Box::from("{}"),
            }),
            LagPolicy::Count
        );
    }
}
