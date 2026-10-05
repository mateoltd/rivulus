//! Cluster supervisor (N shards, ordered start, respawn, health).
//!
//! Identify ordering respects `max_concurrency` buckets:
//! `bucket(shard_id) = shard_id % max_concurrency` with 1 identify
//! per 5s per bucket. `spawn` supervises live shards with bounded
//! reconnects.

use std::collections::HashMap;
use std::sync::Arc;

use crate::SessionStore as _;

/// Per-shard health.
#[derive(Debug, Clone)]
pub struct ShardHealth {
    /// Shard identity.
    pub shard: crate::ShardId,
    /// Connected.
    pub connected: bool,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Last sequence number.
    pub seq: Option<u64>,
}

/// Cluster configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClusterConfig {
    /// Total shards.
    pub total: u32,
    /// Identify max concurrency.
    pub max_concurrency: u32,
}

impl ClusterConfig {
    /// New (`total`/`max_concurrency` at least 1).
    #[must_use]
    pub fn new(total: u32, max_concurrency: u32) -> Self {
        Self {
            total: total.max(1),
            max_concurrency: max_concurrency.max(1),
        }
    }
}

/// Recommended shard count from `GET /gateway/bot` (pure helper).
#[must_use]
pub fn recommended_shards(response: &model::GetGatewayBotResponse) -> u32 {
    response.shards.max(1)
}

/// Identify bucket for a shard: `shard_id % max_concurrency`.
#[must_use]
pub fn bucket(shard_id: u32, max_concurrency: u32) -> u32 {
    shard_id % max_concurrency.max(1)
}

/// Ordered-start list: `0..total` in identify order.
///
/// Natural numeric order already interleaves buckets (same-bucket shards
/// stay spaced), satisfying `1/5s/key` pacing when consumed with `bucket()`.
#[must_use]
pub fn ordered_start_list(total: u32, max_concurrency: u32) -> Vec<u32> {
    let total = total.max(1);
    let max_concurrency = max_concurrency.max(1);
    let mut ids: Vec<u32> = (0..total).collect();
    ids.sort_by_key(|id| (bucket(*id, max_concurrency), *id));
    interleave_by_bucket(ids, max_concurrency)
}

fn interleave_by_bucket(ids: Vec<u32>, max_concurrency: u32) -> Vec<u32> {
    let mut buckets: Vec<Vec<u32>> = Vec::new();
    buckets.resize_with(max_concurrency as usize, Vec::new);
    for id in ids {
        let b = bucket(id, max_concurrency) as usize;
        if let Some(slot) = buckets.get_mut(b) {
            slot.push(id);
        }
    }
    let mut out = Vec::new();
    loop {
        let mut progressed = false;
        for slot in &mut buckets {
            if let Some(id) = slot.first().copied() {
                slot.remove(0);
                out.push(id);
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    out
}

/// Shard identities for a config, in start order.
#[must_use]
pub fn shard_ids(config: &ClusterConfig) -> Vec<crate::ShardId> {
    ordered_start_list(config.total, config.max_concurrency)
        .into_iter()
        .map(|id| crate::ShardId {
            id,
            total: config.total,
        })
        .collect()
}

/// Shard cluster (supervises N shards; O(shards) tasks, never per-guild).
#[derive(Debug)]
pub struct Cluster {
    /// Shard identities.
    pub shards: Vec<crate::ShardId>,
    health: HashMap<u32, ShardHealth>,
}

impl Cluster {
    /// New cluster for shard ids.
    #[must_use]
    pub fn new(ids: Vec<crate::ShardId>) -> Self {
        let health = ids
            .iter()
            .map(|s| {
                (
                    s.id,
                    ShardHealth {
                        shard: *s,
                        connected: false,
                        latency_ms: 0,
                        seq: None,
                    },
                )
            })
            .collect();
        Self {
            shards: ids,
            health,
        }
    }
    /// New from config (ordered start).
    #[must_use]
    pub fn from_config(config: &ClusterConfig) -> Self {
        Self::new(shard_ids(config))
    }
    /// Mark a shard connected.
    pub fn mark_connected(&mut self, id: u32, latency_ms: u64) {
        if let Some(h) = self.health.get_mut(&id) {
            h.connected = true;
            h.latency_ms = latency_ms;
        }
    }
    /// Health snapshot.
    #[must_use]
    pub fn health(&self) -> Vec<ShardHealth> {
        self.health.values().cloned().collect()
    }
    /// Restart bookkeeping for a shard.
    pub fn restart(&mut self, id: u32) {
        if let Some(h) = self.health.get_mut(&id) {
            h.connected = false;
        }
    }
    /// Shared ownership helper.
    #[must_use]
    pub fn shared(ids: Vec<crate::ShardId>) -> Arc<std::sync::Mutex<Self>> {
        Arc::new(std::sync::Mutex::new(Self::new(ids)))
    }
    /// Record latest latency/seq for a shard (keeps `connected` as given).
    pub fn update(&mut self, id: u32, latency_ms: u64, seq: Option<u64>, connected: bool) {
        if let Some(h) = self.health.get_mut(&id) {
            h.latency_ms = latency_ms;
            h.seq = seq;
            h.connected = connected;
        }
    }
    /// Spawn one supervisor task per shard (ordered start, respawn on resume).
    ///
    /// Flow: `bootstrap.get_gateway_bot()` for `url` + `session_start_limit`
    /// (remaining `== 0` sleeps `reset_after` inside `Shard::run`), compute
    /// `ordered_start_list` by `bucket`, then spawn per-shard loops calling
    /// `Shard::run` with respawn on `Resume`/`BackoffResume`/`FreshIdentify`
    /// (bounded attempts, exp backoff `1s -> 120s` with jitter). `FailFast`
    /// and `Shutdown` stop the loop. Health is updated after each attempt;
    /// use `health()`/`restart()` on the returned shared cluster.
    pub async fn spawn<B>(
        config: ClusterConfig,
        auth: ClusterAuth,
        bootstrap: B,
        out: tokio::sync::mpsc::Sender<model::Event>,
    ) -> Result<
        (
            Arc<std::sync::Mutex<Self>>,
            Vec<tokio::task::JoinHandle<()>>,
        ),
        common::Error,
    >
    where
        B: crate::Bootstrap,
    {
        let bot = bootstrap.get_gateway_bot().await?;
        let mc_raw = bot.session_start_limit.max_concurrency.max(1);
        let mc = u32::try_from(mc_raw)
            .unwrap_or(1)
            .max(1)
            .max(config.max_concurrency);
        let total = config.total.max(1);
        let ids = ordered_start_list(total, mc);
        let shards: Vec<crate::ShardId> = ids
            .iter()
            .map(|id| crate::ShardId { id: *id, total })
            .collect();
        let cluster = Arc::new(std::sync::Mutex::new(Self::new(shards.clone())));
        let identify_queue = Arc::new(crate::InMemoryQueue::new(mc));
        let limit = Some(bot.session_start_limit.clone());
        let fallback = Some(bot.url.to_string());
        let base_override = auth.base_url.clone().or(fallback);
        let mut handles = Vec::new();
        for shard in shards {
            let cluster_c = cluster.clone();
            let queue_c = identify_queue.clone();
            let send_queue = Arc::new(crate::SendQueue::new());
            let store = Arc::new(crate::MemorySessionStore::new());
            let messenger = crate::ShardMessenger {
                shard,
                latency: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            };
            let messenger_c = messenger.clone();
            let token_c = auth.token.clone();
            let out_c = out.clone();
            let shutdown_c = auth.shutdown.clone();
            let limit_c = limit.clone();
            let base_c = base_override.clone();
            let intents_c = auth.intents;
            let h = tokio::spawn(async move {
                let mut attempt: u32 = 0;
                loop {
                    if shutdown_c.is_cancelled() {
                        break;
                    }
                    let cfg = crate::ShardConfig::new(token_c.clone(), intents_c, shard, 50);
                    let exit = crate::Shard::run(
                        cfg,
                        messenger_c.clone(),
                        store.clone(),
                        queue_c.clone(),
                        send_queue.clone(),
                        limit_c.clone(),
                        base_c.clone(),
                        out_c.clone(),
                        shutdown_c.clone(),
                    )
                    .await;
                    let latency = messenger_c.latency_ms();
                    let seq = store.snapshot().seq;
                    if let Ok(mut c) = cluster_c.lock() {
                        c.update(shard.id, latency, seq, false);
                    }
                    match exit {
                        crate::ShardExit::Shutdown => break,
                        crate::ShardExit::FailFast { help } => {
                            tracing::error!(shard = shard.id, help, "shard fail-fast");
                            break;
                        }
                        crate::ShardExit::Resume
                        | crate::ShardExit::BackoffResume
                        | crate::ShardExit::FreshIdentify => {
                            if attempt >= crate::MAX_RECONNECT_ATTEMPTS {
                                tracing::error!(shard = shard.id, "reconnect budget exhausted");
                                break;
                            }
                            let delay = crate::backoff_delay(attempt, crate::random_fract());
                            attempt = attempt.saturating_add(1);
                            tokio::select! {
                                _ = shutdown_c.cancelled() => break,
                                _ = tokio::time::sleep(delay) => {}
                            }
                        }
                    }
                }
            });
            handles.push(h);
        }
        Ok((cluster, handles))
    }
}

/// Auth + runtime options for [`Cluster::spawn`].
pub struct ClusterAuth {
    /// Bot token (never `Debug`-printed).
    pub token: secrecy::SecretString,
    /// Intents.
    pub intents: model::Intents,
    /// Base URL override for tests (`ws://127.0.0.1:port`); `None` uses
    /// `GET /gateway/bot` `url` (production `wss://gateway.discord.gg`).
    pub base_url: Option<String>,
    /// Shutdown token (5s drain: close 1000, cancel chunks, drain).
    pub shutdown: tokio_util::sync::CancellationToken,
}

impl std::fmt::Debug for ClusterAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClusterAuth")
            .field("intents", &self.intents)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_math() {
        assert_eq!(bucket(20, 16), 4);
        assert_eq!(bucket(0, 1), 0);
        assert_eq!(bucket(7, 4), 3);
    }

    #[test]
    fn recommended() {
        let resp = model::GetGatewayBotResponse {
            url: Box::from("wss://gateway.discord.gg"),
            shards: 9,
            session_start_limit: model::SessionStartLimit {
                total: 1000,
                remaining: 999,
                reset_after: 1,
                max_concurrency: 16,
            },
        };
        assert_eq!(recommended_shards(&resp), 9);
    }

    #[test]
    fn ordered_covers_all_once() {
        for (total, mc) in [(1, 1), (4, 2), (6, 4), (9, 16)] {
            let list = ordered_start_list(total, mc);
            assert_eq!(list.len() as u32, total.max(1));
            let mut sorted = list.clone();
            sorted.sort_unstable();
            let expected: Vec<u32> = (0..total.max(1)).collect();
            assert_eq!(sorted, expected);
        }
    }

    #[test]
    fn ordered_interleaves_buckets() {
        let list = ordered_start_list(4, 2);
        assert_eq!(list.len(), 4);
        assert_eq!(bucket(list[0], 2), 0);
        assert_eq!(bucket(list[1], 2), 1);
    }
}
