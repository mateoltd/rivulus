//! Short soak (fast CI): seeded chaos fan-out, resume-rate + p99 + 0 panics.
//!
//! NOT the 10min `tests/soak.rs --short` from `plans/07`/`plans/08` P8;
//! this is a deterministic <30s approximation for every-CI signal:
//! N tasks fan events through [`rivulus::Dispatcher`] + standby feed +
//! rapid `MemorySessionStore` connect/disconnect cycles. Asserts the soak
//! SLO from `plans/06` §2 (>=99% resume-ok, 0 panics, p99 resume <15s).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gateway::SessionStore;

/// Deterministic LCG (no `rand` dep): `state = a*state + c`.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 33) & 0xFFFF_FFFF
    }

    fn bound(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

fn test_ctx() -> rivulus::Context {
    let http = Arc::new(
        rest::Client::builder(secrecy::SecretString::from(String::from("soak")))
            .build()
            .expect("rest build"),
    );
    let cache: Arc<dyn cache::Cache> =
        Arc::new(cache::InMemoryCache::new(cache::CacheConfig::minimal()));
    #[cfg(feature = "standby")]
    let standby = Arc::new(standby::Standby::new());
    rivulus::Context {
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

fn message_event(id: u64, channel: u64) -> model::Event {
    let v = format!(
        r#"{{"id":"{id}","channel_id":"{channel}","author_id":"2","content":"soak","timestamp":"2030-01-01T00:00:00Z"}}"#
    );
    let m: model::Message = common::json::from_slice(v.as_bytes()).expect("message");
    model::Event::MessageCreate(Arc::new(m))
}

#[tokio::test]
async fn soak_short_seeded_chaos() {
    const TASKS: usize = 8;
    const EVENTS_PER_TASK: usize = 200;

    let dispatcher = Arc::new(rivulus::Dispatcher::new(Vec::new()));
    let metrics = Arc::new(gateway::Metrics::new());
    let resume_ok = Arc::new(AtomicU64::new(0));
    let resume_attempts = Arc::new(AtomicU64::new(0));
    let latencies: Arc<std::sync::Mutex<Vec<Duration>>> =
        Arc::new(std::sync::Mutex::new(Vec::new()));

    let mut handles = Vec::new();
    for task in 0..TASKS {
        let dispatcher = dispatcher.clone();
        let metrics = metrics.clone();
        let resume_ok = resume_ok.clone();
        let resume_attempts = resume_attempts.clone();
        let latencies = latencies.clone();
        handles.push(tokio::spawn(async move {
            let ctx = test_ctx();
            // Per-task session store: rapid connect/disconnect cycles.
            let store = gateway::MemorySessionStore::new();
            let mut rng = Lcg(0x9E37_79B9_7F4A_7C15u64.wrapping_add(task as u64 * 0x1000_0001));
            for i in 0..EVENTS_PER_TASK {
                let pick = rng.bound(100);
                let id = (task as u64) * 1_000_000 + i as u64 + 1;
                let ev = if pick < 70 {
                    message_event(id + 100, rng.bound(8) + 1)
                } else if pick < 85 {
                    model::Event::TypingStart {
                        channel_id: model::ChannelId::new(rng.bound(8) + 1).expect("channel"),
                        user_id: model::UserId::new(rng.bound(5000) + 1).expect("user"),
                    }
                } else {
                    metrics.report_unknown("FUTURE_KIND");
                    model::Event::unknown("FUTURE_KIND", Some(id), "{}")
                };
                metrics.inc_event();
                // Fan-out through cache -> standby -> handlers (+ broadcast).
                dispatcher.dispatch(&ctx, ev).await;
                #[cfg(feature = "standby")]
                ctx.standby.feed(Arc::new(model::Event::Resumed));

                // Rapid connect/disconnect: READY -> seq -> resume check ->
                // clear (fresh identify) every 25 events.
                if i % 25 == 0 {
                    store.clear();
                }
                store.set_ready(
                    Box::from(format!("sess-{task}")),
                    Box::from("wss://resume.test/"),
                );
                let seq = id;
                store.set_seq(seq);
                let start = Instant::now();
                let snap = store.snapshot();
                let can = snap.can_resume();
                debug_assert_eq!(snap.gateway_url("fallback"), "wss://resume.test/");
                let elapsed = start.elapsed();
                resume_attempts.fetch_add(1, Ordering::Relaxed);
                if can {
                    resume_ok.fetch_add(1, Ordering::Relaxed);
                    metrics.inc_resume();
                }
                if let Ok(mut v) = latencies.lock() {
                    v.push(elapsed);
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("soak task panicked (0-panic SLO)");
    }

    let attempts = resume_attempts.load(Ordering::Relaxed);
    let ok = resume_ok.load(Ordering::Relaxed);
    assert!(attempts > 0, "soak must attempt resumes");
    let rate_pct = ok * 100 / attempts.max(1);
    assert!(
        rate_pct >= 99,
        "resume-rate SLO >=99%: {ok}/{attempts} ({rate_pct}%)"
    );

    let mut lat = latencies.lock().map(|v| v.clone()).unwrap_or_default();
    assert!(!lat.is_empty(), "must record resume timings");
    lat.sort_unstable();
    let p99 = lat[(lat.len() * 99 / 100).min(lat.len() - 1)];
    assert!(
        p99 < Duration::from_secs(15),
        "p99 resume {p99:?} must be <15s"
    );

    let (_, _, unknown) = metrics.snapshot();
    assert!(unknown > 0, "chaos must include unknown dispatches");
}
