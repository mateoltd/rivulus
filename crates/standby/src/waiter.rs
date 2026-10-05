//! `Standby` event waiters (bounded, sharded by event kind).
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Waiter kind shard (avoids O(waiters x events) fan-out).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WaiterKind {
    /// Messages.
    Message,
    /// Reactions.
    Reaction,
    /// Interactions.
    Interaction,
    /// Any.
    Any,
}

impl WaiterKind {
    fn of(ev: &model::Event) -> Self {
        match ev {
            model::Event::MessageCreate(_)
            | model::Event::MessageUpdate(..)
            | model::Event::MessageDelete { .. }
            | model::Event::MessageDeleteBulk { .. } => Self::Message,
            model::Event::ReactionAdd { .. }
            | model::Event::ReactionRemove { .. }
            | model::Event::ReactionRemoveAll { .. }
            | model::Event::ReactionRemoveEmoji { .. } => Self::Reaction,
            model::Event::InteractionCreate(_) => Self::Interaction,
            _ => Self::Any,
        }
    }
}

struct OneshotEntry {
    id: u64,
    pred: Box<dyn FnMut(&model::Event) -> bool + Send>,
    tx: tokio::sync::oneshot::Sender<Arc<model::Event>>,
}

struct StreamEntry {
    kind: WaiterKind,
    pred: Box<dyn Fn(&model::Event) -> bool + Send>,
    tx: futures::channel::mpsc::UnboundedSender<Arc<model::Event>>,
}

/// Standby router (cap 1024 waiters per kind-shard; oldest-evict + evict on timeout).
pub struct Standby {
    message: std::sync::Mutex<Vec<OneshotEntry>>,
    reaction: std::sync::Mutex<Vec<OneshotEntry>>,
    interaction: std::sync::Mutex<Vec<OneshotEntry>>,
    any: std::sync::Mutex<Vec<OneshotEntry>>,
    streams: std::sync::Mutex<Vec<StreamEntry>>,
    next_id: AtomicU64,
    /// Number of waiters ever evicted (full-shard oldest-evict + timeout).
    pub evicted: AtomicU64,
}

impl std::fmt::Debug for Standby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Standby").finish_non_exhaustive()
    }
}

impl Default for Standby {
    fn default() -> Self {
        Self {
            message: std::sync::Mutex::new(Vec::new()),
            reaction: std::sync::Mutex::new(Vec::new()),
            interaction: std::sync::Mutex::new(Vec::new()),
            any: std::sync::Mutex::new(Vec::new()),
            streams: std::sync::Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            evicted: AtomicU64::new(0),
        }
    }
}

impl Standby {
    /// New empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn shard(&self, kind: WaiterKind) -> &std::sync::Mutex<Vec<OneshotEntry>> {
        match kind {
            WaiterKind::Message => &self.message,
            WaiterKind::Reaction => &self.reaction,
            WaiterKind::Interaction => &self.interaction,
            WaiterKind::Any => &self.any,
        }
    }

    fn remove_by_id(&self, kind: WaiterKind, id: u64) {
        if let Ok(mut v) = self.shard(kind).lock() {
            if let Some(pos) = v.iter().position(|e| e.id == id) {
                v.remove(pos);
            }
        }
    }

    /// Wait for the next event of a kind matching a predicate with timeout.
    ///
    /// Bounded per kind-shard (1024): when full the oldest waiter is evicted
    /// (its future resolves `ShardDown`) and `evicted` is bumped. On timeout
    /// the waiter is removed and `evicted` is bumped.
    ///
    /// # Errors
    /// Returns [`common::Error::Timeout`] on timeout (== `WaitTimeout`
    /// semantics), [`common::Error::ShardBackpressure`] when the shard lock
    /// is poisoned, [`common::Error::ShardDown`] when evicted.
    pub async fn wait_for(
        &self,
        kind: WaiterKind,
        timeout: Duration,
        pred: impl FnMut(&model::Event) -> bool + Send + 'static,
    ) -> Result<Arc<model::Event>, common::Error> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let shard = self.shard(kind);
            let mut w = shard
                .lock()
                .map_err(|_| common::Error::ShardBackpressure(Box::from("standby lock")))?;
            if w.len() >= 1024 {
                w.remove(0);
                self.evicted.fetch_add(1, Ordering::Relaxed);
            }
            w.push(OneshotEntry {
                id,
                pred: Box::new(pred),
                tx,
            });
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(ev)) => Ok(ev),
            Ok(Err(_)) => Err(common::Error::ShardDown(Box::from("waiter gone"))),
            Err(_) => {
                self.remove_by_id(kind, id);
                self.evicted.fetch_add(1, Ordering::Relaxed);
                Err(common::Error::Timeout(Box::from("standby timeout")))
            }
        }
    }

    /// Subscribe to a kind-filtered event stream (`impl Stream`).
    /// Bounded (1024 streams total, oldest-evict). The returned
    /// `UnboundedReceiver` implements `futures::Stream`.
    pub fn stream_for(
        &self,
        kind: WaiterKind,
        pred: impl Fn(&model::Event) -> bool + Send + 'static,
    ) -> futures::channel::mpsc::UnboundedReceiver<Arc<model::Event>> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        if let Ok(mut s) = self.streams.lock() {
            if s.len() >= 1024 {
                s.remove(0);
                self.evicted.fetch_add(1, Ordering::Relaxed);
            }
            s.push(StreamEntry {
                kind,
                pred: Box::new(pred),
                tx,
            });
        }
        rx
    }

    /// Feed an event to matching waiters (kind-sharded, short-circuit).
    /// Only the event's kind shard plus `Any` are scanned; predicates decide
    /// delivery (non-matching waiters stay registered).
    pub fn feed(&self, event: Arc<model::Event>) {
        let want = WaiterKind::of(event.as_ref());
        let mut ready = Vec::new();
        if want == WaiterKind::Any {
            if let Ok(mut v) = self.any.lock() {
                let mut i = 0;
                while i < v.len() {
                    let matched = {
                        if let Some(e) = v.get_mut(i) {
                            (e.pred)(event.as_ref())
                        } else {
                            break;
                        }
                    };
                    if matched {
                        ready.push(v.remove(i).tx);
                    } else {
                        i = i.saturating_add(1);
                    }
                }
            }
        } else {
            for shard in [self.shard(want), self.shard(WaiterKind::Any)] {
                if let Ok(mut v) = shard.lock() {
                    let mut i = 0;
                    while i < v.len() {
                        let matched = {
                            if let Some(e) = v.get_mut(i) {
                                (e.pred)(event.as_ref())
                            } else {
                                break;
                            }
                        };
                        if matched {
                            ready.push(v.remove(i).tx);
                        } else {
                            i = i.saturating_add(1);
                        }
                    }
                }
            }
        }
        for tx in ready {
            let _ = tx.send(event.clone());
        }
        if let Ok(mut s) = self.streams.lock() {
            let mut i = 0;
            while i < s.len() {
                let should_send = {
                    if let Some(e) = s.get(i) {
                        let km = e.kind == want || e.kind == WaiterKind::Any;
                        km && (e.pred)(event.as_ref())
                    } else {
                        false
                    }
                };
                if should_send {
                    if let Some(e) = s.get(i) {
                        if e.tx.unbounded_send(event.clone()).is_ok() {
                            i = i.saturating_add(1);
                        } else {
                            s.remove(i);
                            self.evicted.fetch_add(1, Ordering::Relaxed);
                        }
                    } else {
                        break;
                    }
                } else {
                    i = i.saturating_add(1);
                }
            }
            s.retain(|e| !e.tx.is_closed());
        }
    }

    /// Wait for a message in a channel.
    ///
    /// # Errors
    /// Same as [`Self::wait_for`].
    pub async fn wait_for_message_in(
        &self,
        channel: model::ChannelId,
        timeout: Duration,
    ) -> Result<Arc<model::Event>, common::Error> {
        self.wait_for(
            WaiterKind::Message,
            timeout,
            move |ev| matches!(ev, model::Event::MessageCreate(m) if m.channel_id == channel),
        )
        .await
    }

    /// Wait for a reaction on a message (add/remove/remove-all/remove-emoji).
    ///
    /// # Errors
    /// Same as [`Self::wait_for`].
    pub async fn wait_for_reaction_on(
        &self,
        message: model::MessageId,
        timeout: Duration,
    ) -> Result<Arc<model::Event>, common::Error> {
        self.wait_for(WaiterKind::Reaction, timeout, move |ev| match ev {
            model::Event::ReactionAdd { message_id, .. }
            | model::Event::ReactionRemove { message_id, .. }
            | model::Event::ReactionRemoveAll { message_id, .. }
            | model::Event::ReactionRemoveEmoji { message_id, .. } => *message_id == message,
            _ => false,
        })
        .await
    }

    /// Wait for a modal submit with `custom_id`.
    /// Keys on `InteractionCreate` with `ModalSubmit` kind; if the model ever
    /// drops the variant, keying falls back to the event debug name (documented).
    ///
    /// # Errors
    /// Same as [`Self::wait_for`].
    pub async fn await_modal_submit(
        &self,
        custom_id: &str,
        timeout: Duration,
    ) -> Result<Arc<model::Event>, common::Error> {
        let want: Box<str> = Box::from(custom_id);
        self.wait_for(WaiterKind::Interaction, timeout, move |ev| {
            matches!(ev, model::Event::InteractionCreate(i)
                if i.kind == model::InteractionType::ModalSubmit
                    && i.custom_id.as_deref() == Some(want.as_ref()))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn predicate_filters() {
        let s = Arc::new(Standby::new());
        let ch_a = model::ChannelId::new(3).unwrap();
        let msg_b: model::Message =
            common::json::from_slice(br#"{"id":"50","channel_id":"30","author_id":"2","content":"b","timestamp":"2030-01-01T00:00:00Z"}"#)
                .unwrap();
        let ev_b = Arc::new(model::Event::MessageCreate(Arc::new(msg_b)));
        let msg_a: model::Message =
            common::json::from_slice(br#"{"id":"51","channel_id":"3","author_id":"2","content":"a","timestamp":"2030-01-01T00:00:00Z"}"#)
                .unwrap();
        let ev_a = Arc::new(model::Event::MessageCreate(Arc::new(msg_a)));
        let h = tokio::spawn({
            let s = s.clone();
            async move {
                s.wait_for_message_in(ch_a, Duration::from_millis(500))
                    .await
            }
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        s.feed(ev_b);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(!h.is_finished(), "filtered event must not resolve");
        s.feed(ev_a.clone());
        let got = h.await.unwrap().unwrap();
        assert!(Arc::ptr_eq(&got, &ev_a));
    }

    #[tokio::test]
    async fn timeout_evicts() {
        let s = Standby::new();
        let r = s
            .wait_for(WaiterKind::Message, Duration::from_millis(10), |_| true)
            .await;
        assert!(matches!(r, Err(common::Error::Timeout(_))));
        assert_eq!(s.evicted.load(Ordering::Relaxed), 1);
    }
}
