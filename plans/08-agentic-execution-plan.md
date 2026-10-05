# 08 — Agentic Execution Plan (P0–P12, short namespaces, locked deps)

> Order is strict: P0 -> P1a -> (P2,P3,P4 read-only parallel) -> P5 -> P6 -> P8 -> P9 -> P10 -> P11 -> P12. P7 voice = stub only.

## P0 Scaffold & guardrails (locks land here)
Paths: `Cargo.toml (rust-version = "1.75", NO rust-toolchain.toml pin — CI matrix 1.75+stable via dtolnay/rust-toolchain), .github/workflows/ci.yml, deny.toml, plans/09*, plans/DECISIONS.md, xtask/`, `crates/{common,model,gateway,rest,cache,standby,interactions,framework,voice,rivulus}` stubs with `[package] rivulus-<x> + [lib] <x>`, `#![forbid(unsafe_code)]` all.
Tasks: features per 02§3 (no simd/zstd/voice-dave/axum features); `common::{Error,json,oauth,utils}` skeleton; MSRV verify (dashmap6/foldhash/reqwest/rustls/ed25519/tracing on 1.75) or pin/replace + log; `tracing` setup; `SecretString` token type; `grep unwrap` lint; fmt/clippy/deny green + `cargo geiger --forbid-only --package rivulus-*` (OUR packages only, deps excluded) + `grep -rn unwrap|expect src/` fail (allow `#[cfg(test)]` only) + `grep -rn "mod gateway" crates/rivulus/src` fail (facade uses re-export, never shadow module) + P0 TLS smoke (webpki-roots) + MSRV 1.75 table check (dashmap6/foldhash/reqwest/rustls/secrecy/ed25519/tracing); `cargo doc` skeleton.
Acceptance: `cargo check --workspace --all-features` + `cargo test --no-run` + `cargo doc -D warnings` pass on stubs.

## P1 Model + Core
Paths: `crates/model/src/{id.rs,permissions.rs,intents.rs,gateway_bot.rs (GetGatewayBotResponse/SessionStartLimit — pure types so gateway needs no rest dep),guild.rs,channel.rs,message.rs,user.rs,member.rs,role.rs,emoji.rs,sticker.rs,invite.rs,webhook.rs,thread.rs,poll.rs,scheduled.rs,stage.rs,soundboard.rs,entitlement.rs,automod.rs,audit.rs,onboarding.rs,oauth.rs,interactions.rs,components_v2.rs,events.rs,partials.rs}`, `crates/common/src/{error.rs,json.rs,validate.rs (shared limits — mirrors twilight-validate),oauth.rs,utils.rs,format.rs,prelude.rs}`.
Tasks: `Id<T>`, `Permissions`+`Intents` bits (exact values per 04A), all resources serde + fixtures, `Event` ~90 + `Unknown`, `Partial`, Components V2 types, builders validate shells, `SnowflakeUtil::timestamp`.
Acceptance: `cargo test -p rivulus-model` >=50 fixtures round-trip, no unknown-field fail on docs samples, `Unknown(u8)` round-trips.

## P1a Contracts freeze (gate before parallel)
Paths: `crates/model/src/events.rs`, `crates/cache/src/trait.rs`, `crates/rest/src/routes.rs`, `crates/common/src/error.rs`, `tests/fixtures/*`, `tests/support/*`.
Tasks: freeze VERBATIM signatures (`model::Event`, `gateway::EventHandler` BoxFuture form, dyn-safe `cache::Cache`, `rest::Route {method/path_template/major_id}`, `common::Error`, `ResourceType` bits, `Partial`, `gateway::ShardMessenger`, `health()` struct) + fixture list (`components_v2.json` included) + `tests/support/mock_gateway.rs + mock_rest.rs` exposing `async fn spawn() -> SocketAddr` (ephemeral port via oneshot, deterministic script). Gate: NO P2/P3/P4 start without this; `cargo doc -p rivulus-model --no-deps` renders; no signature change without RFC after.
Acceptance: contracts render + mocks spawn + `Arc<dyn Cache>` compiles.

## P2 REST + Ratelimiter
Paths: `crates/rest/src/{client.rs,ratelimiter.rs,routes.rs,builders/*.rs,multipart.rs,audit.rs}` (lib `rest`).
Tasks: `rest::Client` (reqwest locked), bucket inference + global 50rps + 429/shared/global handling, typed routes per 04B, audit-reason, multipart `payload_json`, validation, request-ids, ban-meter. `gateway` does NOT depend on this crate.
Acceptance: `tests/rest_ratelimit.rs` on `mock_rest` shows pre-emptive wait + `retry_after` honored + 401/403 never retried + shared excluded; 0-placebo.

## P3 Gateway + Sharding (after P1a; parallel read-only with P2)
Paths: `crates/gateway/src/{shard.rs,cluster.rs,session.rs,compression.rs,intents.rs,opcodes.rs,queue.rs}` (lib `gateway`).
Tasks: Hello/Identify/Ready/Heartbeat+Ack watchdog/Reconnect/InvalidSession/Resume, close-code matrix + help, zlib-stream (resync buffer, 256K cap, reset), presence op3, chunk op8 + reassembly, `SendQueue` (120/min + 5/20s presence), `Cluster` max_concurrency ordering + respawn + `Bootstrap` trait (no rest dep), P3 spike: `tokio-tungstenite` vs `tokio-websockets` -> DECISIONS.md (one ships). Owned `Event` only; header-peek via `common::json`.
Acceptance: `tests/gateway_resume.rs` on `mock_gateway` (drop->resume assert) + close-code matrix + latency tracked.

## P4 Cache + Standby (after P1a; parallel read-only)
Paths: `crates/cache/src/{trait.rs,inmemory.rs,sweeper.rs}` (lib `cache`, dashmap6 locked), `crates/standby/src/{waiter.rs,stream.rs,collector.rs}` (lib `standby`).
Tasks: `ResourceType` gating, canonical `users` interning, caps + sharded `retain` sweeper + metrics, old/new diffs (Arc clone, no full-guild clone), partials bounded; `Standby::wait_for/stream_for` + `Collector {filter,max,time,idle,dispose,end_reason}`.
Acceptance: `tests/cache_diff.rs` + `tests/collectors.rs` (filter+max+idle+timeout+end-reason) + RAM-table doc draft.

## P5 Facade Client + Managers
Paths: `crates/rivulus/src/{client.rs,context.rs,dispatcher.rs,managers/*.rs,formatters.rs,prelude.rs}`.
Tasks: sync `Client::builder/add_handler/build`, async `login/shutdown/health/latencies`, `Context`, manager fns (CRUD + dual pagination), fan-out `gateway -> cache -> standby -> handlers` with lag-aware broadcast (explicit `Lagged` resync, never silent drop), backpressure errors.
Acceptance: `examples/ping.rs` + `examples/sharded.rs` run vs mocks in CI + `tests/dispatcher_lag.rs` (Lagged policy per kind) green.

## P6 Interactions (+ framework)
Paths: `crates/interactions/src/{commands.rs,verify.rs,respond.rs}` (lib `interactions`), `crates/framework/src/{parser.rs,registry.rs}` (lib `framework`), `examples/webhook-verify-axum.rs` (dev-dep only).
Tasks: bulk-overwrite register + diff log, <=3s respond/defer, 15min followups, <=25 autocomplete, modal router, V2 components, ed25519 verify vectors, framework args + permission guards.
Acceptance: `tests/interactions_verify.rs` + `examples/slash.rs` + `examples/buttons-collector.rs` vs mocks (live `#[ignore]`).

## P7 Voice — STUB ONLY (deferred D6)
Paths: `crates/voice/src/lib.rs` (lib `voice`): empty `JoinConfig`, `Error::Deferred`, doc pointer to 04E. No deps beyond model+core.
Acceptance: compiles, docs state deferred, no UDP/crypto in lockfile for default build.

## P8 Hardening
Tasks: `SessionStore` trait, ban-meter wiring, request-ids, validation pass, unknown-event counters, `health()` + `ShutdownHandle` (CancellationToken+signal), token redaction tests.
Acceptance: `tests/soak.rs --short` (10min seeded chaos) resume-rate assert + fail-fast message checks + 0 panics.

## P9 Examples + Docs + Migration
Tasks: `examples/{ping,slash,buttons-collector,sharded,cache-tuning,webhook-verify-axum}.rs` + root README (parity/RAM graph/quickstart/migration/features/voice notice) + `MIGRATION.md` (short imports) + doctests.
Acceptance: dogfood (<30min, multi-handler + closure + dual pagination) + `cargo test --doc` green.

## P10 Perf + Bench vs discord.js
Tasks: interning audit, `benches/replay.rs` + `benches/rest_ratelimit.rs`, `bench/djs-equiv/` quick/nightly, tolerance-band perf CI, README graph.
Acceptance: <=20% RSS vs djs (tolerance bands) + p99 <5ms replay; `bench/results/<date>.md`.

## P11 Testing/CI/Release
Tasks: full pyramid + CI matrix (1.75+stable x linux/win/mac) + deny/geiger/audit + publish dry-run + CHANGELOG.
Acceptance: green CI + tarpaulin >=80% (rest,gateway,cache).

## P12 1.0 + Soak
Tasks: API freeze, SemVer tags `rivulus-vX.Y.Z`, crates.io publish (voice stub versioned too), manual `soak-72h`.
Acceptance: S1-S5 (voice-excluded) signed in `plans/SIGNOFF.md`.

## Agent playbook
- `cargo fmt && cargo clippy -- -D warnings && cargo test -p <pkg>` before handoff. No `unwrap/expect` in `src/`.
- Never add deps without updating `02` + `09` + `deny.toml` + `DECISIONS.md`.
- Short imports only in code/docs (`gateway::`, `rest::`, ...). Fail review on `rivulus-` prefix in Rust code.
- Log decisions in `plans/DECISIONS.md`; blockers via team tasks.

