# CHANGELOG

All notable changes to `rivulus` (facade version = release; inner crates
version independently). Format follows Keep a Changelog; versions follow
SemVer (`0.x` pre-1.0, per `plans/07-testing-ci-docs-release.md`).

## [0.1.0] — Unreleased

### Added (P0 Scaffold & guardrails)

- Workspace `crates/{common,model,gateway,rest,cache,standby,interactions,framework,voice,rivulus}`
  with short lib names, `#![forbid(unsafe_code)]` everywhere, MSRV 1.75.
- Locked decisions (`plans/09-decisions-locked.md`): reqwest+rustls HTTP,
  serde_json behind `common::json`, dashmap6+foldhash cache, tokio-only.

### Added (P1 Model + Core / P1a Contracts freeze)

- `model`: `Id<T>`, `Permissions`/`Intents` bits, all resource types,
  typed `Event` (~90 + `Unknown`), `Partial`, Components V2, builders
  validate shells, `SnowflakeUtil::timestamp`.
- Frozen P1a contracts: `model::Event`, `gateway::EventHandler`
  BoxFuture form, dyn-safe `cache::Cache`, `rest::Route`,
  `common::Error`, `ResourceType` bits, `gateway::ShardMessenger`.

### Added (P2 REST + Ratelimiter)

- `rest::Client` (reqwest), bucket inference + global 50rps +
  429/shared/global handling, typed routes, audit-reason, multipart
  `payload_json`, validation, request-ids, ban-meter.

### Added (P3 Gateway + Sharding)

- Hello/Identify/Ready/Heartbeat+Ack watchdog/Reconnect/InvalidSession/
  Resume, close-code matrix, zlib-stream, presence op3, chunk op8 +
  reassembly, `SendQueue`, `Cluster` max_concurrency ordering + respawn.

### Added (P4 Cache + Standby)

- `cache`: `ResourceType` gating, canonical `users` interning, caps +
  sharded `retain` sweeper + `cache_hit_ratio`, old/new diffs, bounded
  partials; presets `minimal()/balanced()/full()` (builder fns).
- `standby`: `wait_for`/`stream_for` + `Collector`
  (filter/max/time/idle/dispose/end_reason).

### Added (P5 Facade Client + Managers)

- `rivulus::{Client, Context, Dispatcher, EventHandler}` (feature-gated);
  sync `Client::builder/add_handler/build`, async
  `login/shutdown/health/latencies`; `ctx.guilds()/channels()/members()/
  messages()` with `get` + `fetch` + `fetch_page` + `stream_pages`;
  lag-aware broadcast fan-out (`LagPolicy`, never silent drop).

### Added (P6 Interactions + framework)

- `interactions`: bulk-overwrite register + diff log, <=3s respond/defer,
  15min followups, <=25 autocomplete, modal router, V2 components, ed25519
  verify vectors.
- `framework`: prefix parser + guarded registry (no cooldowns/macros v1).

### Added (P7 Voice stub)

- `voice` crate: `JoinConfig` placeholder; docs state deferred post-1.0
  (D6). No UDP/crypto in the default build.

### Added (P8 Hardening)

- `SessionStore` trait, ban-meter wiring, request-ids, validation pass,
  unknown-event counters, `health()` + `shutdown_handle()`
  (CancellationToken + signal), token redaction.

### Added (P9 Examples + Docs + Migration)

- `examples/`: `ping`, `slash`, `buttons-collector`, `sharded`,
  `cache-tuning`, `webhook-verify-axum` (all mock-safe; mirrored under
  `crates/rivulus/examples/` so `cargo run -p rivulus --example <name>`
  works).
- Root `README.md` (parity badge, template RAM table, quickstart, feature
  flags, voice notice), `MIGRATION.md` (discord.js mapping, short
  imports), this `CHANGELOG.md`.
