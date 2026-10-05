//! Facade configuration and sync builder.
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

/// Per-shard connection status (pre-spawn placeholder until `Cluster` runs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShardStatus {
    /// Not yet connected (pre-`login` or restarting).
    Disconnected,
    /// Connected (reserved for post-`Cluster` spawn; currently never set).
    Connected,
}

/// Per-shard health snapshot.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShardHealth {
    /// Shard identity.
    pub shard: gateway::ShardId,
    /// Connection status.
    pub status: ShardStatus,
    /// Last latency in milliseconds.
    pub latency_ms: u64,
    /// Last sequence number (`None` pre-spawn; no seq tracking yet).
    pub seq: Option<u64>,
    /// Guilds in cache (global count; cache is not per-shard).
    pub guilds_cached: usize,
}

/// Client health snapshot.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ClientHealth {
    /// Per-shard health.
    pub shards: Vec<ShardHealth>,
    /// Whether `login()` has succeeded (30s READY stub).
    pub ready: bool,
}

/// Client configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Gateway intents.
    pub intents: model::Intents,
    /// Cache configuration.
    pub cache: cache::CacheConfig,
    /// Shard strategy.
    pub sharding: gateway::ShardStrategy,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            intents: model::Intents::GUILDS | model::Intents::GUILD_MESSAGES,
            cache: cache::CacheConfig::minimal(),
            sharding: gateway::ShardStrategy::Auto,
        }
    }
}

/// Sync builder (no I/O in `build`).
pub struct ClientBuilder {
    token: secrecy::SecretString,
    config: Config,
    handlers: Vec<Arc<dyn crate::EventHandler>>,
}

impl std::fmt::Debug for ClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientBuilder").finish_non_exhaustive()
    }
}

impl ClientBuilder {
    /// Set intents.
    #[must_use]
    pub fn intents(mut self, i: model::Intents) -> Self {
        self.config.intents = i;
        self
    }
    /// Configure cache.
    #[must_use]
    pub fn cache(mut self, f: impl FnOnce(cache::CacheConfig) -> cache::CacheConfig) -> Self {
        let cfg = std::mem::replace(&mut self.config.cache, cache::CacheConfig::minimal());
        self.config.cache = f(cfg);
        self
    }
    /// Shard strategy.
    #[must_use]
    pub fn sharding(mut self, s: gateway::ShardStrategy) -> Self {
        self.config.sharding = s;
        self
    }
    /// Add an event handler (multi-listener, repeatable).
    #[must_use]
    pub fn add_handler(mut self, h: impl crate::EventHandler) -> Self {
        self.handlers.push(Arc::new(h));
        self
    }
    /// Build (sync).
    ///
    /// `shards` is the configured cheapest placeholder: one
    /// [`gateway::ShardMessenger`] with `ShardId { id: 0, total: 1 }`.
    /// The real count comes from `GET /gateway/bot` at `login()` once the
    /// `Cluster` spawns (P6); until then `health()`/`latencies()` report
    /// this single placeholder. `Manual` carries no ids (frozen P1a shape),
    /// so it also maps to the single placeholder (documented gap).
    ///
    /// # Errors
    /// Returns [`common::Error::Config`] on bad configuration.
    pub fn build(self) -> Result<Client, common::Error> {
        let token = self.token.clone();
        let http = Arc::new(rest::Client::builder(self.token).build()?);
        let cache: Arc<dyn cache::Cache> =
            Arc::new(cache::InMemoryCache::new(self.config.cache.clone()));
        #[cfg(feature = "standby")]
        let standby = Arc::new(standby::Standby::new());
        let messenger = gateway::ShardMessenger {
            shard: gateway::ShardId { id: 0, total: 1 },
            latency: Arc::new(AtomicU64::new(0)),
        };
        Ok(Client {
            config: self.config,
            token,
            http,
            cache,
            #[cfg(feature = "standby")]
            standby,
            dispatcher: Arc::new(crate::Dispatcher::new(self.handlers)),
            shards: vec![messenger],
            started_at: Instant::now(),
            ready_at: Mutex::new(None),
            shutdown: tokio_util::sync::CancellationToken::new(),
        })
    }
}

/// Connected client.
pub struct Client {
    config: Config,
    token: secrecy::SecretString,
    http: Arc<rest::Client>,
    cache: Arc<dyn cache::Cache>,
    #[cfg(feature = "standby")]
    standby: Arc<standby::Standby>,
    dispatcher: Arc<crate::Dispatcher>,
    shards: Vec<gateway::ShardMessenger>,
    started_at: Instant,
    ready_at: Mutex<Option<Instant>>,
    shutdown: tokio_util::sync::CancellationToken,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

/// Live `GET /gateway/bot` bootstrap for [`Client::login`].
///
/// Executes [`rest::Route::GetGatewayBot`] against the client's own REST
/// base (production `https://discord.com/api/v10` unless the builder
/// overrode it) and parses [`model::GetGatewayBotResponse`]. Network
/// failures propagate as [`common::Error::Network`]/[`common::Error::Timeout`].
struct RestBootstrap {
    http: Arc<rest::Client>,
}

impl gateway::Bootstrap for RestBootstrap {
    fn get_gateway_bot(
        &self,
    ) -> futures::future::BoxFuture<'_, Result<model::GetGatewayBotResponse, common::Error>> {
        Box::pin(async move {
            let route = rest::Route::GetGatewayBot;
            let bytes = self.http.execute(&route, None, None).await?;
            common::json::from_slice::<model::GetGatewayBotResponse>(&bytes).map_err(|e| {
                common::Error::Deserialize {
                    event: Box::from("GetGatewayBot"),
                    reason: common::error::truncate_source(&e.to_string()),
                }
            })
        })
    }
}

impl Client {
    /// Start building with a token.
    pub fn builder(token: impl Into<secrecy::SecretString>) -> ClientBuilder {
        ClientBuilder {
            token: token.into(),
            config: Config::default(),
            handlers: Vec::new(),
        }
    }
    /// Login against live Discord (`GET /gateway/bot` then spawn).
    ///
    /// Fetches [`model::GetGatewayBotResponse`] over REST (network errors
    /// propagate as [`common::Error::Network`]/[`common::Error::Timeout`] —
    /// never panics offline), sizes `Auto` sharding from the response, and
    /// spawns `gateway::Cluster` against the returned `url` (live
    /// `wss://...` needs the `tls` feature; see `plans/DECISIONS.md` D1).
    /// Fails after 30s without READY and stamps `ready_at` on success.
    ///
    /// # Errors
    /// Returns [`common::Error`] when bootstrap fails or READY times out.
    pub async fn login(&self) -> Result<(), common::Error> {
        let bootstrap = RestBootstrap {
            http: self.http.clone(),
        };
        self.login_inner(None, bootstrap).await
    }
    /// Login against an explicit gateway base URL (tests + custom gateways).
    ///
    /// Same internals as [`Self::login`], but the bootstrap is a stub
    /// pointing at `base_url` (e.g. `ws://127.0.0.1:port` for mocks) and
    /// sharding resolves to a single shard. Forwards events through
    /// cache/standby/handlers, and fails after 30s without READY. Live
    /// `Client::shards` messenger wiring is still a placeholder
    /// (health reports the pre-spawn messenger; per-shard live latencies
    /// live in the cluster tasks).
    ///
    /// # Errors
    /// Returns [`common::Error::Timeout`] after 30s without READY and
    /// [`common::Error`] when the cluster fails to spawn.
    pub async fn login_with_url(&self, base_url: &str) -> Result<(), common::Error> {
        struct StubBootstrap {
            url: String,
            shards: u32,
        }
        impl gateway::Bootstrap for StubBootstrap {
            fn get_gateway_bot(
                &self,
            ) -> futures::future::BoxFuture<'_, Result<model::GetGatewayBotResponse, common::Error>>
            {
                let url: Box<str> = Box::from(self.url.clone());
                let shards = self.shards;
                Box::pin(async move {
                    Ok(model::GetGatewayBotResponse {
                        url,
                        shards,
                        session_start_limit: model::SessionStartLimit {
                            total: 1000,
                            remaining: 1000,
                            reset_after: 0,
                            max_concurrency: 1,
                        },
                    })
                })
            }
        }
        let bootstrap = StubBootstrap {
            url: String::from(base_url),
            shards: 1,
        };
        self.login_inner(Some(String::from(base_url)), bootstrap)
            .await
    }
    /// Shared login path: fetch `GET /gateway/bot` once (sizes `Auto`),
    /// spawn `gateway::Cluster` with a replay bootstrap, wait for READY
    /// (30s), stamp `ready_at`, then forward later events in the background.
    async fn login_inner<B>(
        &self,
        base_url: Option<String>,
        bootstrap: B,
    ) -> Result<(), common::Error>
    where
        B: gateway::Bootstrap,
    {
        let bot = bootstrap.get_gateway_bot().await?;
        let total = match self.config.sharding {
            gateway::ShardStrategy::Single => 1,
            gateway::ShardStrategy::Manual => 1,
            gateway::ShardStrategy::Auto => gateway::recommended_shards(&bot),
            _ => 1,
        };
        struct ReplayBootstrap {
            bot: model::GetGatewayBotResponse,
        }
        impl gateway::Bootstrap for ReplayBootstrap {
            fn get_gateway_bot(
                &self,
            ) -> futures::future::BoxFuture<'_, Result<model::GetGatewayBotResponse, common::Error>>
            {
                let bot = self.bot.clone();
                Box::pin(async move { Ok(bot) })
            }
        }
        let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<model::Event>(256);
        let _cluster = gateway::Cluster::spawn(
            gateway::ClusterConfig::new(total, 1),
            gateway::ClusterAuth {
                token: self.token.clone(),
                intents: self.config.intents,
                base_url,
                shutdown: self.shutdown.clone(),
            },
            ReplayBootstrap { bot },
            out_tx,
        )
        .await?;
        let messenger = self
            .shards
            .first()
            .cloned()
            .unwrap_or_else(|| gateway::ShardMessenger {
                shard: gateway::ShardId { id: 0, total },
                latency: Arc::new(AtomicU64::new(0)),
            });
        let ctx = self.context(messenger);
        let found = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(ev) = out_rx.recv().await {
                let is_ready = matches!(ev, model::Event::Ready(_));
                self.dispatch(&ctx, ev).await;
                if is_ready {
                    return true;
                }
            }
            false
        })
        .await
        .map_err(|_| common::Error::Timeout(Box::from("READY timeout")))?;
        if !found {
            return Err(common::Error::Timeout(Box::from("READY timeout")));
        }
        match self.ready_at.lock() {
            Ok(mut g) => {
                *g = Some(Instant::now());
            }
            Err(p) => {
                *p.into_inner() = Some(Instant::now());
            }
        }
        let http = self.http.clone();
        let cache = self.cache.clone();
        #[cfg(feature = "standby")]
        let standby = self.standby.clone();
        let dispatcher = self.dispatcher.clone();
        let shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            let shard = gateway::ShardMessenger {
                shard: gateway::ShardId { id: 0, total: 1 },
                latency: Arc::new(AtomicU64::new(0)),
            };
            let ctx = crate::Context {
                http,
                cache,
                #[cfg(feature = "standby")]
                standby,
                shard,
            };
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    msg = out_rx.recv() => {
                        match msg {
                            None => break,
                            Some(ev) => dispatcher.dispatch(&ctx, ev).await,
                        }
                    }
                }
            }
        });
        Ok(())
    }
    /// Graceful shutdown (close 1000, cancel chunks, drain dispatcher, 5s max).
    ///
    /// Cancels the [`Self::shutdown_handle`] token, then drains up to 5s.
    /// P5 stub: no live shards/chunks yet, so the drain is a bounded wait.
    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), async {}).await;
    }
    /// Shutdown token (clone; cancelled by [`Self::shutdown`]).
    ///
    /// P6 `Cluster`/chunk tasks select on this token plus the process
    /// signal; cancelling here starts the 5s drain.
    #[must_use]
    pub fn shutdown_handle(&self) -> tokio_util::sync::CancellationToken {
        self.shutdown.clone()
    }
    /// Configuration snapshot.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }
    /// Build a `Context` for managers and handlers.
    #[must_use]
    pub fn context(&self, shard: gateway::ShardMessenger) -> crate::Context {
        crate::Context {
            http: self.http.clone(),
            cache: self.cache.clone(),
            #[cfg(feature = "standby")]
            standby: self.standby.clone(),
            shard,
        }
    }
    /// Per-shard latencies from messengers.
    ///
    /// Pre-spawn this is the configured cheapest placeholder (single
    /// `(ShardId { id: 0, total: 1 }, 0)`); post-`Cluster` spawn it mirrors
    /// live messenger latencies.
    #[must_use]
    pub fn latencies(&self) -> Vec<(gateway::ShardId, u64)> {
        self.shards
            .iter()
            .map(|m| (m.shard, m.latency_ms()))
            .collect()
    }
    /// Health snapshot: per-shard `{status, latency_ms, seq, guilds_cached}`.
    ///
    /// `status` is `Disconnected` pre-spawn (no `Cluster` yet); `seq` is
    /// `None` (no seq tracking until P6 run loop); `guilds_cached` is the
    /// global cache count (cache is not per-shard; documented
    /// approximation).
    #[must_use]
    pub fn health(&self) -> ClientHealth {
        let guilds_cached = self.cache.stats().guilds;
        let shards = self
            .shards
            .iter()
            .map(|m| ShardHealth {
                shard: m.shard,
                status: ShardStatus::Disconnected,
                latency_ms: m.latency_ms(),
                seq: None,
                guilds_cached,
            })
            .collect();
        ClientHealth {
            shards,
            ready: self.ready_at().is_some(),
        }
    }
    /// Instant `login()` succeeded, if any.
    #[must_use]
    pub fn ready_at(&self) -> Option<Instant> {
        match self.ready_at.lock() {
            Ok(g) => *g,
            Err(p) => *p.into_inner(),
        }
    }
    /// Time since `build()`.
    #[must_use]
    pub fn uptime(&self) -> Duration {
        self.started_at.elapsed()
    }
    /// Dispatch an event through cache/standby/handlers.
    pub async fn dispatch(&self, ctx: &crate::Context, ev: model::Event) {
        self.dispatcher.dispatch(ctx, ev).await;
    }
    /// Access the dispatcher (handlers).
    #[must_use]
    pub fn dispatcher(&self) -> &crate::Dispatcher {
        &self.dispatcher
    }
}
