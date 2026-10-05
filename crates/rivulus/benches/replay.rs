//! P10 replay bench (bench-only harness, zero new deps).
//!
//! Measures the WS-bytes -> owned-value path on the `tests/fixtures/` corpus:
//! borrowed header peek (`common::json::Header`, never across `.await`) +
//! full owned parse (`common::json`) + owned [`model::Event`] wrap.
//!
//! Reports per case: input bytes, `allocs/event`, allocated `bytes/event`,
//! net-live `bytes/event`, mean/p50/p99 dispatch ms, throughput.
//! Budgets (plans/05): replay p99 < 5ms. Breaches print `WARN`, never fail
//! (tolerance-band CI compares JSON artifacts via `bench/compare.py`).
//!
//! The counting allocator lives ONLY in this file (bench-only, never in lib:
//! every lib crate is `#![forbid(unsafe_code)]`, and `GlobalAlloc` needs
//! `unsafe impl`). `common::json` itself cannot count allocs (plans/05 §4);
//! counting happens at the allocator around each replay step.
//!
//! Run:
//! ```sh
//! cargo bench -p rivulus --bench replay
//! cargo bench -p rivulus --bench replay -- --quick --out bench/results/local.json
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Bench-only counting allocator (never in lib).
struct CountingAlloc;

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static REALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: forwards to the system allocator with the same layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: `ptr`/`layout` come from a matching `alloc`/`realloc`.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        REALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size, Ordering::Relaxed);
        DEALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: forwards to the system allocator with matching args.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn reset_counters() {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    REALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    DEALLOC_BYTES.store(0, Ordering::Relaxed);
}

#[derive(Debug, Clone, Copy)]
struct AllocSample {
    allocs: usize,
    reallocs: usize,
    bytes: usize,
    net_bytes: usize,
}

fn sample() -> AllocSample {
    let bytes = ALLOC_BYTES.load(Ordering::Relaxed);
    let freed = DEALLOC_BYTES.load(Ordering::Relaxed);
    AllocSample {
        allocs: ALLOC_COUNT.load(Ordering::Relaxed),
        reallocs: REALLOC_COUNT.load(Ordering::Relaxed),
        bytes,
        net_bytes: bytes.saturating_sub(freed),
    }
}

/// Load a workspace `tests/fixtures/` file (same layout as
/// `crates/model/tests/roundtrip.rs`: `CARGO_MANIFEST_DIR` = `crates/rivulus`).
fn fx(name: &str) -> Vec<u8> {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("tests/fixtures");
    p.push(name);
    std::fs::read(&p).unwrap_or_else(|_| panic!("missing fixture {}", p.display()))
}

/// Wrap raw payload bytes in a gateway dispatch envelope.
fn envelope(t: &str, seq: u64, d: &[u8]) -> Vec<u8> {
    let head = format!(r#"{{"op":0,"s":{seq},"t":"{t}","d":"#);
    let mut v = Vec::with_capacity(head.len() + d.len() + 1);
    v.extend_from_slice(head.as_bytes());
    v.extend_from_slice(d);
    v.push(b'}');
    v
}

/// Borrowed header peek (transient; never across `.await`), then full owned parse.
fn peek(bytes: &[u8]) -> common::json::Header<'_> {
    common::json::from_slice(bytes).expect("header peek must parse")
}

fn replay_message(bytes: &[u8]) {
    let h = peek(bytes);
    let m: model::Message = common::json::from_raw(h.d).expect("message");
    black_box(model::Event::MessageCreate(Arc::new(m)));
}

fn replay_guild(bytes: &[u8]) {
    let h = peek(bytes);
    let g: model::Guild = common::json::from_raw(h.d).expect("guild");
    black_box(model::Event::GuildCreate(Arc::new(g)));
}

fn replay_member(bytes: &[u8]) {
    let h = peek(bytes);
    let m: model::Member = common::json::from_raw(h.d).expect("member");
    black_box(model::Event::MemberAdd(Arc::new(m)));
}

fn replay_channel(bytes: &[u8]) {
    let h = peek(bytes);
    let c: model::Channel = common::json::from_raw(h.d).expect("channel");
    black_box(model::Event::ChannelCreate(Arc::new(c)));
}

fn replay_role(bytes: &[u8]) {
    let h = peek(bytes);
    let r: model::Role = common::json::from_raw(h.d).expect("role");
    let ev = model::Event::RoleCreate {
        guild_id: model::GuildId::new(1).expect("guild id"),
        role: Arc::new(r),
    };
    black_box(ev);
}

fn replay_chunk(bytes: &[u8]) {
    let h = peek(bytes);
    let c: model::GuildMembersChunk = common::json::from_raw(h.d).expect("chunk");
    black_box(model::Event::MembersChunk(Arc::new(c)));
}

fn replay_interaction(bytes: &[u8]) {
    let h = peek(bytes);
    let i: model::Interaction = common::json::from_raw(h.d).expect("interaction");
    black_box(model::Event::InteractionCreate(Arc::new(i)));
}

fn parse_component(bytes: &[u8]) {
    let c: model::Component = common::json::from_slice(black_box(bytes)).expect("component");
    black_box(c);
}

fn parse_audit(bytes: &[u8]) {
    let a: model::AuditEntry = common::json::from_slice(black_box(bytes)).expect("audit");
    black_box(a);
}

fn parse_gateway_bot(bytes: &[u8]) {
    let g: model::GetGatewayBotResponse =
        common::json::from_slice(black_box(bytes)).expect("gateway bot");
    black_box(g);
}

fn parse_permissions(bytes: &[u8]) {
    #[derive(serde::Deserialize)]
    struct W {
        p: model::Permissions,
    }
    let w: W = common::json::from_slice(black_box(bytes)).expect("permissions");
    black_box(w.p);
}

struct Case {
    name: &'static str,
    input: Vec<u8>,
    run: fn(&[u8]),
}

fn corpus() -> Vec<Case> {
    let mut seq = 1_u64;
    let mut dispatch = |label: &'static str, t: &'static str, fixture: &str, run: fn(&[u8])| {
        seq += 1;
        Case {
            name: label,
            input: envelope(t, seq, &fx(fixture)),
            run,
        }
    };
    // Dispatch group: envelope + header peek + owned parse + `Event` wrap.
    let mut cases = vec![
        dispatch(
            "MESSAGE_CREATE",
            "MESSAGE_CREATE",
            "message.json",
            replay_message,
        ),
        dispatch("GUILD_CREATE", "GUILD_CREATE", "guild.json", replay_guild),
        dispatch(
            "GUILD_MEMBER_ADD",
            "GUILD_MEMBER_ADD",
            "member.json",
            replay_member,
        ),
        dispatch(
            "CHANNEL_CREATE",
            "CHANNEL_CREATE",
            "channel.json",
            replay_channel,
        ),
        dispatch(
            "CHANNEL_CREATE(unknown kind)",
            "CHANNEL_CREATE",
            "channel_unknown.json",
            replay_channel,
        ),
        dispatch(
            "GUILD_ROLE_CREATE",
            "GUILD_ROLE_CREATE",
            "role.json",
            replay_role,
        ),
        dispatch(
            "GUILD_MEMBERS_CHUNK",
            "GUILD_MEMBERS_CHUNK",
            "chunk.json",
            replay_chunk,
        ),
        dispatch(
            "INTERACTION_CREATE",
            "INTERACTION_CREATE",
            "interaction.json",
            replay_interaction,
        ),
    ];
    // Resource group: raw owned parse (REST shapes / sub-resources, no envelope).
    cases.push(Case {
        name: "resource:Component",
        input: fx("components_v2.json"),
        run: parse_component,
    });
    cases.push(Case {
        name: "resource:AuditEntry",
        input: fx("audit.json"),
        run: parse_audit,
    });
    cases.push(Case {
        name: "resource:GetGatewayBotResponse",
        input: fx("gateway_bot.json"),
        run: parse_gateway_bot,
    });
    cases.push(Case {
        name: "resource:Permissions",
        input: fx("permissions_str.json"),
        run: parse_permissions,
    });
    cases
}

struct CaseResult {
    name: String,
    input_bytes: usize,
    allocs_per_event: f64,
    reallocs_per_event: f64,
    alloc_bytes_per_event: f64,
    net_bytes_per_event: f64,
    mean_ms: f64,
    p50_ms: f64,
    p99_ms: f64,
    events_per_sec: f64,
}

fn percentile(sorted_nanos: &[u64], pct: f64) -> f64 {
    let n = sorted_nanos.len();
    if n == 0 {
        return 0.0;
    }
    let idx = ((n as f64 * pct).floor() as usize)
        .saturating_sub(1)
        .min(n - 1);
    sorted_nanos[idx] as f64 / 1_000_000.0
}

fn bench_case(case: &Case, iters: usize, alloc_iters: usize, warmup: usize) -> CaseResult {
    for _ in 0..warmup {
        (case.run)(&case.input);
    }
    // Alloc sample (amortized over `alloc_iters` runs to skip one-time init).
    reset_counters();
    for _ in 0..alloc_iters {
        (case.run)(black_box(&case.input));
    }
    let a = sample();
    let n = alloc_iters as f64;
    // Timing sample (per-iteration wall time).
    let mut nanos: Vec<u64> = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        (case.run)(black_box(&case.input));
        nanos.push(t.elapsed().as_nanos() as u64);
    }
    nanos.sort_unstable();
    let sum: u128 = nanos.iter().map(|&x| u128::from(x)).sum();
    let mean_ms = sum as f64 / nanos.len() as f64 / 1_000_000.0;
    CaseResult {
        name: case.name.to_string(),
        input_bytes: case.input.len(),
        allocs_per_event: a.allocs as f64 / n,
        reallocs_per_event: a.reallocs as f64 / n,
        alloc_bytes_per_event: a.bytes as f64 / n,
        net_bytes_per_event: a.net_bytes as f64 / n,
        mean_ms,
        p50_ms: percentile(&nanos, 0.50),
        p99_ms: percentile(&nanos, 0.99),
        events_per_sec: 1_000_000_000.0 / (sum as f64 / nanos.len() as f64),
    }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let quick = args.iter().any(|a| a == "--quick");
    let (iters, alloc_iters, warmup) = if quick {
        (200, 50, 20)
    } else {
        (2000, 200, 100)
    };
    let out = args
        .windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| w[1].clone());

    const P99_BUDGET_MS: f64 = 5.0;
    let mut results = Vec::new();
    for case in &corpus() {
        results.push(bench_case(case, iters, alloc_iters, warmup));
    }

    println!(
        "{:<34} {:>7} {:>8} {:>10} {:>10} {:>10} {:>10} {:>12}",
        "case", "in B", "alloc/ev", "B/ev", "net B/ev", "mean ms", "p99 ms", "ev/s"
    );
    let mut breached = false;
    for r in &results {
        println!(
            "{:<34} {:>7} {:>8.1} {:>10.0} {:>10.0} {:>10.4} {:>10.4} {:>12.0}",
            r.name,
            r.input_bytes,
            r.allocs_per_event,
            r.alloc_bytes_per_event,
            r.net_bytes_per_event,
            r.mean_ms,
            r.p99_ms,
            r.events_per_sec
        );
        if r.p99_ms >= P99_BUDGET_MS {
            breached = true;
            println!(
                "WARN: {} p99 {:.3}ms >= budget {:.1}ms",
                r.name, r.p99_ms, P99_BUDGET_MS
            );
        }
    }
    if breached {
        println!(
            "WARN: p99 budget breached (tolerance-band CI compares artifacts; bench never fails)"
        );
    } else {
        println!("PASS: all cases p99 < {P99_BUDGET_MS:.1}ms");
    }

    if let Some(path) = out {
        let cases: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "input_bytes": r.input_bytes,
                    "allocs_per_event": r.allocs_per_event,
                    "reallocs_per_event": r.reallocs_per_event,
                    "alloc_bytes_per_event": r.alloc_bytes_per_event,
                    "net_bytes_per_event": r.net_bytes_per_event,
                    "mean_ms": r.mean_ms,
                    "p50_ms": r.p50_ms,
                    "p99_ms": r.p99_ms,
                    "events_per_sec": r.events_per_sec,
                })
            })
            .collect();
        let doc = serde_json::json!({
            "bench": "replay",
            "unix_secs": unix_secs(),
            "iters": iters,
            "budgets": {"p99_ms": P99_BUDGET_MS},
            "cases": cases,
        });
        let text = serde_json::to_string_pretty(&doc).expect("json");
        std::fs::write(&path, text).unwrap_or_else(|_| panic!("cannot write {path}"));
        println!("wrote {path}");
    }
}
