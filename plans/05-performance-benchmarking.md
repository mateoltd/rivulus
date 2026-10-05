# 05 — Performance & Benchmarking (honest alloc story)

## 1. Budgets
- RSS <=20% of djs equiv on matrix (quick CI 100 guilds; nightly 1k/10k/50k x msg-only/full-cache). Per-guild table MEASURED (entry overhead ~120B + payload documented after P10; no 8KB promise until measured).
- p99 dispatch (WS bytes -> handler) <5ms local replay; REST client-side queueing p99 <2ms when bucket available.
- Idle tasks: O(shards) — read-loop + heartbeat + dispatch per shard + 1 sweeper + 1 supervisor; ZERO per-guild/per-channel tasks.

## 2. Alloc rules (realistic: owned Event, fewer/smaller allocs — not zero-copy)
- `model::Id<T> = #[repr(transparent)] NonZeroU64`; `FromStr` fast parse; `Display` via `itoa`.
- `Event: 'static` owned. Borrowed `&str` ONLY for transient header peek (`op/t/s` via `common::json::RawValue`); never across `.await`.
- WS baseline `tokio-tungstenite` returns owned `Message`; accept 1 alloc/msg, optimize shape (no `Value`, `from_slice` direct to `Event`, `Bytes` reuse only if P3 spike switches crate — single stack).
- Model strings: `Box<str>` cold; `Arc<str>` only where flame shows sharing wins. `SmallVec<[T;4]>` embeds/components/overwrites. `bitflags` intents/permissions. Forward-compat via manual `Unknown(u8)` impls.
- Cache `Arc<T>` sharing + canonical `users` map (04C); diff clones `Arc` only. REST builders serialize direct `to_vec`, no `Value`. Routes built with const format + `itoa`, no `String` concat in hot path.
- JSON locked `serde_json` via `common::json` shim (`from_slice/to_vec/RawValue`). No simd v1.
- Backpressure (no silent drop): per-shard `mpsc(1024)` + handler `broadcast(2048)`; `send().await` timeout => `common::Error::ShardBackpressure`. `Lagged(n)` policy per kind: cacheable (message/guild/member) => refetch-by-id + `warn!`; ephemeral (typing/presence/reaction-remove) => drop + count `standby_lag_total`; `Unknown` => count only. `broadcast` overflow never reconstructs ephemeral events from cache (impossible — documented). P5 `tests/dispatcher_lag.rs` proves it. Standby: cap 1024 waiters/shard + evict on timeout.
- Decompression cap 256 KiB/frame; error `source` truncated to 500 chars in logs (no 100KB GUILD_CREATE in tracing).
- Metrics allowlist ONLY: `gateway_events_total, gateway_resumes_total, gateway_unknown_events_total, rest_requests_total{route_template,status}, rest_429_total{scope}, cache_hit_ratio, ws_latency_ms, standby_lag_total` — `route_template` from allowlist, never bucket hash. Per-event spans at `trace!`, dispatch summary at `debug!` (never per-event `info!`).

## 3. Cache tunables (measured table template; P10 fills numbers)
| preset (builder fn) | resources | target |
|---|---|---|
| `minimal` (default) | guild,channel,role,self-user | lowest |
| `balanced` | +member(no presence),thread,voice-state | mid |
| `full` | +message(50/ch),reaction,presence | warn: high |
Sweeper: interval 60s, msg max_age 1h, typing 10s. Docs show djs `makeCache/sweepers` 1:1 mapping. Hasher `foldhash` documented.

## 4. Bench harness (ships P10; quick vs nightly)
- `benches/replay.rs` (criterion) with BENCH-ONLY counting allocator (`stats-alloc` or own `CountingAlloc` in `benches/`, never in lib): reports `allocs/event + bytes/event` on replay corpus. `common::json` shim CANNOT count allocs — documented.
- `benches/rest_ratelimit.rs`: mock hyper with bucket headers; asserts pre-emptive wait + queue depth (shared-scope excluded).
- `bench/djs-equiv/` (Node 20 + djs latest): identical bot; RSS via `/proc/self/status` (both); matrix quick/nightly; output `bench/results/<date>.md` + flame notes.
- Perf CI: `cargo bench -- --quick` + tolerance-band compare to baseline artifact (not absolute RSS). Graph in README.

## 5. Pitfalls (fail review if present)
Duplicated User; full-guild clone on MemberUpdate; pretty JSON; per-event Client construction; global `RwLock<HashMap>`; unbounded retry queues; `try_send`+drop; untruncated error sources; high-cardinality bucket labels; second HTTP/WS/JSON stack.

