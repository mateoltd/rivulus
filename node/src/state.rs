//! Client registry: numeric monotonic handles over core clients.
//!
//! Handles are `u64` ids starting at 1, never recycled (ABA impossible by
//! construction). One subscription set, one listener, one pump task and one
//! threadsafe function per client at most: re-subscribing replaces the
//! previous listener. Forgetting `close` leaks the client; the test-only
//! [`Registry::live_count`] probe lets tests assert zero at process exit.
//!
//! The shared pump state is created before the core client so the dispatch
//! handler can close over it; the registry entry wraps the same state after
//! `insert`. No placeholder handlers, no patch-up pass.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::JsFunction;

/// Max events per threadsafe-function call (plan section 2.2).
const BATCH_CAP: usize = 64;
/// Max queued batches before oldest is dropped (bounded bridge).
const BRIDGE_CAP: usize = 256;
/// Batch window: collect arrivals for this long at most (plan 2.2).
const BATCH_WINDOW: Duration = Duration::from_millis(8);
/// Max queued threadsafe-function calls (plan: bounded bridge end to end).
const TSFN_QUEUE_CAP: usize = 16;

/// JSON batch queued for delivery to JS.
type Batch = String;

/// Pump state shared between the dispatch handler and the registry entry.
struct Shared {
    /// Subscribed event kinds (empty means no delivery).
    subs: Mutex<HashSet<String>>,
    /// Queued batches waiting for the pump task.
    bridge: Mutex<VecDeque<Batch>>,
    /// Wakeups for the pump task.
    notify: tokio::sync::Notify,
    /// Batches dropped at either bound (surfaced in `getHealth`).
    dropped: AtomicU64,
    /// Set on unsubscribe-last and close; ends the pump task.
    stopped: AtomicBool,
}

impl Shared {
    fn fresh() -> Arc<Self> {
        Arc::new(Self {
            subs: Mutex::new(HashSet::new()),
            bridge: Mutex::new(VecDeque::new()),
            notify: tokio::sync::Notify::new(),
            dropped: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
        })
    }

    fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Push one serialized event for subscribed kinds (drop-oldest bound).
    fn push_event(&self, kind: &str, batch: Batch) {
        let subscribed = self.subs.lock().map(|s| s.contains(kind)).unwrap_or(false);
        if !subscribed || self.is_stopped() {
            return;
        }
        if let Ok(mut bridge) = self.bridge.lock() {
            if bridge.len() >= BRIDGE_CAP {
                bridge.pop_front();
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
            bridge.push_back(batch);
        }
        self.notify.notify_one();
    }
}

/// Per-client entry behind a registry handle.
pub struct ClientEntry {
    /// Core client (shared with in-flight operations by `Arc`).
    pub client: Arc<rivulus::Client>,
    /// Shared pump state (also held by the dispatch handler).
    shared: Arc<Shared>,
    /// Active listener threadsafe function, if subscribed.
    tsfn: Mutex<Option<ThreadsafeFunction<String, ErrorStrategy::CalleeHandled>>>,
    /// Pump task handle (aborted on unsubscribe-last and close).
    pump: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Gateway base override for tests (`None` means production login).
    gateway_base: Option<String>,
    /// Set by `close` only (before removal); in-flight operations consult it.
    closed: AtomicBool,
}

impl ClientEntry {
    /// Whether `close` ran (delivery state is separate; see `stop_delivery`).
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Mark closed (close path only).
    pub fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }

    /// Dropped-batch count for `getHealth`.
    pub fn dropped_count(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Gateway base override clone for the login path.
    pub fn gateway_base(&self) -> Option<String> {
        self.gateway_base.clone()
    }

    /// Replace the subscription set.
    pub fn set_kinds(&self, kinds: Vec<String>) {
        if let Ok(mut subs) = self.shared.subs.lock() {
            subs.clear();
            subs.extend(kinds);
        }
    }

    /// Stop delivery: mark stopped, abort the pump, release the listener.
    fn stop_delivery(&self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        self.shared.notify.notify_waiters();
        if let Ok(mut pump) = self.pump.lock() {
            if let Some(handle) = pump.take() {
                handle.abort();
            }
        }
        if let Ok(mut tsfn) = self.tsfn.lock() {
            if let Some(tsfn) = tsfn.take() {
                let _ignored = tsfn.abort();
            }
        }
        if let Ok(mut subs) = self.shared.subs.lock() {
            subs.clear();
        }
    }

    /// Start (or restart) the pump task for the installed listener.
    fn start_pump(self: &Arc<Self>) {
        let this = Arc::clone(self);
        let handle = napi::bindgen_prelude::spawn(async move {
            loop {
                if this.shared.is_stopped() {
                    return;
                }
                tokio::time::timeout(BATCH_WINDOW, this.shared.notify.notified())
                    .await
                    .ok();
                if this.shared.is_stopped() {
                    return;
                }
                let mut batch: Vec<String> = Vec::new();
                if let Ok(mut bridge) = this.shared.bridge.lock() {
                    while batch.len() < BATCH_CAP {
                        match bridge.pop_front() {
                            Some(event) => batch.push(event),
                            None => break,
                        }
                    }
                }
                if batch.is_empty() {
                    continue;
                }
                let payload = format!("[{}]", batch.join(","));
                let status = this.tsfn.lock().map(|guard| {
                    guard
                        .as_ref()
                        .map(|tsfn| tsfn.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking))
                });
                if matches!(status, Ok(Some(napi::Status::QueueFull))) {
                    this.shared
                        .dropped
                        .fetch_add(batch.len() as u64, Ordering::Relaxed);
                }
            }
        });
        if let Ok(mut pump) = self.pump.lock() {
            if let Some(old) = pump.replace(handle) {
                old.abort();
            }
        }
    }
}

/// Handler feeding core dispatches into the bridge (registered at build).
struct PumpHandler {
    shared: Arc<Shared>,
}

impl rivulus::EventHandler for PumpHandler {
    fn on_dispatch(
        &self,
        _ctx: rivulus::Context,
        event: rivulus::model::Event,
    ) -> futures::future::BoxFuture<'_, ()> {
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            // `Event` itself is not `Serialize`; snapshot what M3 promises:
            // full message snapshots for creates, kind-only otherwise.
            let kind = event.kind().to_owned();
            let data = match &event {
                rivulus::model::Event::MessageCreate(message) => {
                    let snapshot = crate::snapshot::MessageSnapshot::of(message);
                    serde_json::json!({
                      "id": snapshot.id,
                      "channel_id": snapshot.channel_id,
                      "author_id": snapshot.author_id,
                      "content": snapshot.content,
                      "timestamp": snapshot.timestamp,
                    })
                }
                _ => serde_json::Value::Null,
            };
            let batch = serde_json::json!({ "kind": kind, "event": data }).to_string();
            shared.push_event(&kind, batch);
        })
    }
}

/// Global registry of live clients.
pub struct Registry {
    entries: Mutex<HashMap<u64, Arc<ClientEntry>>>,
    next: AtomicU64,
    live: AtomicU64,
}

impl Registry {
    /// Build a core client wired to fresh pump state, then insert it,
    /// returning the new handle.
    pub fn create(
        &self,
        token: secrecy::SecretString,
        intents: u32,
        cache: &str,
        gateway_base: Option<String>,
        rest_base: Option<String>,
    ) -> Result<u64, rivulus::common::Error> {
        let shared = Shared::fresh();
        let mut builder = rivulus::Client::builder(token)
            .intents(rivulus::model::Intents::from_bits_truncate(intents as u64))
            .sharding(rivulus::gateway::ShardStrategy::Auto)
            .add_handler(PumpHandler {
                shared: Arc::clone(&shared),
            });
        builder = match cache {
            "minimal" => builder.cache(|_| rivulus::cache::CacheConfig::minimal()),
            "full" => builder.cache(|_| rivulus::cache::CacheConfig::full()),
            _ => builder.cache(|_| rivulus::cache::CacheConfig::balanced()),
        };
        if let Some(base) = rest_base {
            builder = builder.base_url(base);
        }
        let client = builder.build()?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let entry = Arc::new(ClientEntry {
            client: Arc::new(client),
            shared,
            tsfn: Mutex::new(None),
            pump: Mutex::new(None),
            gateway_base,
            closed: AtomicBool::new(false),
        });
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(id, Arc::clone(&entry));
        }
        self.live.fetch_add(1, Ordering::Relaxed);
        Ok(id)
    }

    /// Look up a live entry by handle.
    pub fn get(&self, id: u64) -> Option<Arc<ClientEntry>> {
        self.entries.lock().ok()?.get(&id).cloned()
    }

    /// Remove an entry, returning it for shutdown. Unknown ids yield `None`.
    pub fn remove(&self, id: u64) -> Option<Arc<ClientEntry>> {
        let removed = self.entries.lock().ok()?.remove(&id);
        if removed.is_some() {
            self.live.fetch_sub(1, Ordering::Relaxed);
        }
        removed
    }

    /// Test-only live-handle count (never in `.d.ts`; exposed to JS in M4).
    #[cfg(test)]
    pub fn live_count(&self) -> u64 {
        self.live.load(Ordering::Relaxed)
    }
}

/// Install the listener threadsafe function for one subscription,
/// replacing any previous listener.
pub fn install_listener(entry: &Arc<ClientEntry>, listener: JsFunction) -> napi::Result<()> {
    let tsfn = listener
        .create_threadsafe_function::<String, String, _, ErrorStrategy::CalleeHandled>(
            TSFN_QUEUE_CAP,
            |ctx| Ok(vec![ctx.value]),
        )?;
    if let Ok(mut slot) = entry.tsfn.lock() {
        if let Some(old) = slot.replace(tsfn) {
            let _ignored = old.abort();
        }
    }
    entry.shared.stopped.store(false, Ordering::SeqCst);
    entry.start_pump();
    Ok(())
}

/// Global registry (one per process).
static REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();

/// Access the global registry.
pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(|| Registry {
        entries: Mutex::new(HashMap::new()),
        next: AtomicU64::new(1),
        live: AtomicU64::new(0),
    })
}

/// Stop delivery for an entry (unsubscribe-last and close paths).
pub fn stop_entry(entry: &Arc<ClientEntry>) {
    entry.stop_delivery();
}

#[cfg(test)]
mod tests {
    use super::Registry;

    fn local_registry() -> Registry {
        Registry {
            entries: std::sync::Mutex::new(std::collections::HashMap::new()),
            next: std::sync::atomic::AtomicU64::new(1),
            live: std::sync::atomic::AtomicU64::new(0),
        }
    }

    #[test]
    fn handles_are_monotonic_and_leak_probe_counts() {
        let registry = local_registry();
        let token = || secrecy::SecretString::from(String::from("mock-token"));
        let a = registry
            .create(token(), 0, "balanced", None, None)
            .expect("builds offline");
        let b = registry
            .create(token(), 0, "balanced", None, None)
            .expect("builds offline");
        assert!(b > a, "handles increase monotonically");
        assert_eq!(registry.live_count(), 2);
        assert!(registry.remove(999_999).is_none(), "unknown id, no panic");
        assert!(registry.remove(a).is_some());
        assert_eq!(registry.live_count(), 1);
        assert!(registry.remove(b).is_some());
        assert_eq!(registry.live_count(), 0);
    }
}
