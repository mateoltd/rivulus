# 09 — Locked Decisions (do not re-debate without RFC)

Status: LOCKED on 2026-09-26 per owner directive. Agents implement as-is.

## D1 Namespace / crates (locked)
Project is `rivulus`. Do NOT prefix every namespace with `rivulus-`.
- Cargo **package** names keep prefix for crates.io uniqueness (`rivulus-gateway`, …).
- Rust **lib / import** names are SHORT technical words (`gateway`, `rest`, `model`, `common`, `cache`, …).
- Facade crate `rivulus` re-exports all: `use rivulus::gateway::Shard`, `use rivulus::common::Error`.
- Direct dep also works: `gateway = { version, package = "rivulus-gateway" }` → `use gateway::Shard`.
- Crate dirs: `crates/common model gateway rest cache standby interactions framework voice rivulus`.
- Forbidden: hyphen-prefixed package paths in imports (e.g. prefixing `common` or `model::Error` with `rivulus-`; error lives in `common`), stuttered `rivulus::rivulus`.

```toml
[package] name = "rivulus-gateway"
[lib] name = "gateway"
```

## D2 HTTP (locked, not deferred)
- `reqwest 0.12` + `rustls-tls-manual-roots` + `webpki-roots 0.26` wired in builder (manual-roots ships NO roots). Single `Client` per `rest::Client`, `pool_max_idle_per_host=32`, `timeout=15s` default, webhook-exec 30s override.
- Also locked: `secrecy 0.8`, `futures` (BoxFuture), `time` (serde-well-known/formatting/parsing), `percent-encoding`, `std::sync::Mutex` for buckets. User-Agent `DiscordBot (https://github.com/<org>/rivulus, <ver>)` (URL required by Discord, not bare name).
- Multipart via `reqwest::multipart`, audit-reason header, `payload_json` part.
- No second HTTP stack. `gateway` MUST NOT depend on `rest`; bootstrap via trait `GetGatewayBot`.

## D3 JSON (locked, not deferred)
- Default `serde_json 1`. All model serde with it. Abstract behind `common::json` shim (`from_slice`, `to_vec`, `RawValue` peek).
- `simd-json` is NOT a default feature; P10 may spike it as opt-in `simd` feature behind same shim. No `simd` in v1 acceptance.

## D4 Cache (locked, not deferred)
- v1: `dashmap 6` + `foldhash` hasher (MSRV-checked in P0). Values `Arc<T>`.
- `papaya` evaluation deferred to P10 spike only, not v1.
- Canonical interning: `users: Map<UserId,Arc<User>>`; `Message.author_id: UserId` + lookup, never duplicate `User` inside `Message/Member/Presence`.
- Caps: per-channel `VecDeque<MessageId>` + `message_limit` (default 0=off), member LRU high-water, `retain` sweeper sharded, no full-scan block.

## D5 MSRV (locked, not deferred)
- `1.75`, edition 2021, `resolver=2`, `tokio` sole runtime. Pinned CI job `1.75 + stable`.
- Allowed: `async fn` in traits + RPITIT (both stable in 1.75), `std::sync::OnceLock`. Forbidden: `async-trait` new use, `once_cell`, `papaya/seize` if it breaks 1.75.
- P0 must verify `dashmap6/foldhash/reqwest/rustls/ed25519/tracing` build on 1.75 or pin/replace.

## D6 Voice (deferred, explicitly)
- Post-1.0. `voice` crate exists as stub + spec only. No voice acceptance in S1-S5 v1 gates.
- When built: WS `?v=8` opcodes 0-9, UDP IP-discovery, RTP seq/ts, `xsalsa20poly1305` + AES-GCM modes, 5x silence spaced 20ms, buffered resume via `seq_ack`, DAVE behind `voice-dave` experimental (default no DAVE flag, never replay ciphertext).
