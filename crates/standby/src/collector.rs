//! Collector with discord.js semantics (filter, max, time, idle, dispose).
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use futures::Future as _;

/// Why a collector ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EndReason {
    /// Time limit reached.
    Time,
    /// Idle timeout reached.
    Idle,
    /// Max items collected.
    Limit,
    /// Stopped by user.
    User,
    /// Message deleted.
    MessageDelete,
    /// Channel deleted.
    ChannelDelete,
    /// Guild deleted.
    GuildDelete,
    /// Thread deleted.
    ThreadDelete,
}

/// Collected item.
#[derive(Debug, Clone)]
pub struct Collected {
    /// Event that matched.
    pub event: Arc<model::Event>,
    /// When collected.
    pub at: Instant,
}

/// Collector configuration.
#[derive(Debug, Clone)]
pub struct CollectorConfig {
    /// Max items (None = unbounded, bounded by time).
    pub max: Option<usize>,
    /// Total time limit.
    pub time: Duration,
    /// Idle timeout (reset per item).
    pub idle: Option<Duration>,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            max: Some(100),
            time: Duration::from_secs(60),
            idle: None,
        }
    }
}

/// Shared event filter predicate.
type EventFilter = Arc<dyn Fn(&model::Event) -> bool + Send + Sync>;

/// Active collector (filter + max + time + idle + dispose; auto-ends on deletes).
pub struct Collector {
    cfg: CollectorConfig,
    filter: Option<EventFilter>,
    dispose: bool,
    items: Vec<Collected>,
    started: Instant,
    last_activity: Instant,
    ended: Option<EndReason>,
}

impl std::fmt::Debug for Collector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collector")
            .field("max", &self.cfg.max)
            .field("time", &self.cfg.time)
            .field("idle", &self.cfg.idle)
            .field("dispose", &self.dispose)
            .field("items", &self.items.len())
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

impl Collector {
    /// New collector (no filter, `dispose = false`).
    #[must_use]
    pub fn new(cfg: CollectorConfig) -> Self {
        let now = Instant::now();
        Self {
            cfg,
            filter: None,
            dispose: false,
            items: Vec::new(),
            started: now,
            last_activity: now,
            ended: None,
        }
    }

    /// New collector with a filter predicate.
    #[must_use]
    pub fn with_filter(
        cfg: CollectorConfig,
        filter: impl Fn(&model::Event) -> bool + Send + Sync + 'static,
    ) -> Self {
        let mut c = Self::new(cfg);
        c.filter = Some(Arc::new(filter));
        c
    }

    /// Replace the filter (`None` = accept all).
    pub fn set_filter(&mut self, filter: Option<EventFilter>) {
        self.filter = filter;
    }

    /// Enable/disable dispose (reaction-remove removes the matching collected add).
    pub fn set_dispose(&mut self, dispose: bool) {
        self.dispose = dispose;
    }

    /// Whether dispose is enabled.
    #[must_use]
    pub fn dispose(&self) -> bool {
        self.dispose
    }

    fn delete_reason(ev: &model::Event) -> Option<EndReason> {
        match ev {
            model::Event::MessageDelete { .. } | model::Event::MessageDeleteBulk { .. } => {
                Some(EndReason::MessageDelete)
            }
            model::Event::ChannelDelete(_) => Some(EndReason::ChannelDelete),
            model::Event::GuildDelete { .. } => Some(EndReason::GuildDelete),
            model::Event::ThreadDelete(_) => Some(EndReason::ThreadDelete),
            _ => None,
        }
    }

    /// Try to dispose (reaction-remove family). Returns `true` when `ev` was
    /// a dispose event and must NOT be collected (even when nothing matched).
    fn try_dispose(&mut self, ev: &model::Event) -> bool {
        if !self.dispose {
            return false;
        }
        match ev {
            model::Event::ReactionRemove {
                message_id,
                user_id,
                ..
            } => {
                self.items.retain(|c| {
                    !matches!(c.event.as_ref(),
                        model::Event::ReactionAdd { message_id: mid, user_id: uid, .. }
                        if *mid == *message_id && *uid == *user_id)
                });
                true
            }
            model::Event::ReactionRemoveAll { message_id, .. }
            | model::Event::ReactionRemoveEmoji { message_id, .. } => {
                self.items.retain(|c| {
                    !matches!(c.event.as_ref(),
                        model::Event::ReactionAdd { message_id: mid, .. }
                        if *mid == *message_id)
                });
                true
            }
            _ => false,
        }
    }

    /// Push an event; returns end reason when the collector completes.
    /// Filtered-out events are ignored (no collect, no idle reset).
    /// Delete events auto-end (`MessageDelete`/`ChannelDelete`/`GuildDelete`/
    /// `ThreadDelete`) even when filtered. Idle resets per collected item.
    pub fn push(&mut self, event: Arc<model::Event>) -> Option<EndReason> {
        if self.ended.is_some() {
            return self.ended;
        }
        if let Some(reason) = Self::delete_reason(event.as_ref()) {
            self.ended = Some(reason);
            return self.ended;
        }
        let now = Instant::now();
        if now.duration_since(self.started) >= self.cfg.time {
            self.ended = Some(EndReason::Time);
            return self.ended;
        }
        if let Some(idle) = self.cfg.idle {
            if now.duration_since(self.last_activity) >= idle {
                self.ended = Some(EndReason::Idle);
                return self.ended;
            }
        }
        if let Some(f) = &self.filter {
            if !(f)(event.as_ref()) {
                return self.ended;
            }
        }
        if self.try_dispose(event.as_ref()) {
            self.last_activity = now;
            return self.ended;
        }
        self.last_activity = now;
        self.items.push(Collected { event, at: now });
        if let Some(max) = self.cfg.max {
            if self.items.len() >= max {
                self.ended = Some(EndReason::Limit);
            }
        }
        self.ended
    }

    /// Collected items.
    #[must_use]
    pub fn items(&self) -> &[Collected] {
        &self.items
    }

    /// End reason when finished.
    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        self.ended
    }

    /// Stop manually.
    pub fn stop(&mut self, reason: EndReason) {
        self.ended = Some(reason);
    }

    fn time_remaining(&self) -> Duration {
        self.cfg
            .time
            .checked_sub(Instant::now().duration_since(self.started))
            .unwrap_or(Duration::ZERO)
    }

    fn idle_remaining(&self) -> Option<Duration> {
        self.cfg.idle.map(|d| {
            d.checked_sub(Instant::now().duration_since(self.last_activity))
                .unwrap_or(Duration::ZERO)
        })
    }
}

/// Streaming collector: `Stream<Item = Collected>` with idle-reset-per-item
/// and auto-end on delete events. Ends (`None`) on `Time`/`Idle`/`Limit`
/// (after yielding the limit item)/delete/`User`/channel close.
pub struct CollectorStream {
    coll: Collector,
    rx: futures::channel::mpsc::UnboundedReceiver<Arc<model::Event>>,
    time: Pin<Box<tokio::time::Sleep>>,
    idle: Option<Pin<Box<tokio::time::Sleep>>>,
    done: bool,
}

impl std::fmt::Debug for CollectorStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorStream")
            .field("items", &self.coll.items.len())
            .field("ended", &self.coll.ended)
            .finish_non_exhaustive()
    }
}

impl CollectorStream {
    /// Wrap a collector over a channel of events.
    #[must_use]
    pub fn new(
        coll: Collector,
        rx: futures::channel::mpsc::UnboundedReceiver<Arc<model::Event>>,
    ) -> Self {
        let time = Box::pin(tokio::time::sleep(coll.time_remaining()));
        let idle = coll
            .idle_remaining()
            .map(|d| Box::pin(tokio::time::sleep(d)));
        Self {
            coll,
            rx,
            time,
            idle,
            done: false,
        }
    }

    /// End reason when finished.
    #[must_use]
    pub fn end_reason(&self) -> Option<EndReason> {
        self.coll.end_reason()
    }

    /// Collected items so far.
    #[must_use]
    pub fn items(&self) -> &[Collected] {
        self.coll.items()
    }

    /// Stop manually.
    pub fn stop(&mut self, reason: EndReason) {
        self.coll.stop(reason);
    }
}

impl futures::Stream for CollectorStream {
    type Item = Collected;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.done {
            return Poll::Ready(None);
        }
        if let Some(r) = self.coll.end_reason() {
            if !matches!(r, EndReason::Limit) || self.coll.items.is_empty() {
                self.done = true;
                return Poll::Ready(None);
            }
        }
        if self.time.as_mut().poll(cx).is_ready() {
            self.coll.stop(EndReason::Time);
            self.done = true;
            return Poll::Ready(None);
        }
        if let Some(idle) = self.idle.as_mut() {
            if idle.as_mut().poll(cx).is_ready() {
                self.coll.stop(EndReason::Idle);
                self.done = true;
                return Poll::Ready(None);
            }
        }
        match Pin::new(&mut self.rx).poll_next(cx) {
            Poll::Ready(Some(ev)) => {
                let before = self.coll.items.len();
                let end = self.coll.push(ev);
                let after = self.coll.items.len();
                if after > before {
                    if let Some(idle_dur) = self.coll.cfg.idle {
                        self.idle = Some(Box::pin(tokio::time::sleep(idle_dur)));
                    }
                    let item = self.coll.items[after.saturating_sub(1)].clone();
                    if end == Some(EndReason::Limit) {
                        self.done = true;
                    }
                    Poll::Ready(Some(item))
                } else if let Some(_reason) = end {
                    self.done = true;
                    Poll::Ready(None)
                } else {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
            }
            Poll::Ready(None) => {
                self.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limit_ends() {
        let mut c = Collector::new(CollectorConfig {
            max: Some(2),
            time: Duration::from_secs(60),
            idle: None,
        });
        let e = Arc::new(model::Event::Resumed);
        assert_eq!(c.push(e.clone()), None);
        assert_eq!(c.push(e), Some(EndReason::Limit));
    }
    #[test]
    fn idle_ends() {
        let mut c = Collector::new(CollectorConfig {
            max: None,
            time: Duration::from_secs(60),
            idle: Some(Duration::from_millis(1)),
        });
        std::thread::sleep(Duration::from_millis(5));
        let e = Arc::new(model::Event::Resumed);
        assert_eq!(c.push(e), Some(EndReason::Idle));
    }
    #[test]
    fn filter_blocks() {
        let mut c = Collector::with_filter(
            CollectorConfig {
                max: None,
                time: Duration::from_secs(60),
                idle: None,
            },
            |ev| matches!(ev, model::Event::Resumed),
        );
        let other = Arc::new(model::Event::TypingStart {
            channel_id: model::ChannelId::new(1).unwrap(),
            user_id: model::UserId::new(2).unwrap(),
        });
        assert_eq!(c.push(other), None);
        assert!(c.items().is_empty());
        assert_eq!(c.push(Arc::new(model::Event::Resumed)), None);
        assert_eq!(c.items().len(), 1);
    }
    #[test]
    fn timeout_ends() {
        let mut c = Collector::new(CollectorConfig {
            max: None,
            time: Duration::from_millis(1),
            idle: None,
        });
        std::thread::sleep(Duration::from_millis(5));
        let e = Arc::new(model::Event::Resumed);
        assert_eq!(c.push(e), Some(EndReason::Time));
    }
    #[test]
    fn delete_end_reasons() {
        let ch = model::ChannelId::new(3).unwrap();
        let msg = model::MessageId::new(4).unwrap();
        let gid = model::GuildId::new(5).unwrap();
        let mut c = Collector::new(CollectorConfig::default());
        assert_eq!(
            c.push(Arc::new(model::Event::MessageDelete {
                channel_id: ch,
                message_id: msg,
            })),
            Some(EndReason::MessageDelete)
        );
        let mut c = Collector::new(CollectorConfig::default());
        let thread = model::Thread {
            id: ch,
            parent_id: None,
            guild_id: None,
            name: None,
        };
        assert_eq!(
            c.push(Arc::new(model::Event::ThreadDelete(Arc::new(thread)))),
            Some(EndReason::ThreadDelete)
        );
        let mut c = Collector::new(CollectorConfig::default());
        assert_eq!(
            c.push(Arc::new(model::Event::GuildDelete {
                guild_id: gid,
                unavailable: false,
            })),
            Some(EndReason::GuildDelete)
        );
    }
    #[test]
    fn dispose_removes() {
        let mut c = Collector::new(CollectorConfig {
            max: None,
            time: Duration::from_secs(60),
            idle: None,
        });
        c.set_dispose(true);
        let ch = model::ChannelId::new(3).unwrap();
        let msg = model::MessageId::new(4).unwrap();
        let user = model::UserId::new(2).unwrap();
        let add = Arc::new(model::Event::ReactionAdd {
            channel_id: ch,
            message_id: msg,
            user_id: user,
            emoji: Box::from("👍"),
        });
        assert_eq!(c.push(add), None);
        assert_eq!(c.items().len(), 1);
        let remove = Arc::new(model::Event::ReactionRemove {
            channel_id: ch,
            message_id: msg,
            user_id: user,
        });
        assert_eq!(c.push(remove), None);
        assert!(c.items().is_empty());
    }
}
