//! P10 REST ratelimit bench (bench-only harness, zero new deps).
//!
//! Exercises `rest::Client` against a hyper-less scripted mock (plain
//! `tokio::net::TcpListener`, same pattern as
//! `crates/rest/tests/ratelimit_conformance.rs`):
//! 1. `preemptive_wait`: bucket exhaust (`remaining=0`) delays the next call.
//! 2. `available_p99`: client-side queue overhead when the bucket is available
//!    (budget: p99 < 2ms, plans/05 §1). Breaches print `WARN`, never fail.
//! 3. `queue_depth`: K concurrent callers on one bucket all succeed.
//! 4. `shared_excluded`: shared-scope 429s never touch the ban meter.
//!
//! Run:
//! ```sh
//! cargo bench -p rivulus --bench rest_ratelimit
//! cargo bench -p rivulus --bench rest_ratelimit -- --quick --out bench/results/rest-local.json
//! ```

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Scripted mock: serves `script[i]` to connection `i`, then `200 {}`.
async fn spawn_script(script: Vec<String>, hits: Arc<AtomicUsize>) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock");
    let addr = listener.local_addr().expect("mock addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = listener.accept().await else {
                break;
            };
            let i = hits.fetch_add(1, Ordering::SeqCst);
            let raw = script.get(i).cloned().unwrap_or_else(|| {
                String::from("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            });
            tokio::spawn(async move {
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    let Ok(n) = tokio::io::AsyncReadExt::read(&mut s, &mut tmp).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    if buf.len() > 65536 {
                        break;
                    }
                }
                let _ = tokio::io::AsyncWriteExt::write_all(&mut s, raw.as_bytes()).await;
            });
        }
    });
    addr
}

fn resp(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut h = format!("{status}\r\n");
    for (k, v) in headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    h
}

fn test_client(addr: &SocketAddr) -> rest::Client {
    rest::Client::builder(secrecy::SecretString::from(String::from("bench-token")))
        .base_url(format!("http://{addr}"))
        .build()
        .expect("bench client builds (no IO)")
}

fn percentile(sorted_ms: &[f64], pct: f64) -> f64 {
    let n = sorted_ms.len();
    if n == 0 {
        return 0.0;
    }
    let idx = ((n as f64 * pct).floor() as usize)
        .saturating_sub(1)
        .min(n - 1);
    sorted_ms[idx]
}

/// Scenario 1: pre-emptive wait is honored (functional assert, generous margin).
async fn scenario_preemptive() -> f64 {
    let ok = resp(
        "HTTP/1.1 200 OK",
        &[
            ("X-RateLimit-Limit", "1"),
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset-After", "0.2"),
            ("Content-Type", "application/json"),
        ],
        "{}",
    );
    let ok2 = resp(
        "HTTP/1.1 200 OK",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![ok, ok2], hits.clone()).await;
    let client = test_client(&addr);
    let route = rest::Route::GetChannel { channel_id: 1 };
    client
        .execute(&route, None, None)
        .await
        .expect("first call");
    let start = Instant::now();
    client
        .execute(&route, None, None)
        .await
        .expect("second call");
    let waited_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert!(
        start.elapsed() >= Duration::from_millis(100),
        "pre-emptive wait not honored: {waited_ms:.1}ms"
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "both calls must hit the mock"
    );
    waited_ms
}

/// Scenario 2: per-request overhead with bucket available (measure only).
async fn scenario_available(n: usize) -> Vec<f64> {
    let ok = resp(
        "HTTP/1.1 200 OK",
        &[
            ("X-RateLimit-Limit", "5"),
            ("X-RateLimit-Remaining", "4"),
            ("X-RateLimit-Reset-After", "0.0"),
            ("Content-Type", "application/json"),
        ],
        "{}",
    );
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![ok], hits.clone()).await;
    let client = test_client(&addr);
    let route = rest::Route::GetChannel { channel_id: 2 };
    // Warmup (connection + bucket state) outside the sample.
    client.execute(&route, None, None).await.expect("warmup");
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let t = Instant::now();
        client
            .execute(&route, None, None)
            .await
            .expect("available call");
        out.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    assert_eq!(hits.load(Ordering::SeqCst), n + 1, "no call may be skipped");
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Scenario 3: K concurrent callers on one bucket (queue depth).
async fn scenario_queue_depth(k: usize) -> (Vec<f64>, usize, f64) {
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(Vec::new(), hits.clone()).await;
    let client = Arc::new(test_client(&addr));
    let wall = Instant::now();
    let mut tasks = Vec::with_capacity(k);
    for _ in 0..k {
        let c = client.clone();
        tasks.push(tokio::spawn(async move {
            let route = rest::Route::CreateMessage { channel_id: 5 };
            let t = Instant::now();
            c.execute(&route, Some(b"{}".to_vec()), None)
                .await
                .expect("queued call");
            t.elapsed().as_secs_f64() * 1000.0
        }));
    }
    let mut lat = Vec::with_capacity(k);
    for t in tasks {
        lat.push(t.await.expect("task joins"));
    }
    let wall_ms = wall.elapsed().as_secs_f64() * 1000.0;
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let hits_n = hits.load(Ordering::SeqCst);
    assert_eq!(hits_n, k, "every queued caller must be served");
    (lat, hits_n, wall_ms)
}

/// Scenario 4: shared-scope 429s never touch the ban meter (pure unit check).
fn scenario_shared_excluded() -> bool {
    let mut meter = rest::BanMeter::new();
    for _ in 0..9000 {
        meter.record(429, "shared");
    }
    let (warn, err) = meter.record(429, "user");
    !warn && !err
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn async_main() {
    let args: Vec<String> = std::env::args().collect();
    let quick = args.iter().any(|a| a == "--quick");
    let n_available = if quick { 50 } else { 200 };
    let depth = if quick { 8 } else { 32 };
    let out = args
        .windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| w[1].clone());

    const AVAILABLE_P99_BUDGET_MS: f64 = 2.0;

    let waited_ms = scenario_preemptive().await;
    println!("preemptive_wait: second call delayed {waited_ms:.1}ms (reset_after=200ms) — PASS");

    let avail = scenario_available(n_available).await;
    let avail_p50 = percentile(&avail, 0.50);
    let avail_p99 = percentile(&avail, 0.99);
    println!("available: n={n_available} p50={avail_p50:.3}ms p99={avail_p99:.3}ms (budget p99<{AVAILABLE_P99_BUDGET_MS:.1}ms)");
    let mut breached = false;
    if avail_p99 >= AVAILABLE_P99_BUDGET_MS {
        breached = true;
        println!("WARN: available p99 {avail_p99:.3}ms >= budget {AVAILABLE_P99_BUDGET_MS:.1}ms");
    }

    let (qlat, hits, wall_ms) = scenario_queue_depth(depth).await;
    let q_p50 = percentile(&qlat, 0.50);
    let q_p99 = percentile(&qlat, 0.99);
    println!("queue_depth: concurrency={depth} hits={hits} wall={wall_ms:.1}ms p50={q_p50:.3}ms p99={q_p99:.3}ms — PASS");

    let shared_ok = scenario_shared_excluded();
    assert!(shared_ok, "shared scope must stay out of the ban meter");
    println!("shared_excluded: 9000 shared 429s uncounted — PASS");

    if breached {
        println!(
            "WARN: p99 budget breached (tolerance-band CI compares artifacts; bench never fails)"
        );
    } else {
        println!("PASS: all scenarios within budget");
    }

    if let Some(path) = out {
        let doc = serde_json::json!({
            "bench": "rest_ratelimit",
            "unix_secs": unix_secs(),
            "budgets": {"available_p99_ms": AVAILABLE_P99_BUDGET_MS},
            "preemptive": {"wait_ms": waited_ms, "reset_after_ms": 200.0, "ok": waited_ms >= 100.0},
            "available": {"n": n_available, "p50_ms": avail_p50, "p99_ms": avail_p99,
                          "within_budget": avail_p99 < AVAILABLE_P99_BUDGET_MS},
            "queue_depth": {"concurrency": depth, "hits": hits, "wall_ms": wall_ms,
                            "p50_ms": q_p50, "p99_ms": q_p99},
            "shared_excluded": {"ok": shared_ok},
        });
        let text = serde_json::to_string_pretty(&doc).expect("json");
        std::fs::write(&path, text).unwrap_or_else(|_| panic!("cannot write {path}"));
        println!("wrote {path}");
    }
}

fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(4)
        .build()
        .expect("bench runtime")
        .block_on(async_main());
}
