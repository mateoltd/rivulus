//! Sharded TTL sweeper (`retain`, never blocks reads).
use std::sync::Arc;

use crate::Cache;

/// Sweeper config.
#[derive(Debug, Clone, Copy)]
pub struct SweeperConfig {
    /// Interval seconds.
    pub interval_secs: u64,
    /// Message max age seconds.
    pub message_max_age_secs: u64,
}

impl Default for SweeperConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            message_max_age_secs: 3600,
        }
    }
}

/// Message-TTL sweeper (v1: messages only).
pub struct Sweeper;

impl Sweeper {
    /// Sweep once (sharded `retain`, bounded per call, never held across
    /// `.await` - fully synchronous). Uses the cache's configured
    /// `message_max_age_secs`. Returns messages removed.
    #[must_use = "sweep count discarded"]
    pub fn sweep_once(cache: &crate::InMemoryCache) -> usize {
        let max_age = cache.config().message_max_age_secs;
        cache.sweep_messages(max_age)
    }

    /// Sweep once with an explicit max age (test hook).
    #[must_use = "sweep count discarded"]
    pub fn sweep_once_with(cache: &crate::InMemoryCache, max_age_secs: u64) -> usize {
        cache.sweep_messages(max_age_secs)
    }

    /// Spawn a background sweeper ticking every `cfg.interval_secs`.
    pub fn spawn(
        cache: Arc<crate::InMemoryCache>,
        cfg: SweeperConfig,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut t =
                tokio::time::interval(std::time::Duration::from_secs(cfg.interval_secs.max(1)));
            loop {
                t.tick().await;
                let _ = Self::sweep_once(&cache);
            }
        })
    }
}

/// Spawn a sweeper task over an `InMemoryCache` (message TTL only v1).
pub fn spawn(cache: Arc<crate::InMemoryCache>, cfg: SweeperConfig) -> tokio::task::JoinHandle<()> {
    Sweeper::spawn(cache, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::Cache;

    fn msg_from_json(v: &str) -> model::Message {
        common::json::from_slice::<model::Message>(v.as_bytes()).unwrap()
    }

    #[test]
    fn insert_expire_sweep() {
        let cfg = crate::CacheConfig {
            resource_types: crate::ResourceType::MESSAGE | crate::ResourceType::USER,
            message_limit: 10,
            message_max_age_secs: 3600,
            ..crate::CacheConfig::default()
        };
        let c = crate::InMemoryCache::new(cfg);
        let old = msg_from_json(
            r#"{"id":"10","channel_id":"3","author_id":"2","content":"old","timestamp":"2020-01-01T00:00:00Z"}"#,
        );
        let fresh = msg_from_json(
            r#"{"id":"11","channel_id":"3","author_id":"2","content":"new","timestamp":"2030-01-01T00:00:00Z"}"#,
        );
        c.insert_message(Arc::new(old));
        c.insert_message(Arc::new(fresh));
        assert_eq!(c.stats().messages, 2);
        let removed = Sweeper::sweep_once(&c);
        assert_eq!(removed, 1);
        assert_eq!(c.stats().messages, 1);
    }

    #[test]
    fn explicit_max_age_hook() {
        let cfg = crate::CacheConfig {
            resource_types: crate::ResourceType::MESSAGE,
            message_limit: 10,
            message_max_age_secs: 3600,
            ..crate::CacheConfig::default()
        };
        let c = crate::InMemoryCache::new(cfg);
        let old = msg_from_json(
            r#"{"id":"20","channel_id":"3","author_id":"2","content":"old","timestamp":"2020-01-01T00:00:00Z"}"#,
        );
        c.insert_message(Arc::new(old));
        // Huge budget keeps everything; zero budget expires everything old.
        assert_eq!(Sweeper::sweep_once_with(&c, i64::MAX as u64), 0);
        assert_eq!(c.stats().messages, 1);
        assert_eq!(Sweeper::sweep_once_with(&c, 0), 1);
        assert_eq!(c.stats().messages, 0);
    }

    #[test]
    fn config_default() {
        let cfg = SweeperConfig::default();
        assert_eq!((cfg.interval_secs, cfg.message_max_age_secs), (60, 3600));
    }

    #[tokio::test]
    async fn spawn_ticks_and_cancels() {
        let cfg = crate::CacheConfig {
            resource_types: crate::ResourceType::MESSAGE,
            message_limit: 10,
            message_max_age_secs: 3600,
            ..crate::CacheConfig::default()
        };
        let c = Arc::new(crate::InMemoryCache::new(cfg));
        let old = msg_from_json(
            r#"{"id":"30","channel_id":"3","author_id":"2","content":"old","timestamp":"2020-01-01T00:00:00Z"}"#,
        );
        c.insert_message(Arc::new(old));
        let scfg = SweeperConfig {
            interval_secs: 1,
            message_max_age_secs: 3600,
        };
        let h = Sweeper::spawn(c.clone(), scfg);
        // First tick is immediate: the stale message is swept quickly.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        while c.stats().messages != 0 {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(c.stats().messages, 0);
        h.abort();
        // Free-function wrapper spawns the same way; smoke + abort.
        let h2 = crate::sweeper::spawn(
            c,
            SweeperConfig {
                interval_secs: 1,
                message_max_age_secs: 3600,
            },
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        h2.abort();
    }
}
