# 01 — Vision, Scope, Success Criteria

## 1. Vision
Build a modular, async Rust library that lets a discord.js developer build the *same bot* with familiar concepts (Client, Intents, Events, Managers, Collectors, Sharding, REST) but with deterministic memory, no GC, predictable scaling to 100k+ guilds.

Naming: project `rivulus`; imports are short words (`gateway`, `rest`, `model`, `core`, `cache`). See `09-decisions-locked.md` D1. Never prefix namespaces with `rivulus-` in code/docs.

## 2. Main objective (verbatim)
> Discord.js-level capabilities, implemented efficiently in Rust, without GC, targeting ~80% lower RAM under equivalent workloads.

Non-goals: new Discord features beyond discord.js parity; JS API-compat; multi-runtime support; built-in hosting; Opus transcode in core; voice past stub (deferred post-1.0, D6); ETF encoding (JSON only v1).

## 3. Scope — MUST cover (discord.js surface, v1)
Client builder/config (`framework` = parser + registry + permission guards ONLY — no cooldowns/help-pagination/macros v1), Gateway intents + partials, ~90 dispatch events, guild/channel/message/member/role/permission/reaction/webhook/attachment/thread/poll/scheduled-event/sticker/emoji/invite/stage/soundboard/entitlement/subscription/audit-log/auto-mod/onboarding management, slash/user/message commands, buttons/selects/text-inputs/modals/autocomplete, Components V2 (ActionRow + TextDisplay/Section/Thumbnail/MediaGallery/File/Separator/Container), REST with ratelimits, configurable cache + sweepers, sharding + clustering, collectors/standby with full djs semantics, builders + formatters, OAuth2 invite/scopes helpers.

Explicit v1 exclusions (documented in README + `MIGRATION.md`): Activities/RPC, Social SDK, Lobby (track only), voice send/recv (stub + spec only, `voice-dave` experimental later), ETF, `interactions-axum` as example-only (not a crate feature).

## 4. Success gates (measurable, CI-checkable)
- **S1 Parity:** >=95% of discord.js stable managers/structures have typed equivalent; `examples/ping`, `examples/slash`, `examples/buttons-collector`, `examples/sharded` run against mock gateway+rest in CI and against live Discord with `#[ignore] live` (needs `DISCORD_TOKEN+GUILD_ID`).
- **S2 Memory:** <=20% of equivalent discord.js RSS on synthetic matrix (quick CI: 100 guilds; nightly full: 1k/10k/50k x msg-only/full-cache). Predictable growth: documented bytes/guild table measured, not asserted. Perf CI uses tolerance bands vs baseline artifact, never absolute RSS.
- **S3 Correctness:** Mock-WS resume test + close-code matrix + 429-storm mock pass; short chaos `tests/soak.rs --short` (10min seeded, shard kills) resume rate asserted; 72h soak is `#[ignore] soak-72h` manual. REST conformance: shared-scope 429s excluded, pre-emptive waits asserted via mock headers (no `0 pre-emptive 429s` absolutism against live).
- **S4 Ergonomics:** Fresh-agent dogfood: ping+slash+button bot from `examples/` without reading internals; requires multi-handler + closure handler + dual pagination API (see 02 section 4).
- **S5 Reliability:** 0 panics, clean shutdown via `ShutdownHandle` (`tokio-util CancellationToken` + signal), `cargo fmt/clippy -D warnings/doc -D warnings/deny/geiger/tarpaulin >=80% (rest,gateway,cache)` green.

## 5. Constraints
Rust ownership, `Send+Sync + 'static` events everywhere, `async/await` only on tokio, backpressure everywhere (no unbounded queues), no global mutable state except explicit `Client`/`ShardManager`, SemVer, MIT/Apache-2.0 dual, `#![forbid(unsafe_code)]` workspace-wide with no exceptions (audit via deny/vet/geiger), all errors via `common::Error`.
