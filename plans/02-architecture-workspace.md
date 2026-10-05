# 02 — Architecture & Workspace (locked deps/namespaces: see 09)

## 1. Workspace layout (`/home/zero/lab/rivulus.rs/`)
```
Cargo.toml (workspace, resolver=2, edition 2021, rust-version 1.75)
crates/
  core/          # package rivulus-core, lib core: Error/Result, json shim, consts, utils, prelude
  model/         # package rivulus-model, lib model: Id<T>, all Discord resources (serde only, no IO)
  rest/          # package rivulus-rest, lib rest: HTTP client, ratelimiter, routes, builders
  gateway/       # package rivulus-gateway, lib gateway: Shard, Cluster, intents, opcodes, compression, session
  cache/         # package rivulus-cache, lib cache: Cache trait + InMemory impl, ResourceType, sweepers
  standby/       # package rivulus-standby, lib standby: event waiters / collectors
  interactions/  # package rivulus-interactions, lib interactions: cmd register, ed25519 verify, responses
  framework/     # package rivulus-framework, lib framework: optional prefix+slash framework
  voice/         # package rivulus-voice, lib voice: STUB + spec only v1 (deferred, D6)
  rivulus/       # facade: Client, managers, Context, formatters; re-exports core/model/gateway/rest/...
xtask/ benches/ bench/djs-equiv/ examples/ tests/ tests/fixtures/ tests/support/ plans/
```
Package/lib mapping (mandatory): `[package] name="rivulus-gateway"` + `[lib] name="gateway"`. Same pattern for all. Crate dir = lib name.

## 2. Dependency policy (LOCKED — P0 verifies on 1.75, replaces only if MSRV breaks)
- `tokio { rt-multi-thread, net, time, sync, macros, signal }`, `tokio-util` (CancellationToken), `futures` (Stream), `tracing`, `tracing-subscriber` (examples only).
- WS: `tokio-tungstenite 0.24+` (P3 spike must evaluate `tokio-websockets` Bytes support; decision logged in DECISIONS.md; only one ships). `flate2` (zlib-stream, miniz_oxide backend — no zlib-ng/unsafe). `zstd` NOT in v1 (spec mentions only as future).
- HTTP: `reqwest 0.12` + `rustls-tls-manual-roots` + `multipart`. `pool_max_idle_per_host=32`. No second HTTP stack.
- Serde: `serde/derive`, `serde_json 1`, `serde_repr`, `bitflags 2`, `smallvec`, `bytes`, `itoa`, `ryu` (via serde_json). String policy: `Box<str>` cold fields, `Arc<str>` only where measured hot; NO `compact-str` v1 (MSRV/unsafe audit cost).
- Concurrency: `dashmap 6` + `foldhash` (locked D4). `arc-swap` for config hot-swap. `tokio::sync::{mpsc,broadcast,RwLock,Semaphore}`. No `papaya` v1 (P10 spike only). No `parking_lot` (std only v1). No `once_cell` (use `std::sync::OnceLock`).
- Time/ids: own `model::Id<T>` (`#[repr(transparent)] NonZeroU64`), own `SnowflakeUtil::timestamp()`. No `snowflake` crate. `time` (not chrono) only in model behind `time` feature for timestamps.
- Crypto: `ed25519-dalek 2` (interactions verify). `secrecy 0.8` (token storage, MSRV-safe, no unsafe). Voice crypto NOT in v1 lockfile (deferred).
- TLS roots: `webpki-roots 0.26` wired explicitly in `rest::Client::builder` (`rustls-tls-manual-roots` ships NO roots — without this TLS fails on first call). P0 TLS smoke test required.
- IDs: request-id = process-local `AtomicU64` counter (never `uuid v4` per-request — alloc/latency). `futures` (BoxFuture for handler blanket). `time` with `serde-well-known/formatting/parsing` for timestamps. `percent-encoding` for audit-reason/URL encoding. Buckets use `std::sync::Mutex` (short critical sections), never `tokio::Mutex` for bucket state.
- Observability: `tracing`; `metrics` optional feature only. Lint: `cargo-deny`, `cargo-geiger` (must report 0 unsafe in our code).
- Reject v1: second JSON lib, second WS lib, `async-trait` (use RPITIT on 1.75), `mini-moka/moka`, `simd-json`, `zstd`, `audiopus`, `xsalsa20poly1305`.

## 3. Feature flags (additive, docs-gated; facade re-exposes)
- Per crate minimal defaults. Facade `rivulus`: `default=["gateway","rest","cache-inmemory"]`; `full=["default","standby","framework","interactions"]`; `voice` exists but documented STUB (compiles empty types + spec link, no UDP/crypto deps).
- `simd`, `zstd-gateway`, `voice-dave`, `interactions-axum` DO NOT EXIST v1 (no placeholder features). Axum webhook example lives in `examples/` with dev-dependency only.
- CI builds: `--no-default-features`, `--all-features`, per-crate minimal. `cargo doc` must show feature table per item.

## 4. Public API shape (discord.js-mapped, Rust-idiomatic; ergonomics fixes applied)
```rust
use rivulus::{Client, Context, Event};
use model::{GuildId, MessageId}; // or rivulus::model::GuildId
use gateway::{Intents, ShardStrategy};
use cache::ResourceType;

let client = Client::builder("Bot TOKEN") // builder is SYNC; no I/O in build()
  .intents(Intents::GUILDS | Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT)
  .cache(|c| c.resource_types(ResourceType::GUILD | ResourceType::CHANNEL).message_limit(0))
  .sharding(ShardStrategy::Auto)
  .add_handler(MyHandler)          // multi-listener; repeatable
  .add_handler(|ctx: Context, ev: Event| async move { /* closure handler */ })
  .build()?;                        // Result<Client, common::Error>
client.login().await?; // GET /gateway/bot + spawn Cluster; returns after READY or fails fast
let h = client.health(); let lat = client.latencies();
client.shutdown().await; // CancellationToken + graceful shard close
```
- `EventHandler` (object-safe — RPITIT `impl Future` in traits is NOT object-safe, breaks `Box<dyn EventHandler>`):
```rust
// crates/rivulus/src/dispatcher.rs (frozen P1a)
pub trait EventHandler: Send + Sync + 'static {
  fn on_dispatch(&self, ctx: Context, ev: Event) -> futures::future::BoxFuture<'_, ()>;
}
impl<F, Fut> EventHandler for F where F: Fn(Context, Event) -> Fut + Send + Sync + 'static, Fut: Future<Output=()> + Send + 'static {
  fn on_dispatch(&self, ctx: Context, ev: Event) -> futures::future::BoxFuture<'_, ()> { Box::pin((self)(ctx, ev)) }
}
```
`Dispatcher::add_handler(h: impl EventHandler)` (multi-listener, repeatable). P5 compile check: two struct handlers + one closure handler.
- `Context { http: Arc<rest::Client>, cache: Arc<dyn cache::Cache>, shard: gateway::ShardMessenger }` — generic `Arc<C: Cache>` rejected v1 (dyn keeps facade simple; note vtable cost in docs).
- `Event: Send + Sync + 'static`, fully owned (`String`/`Arc<str>`/`Arc<T>`), never borrowed from WS buffer. Fast-path header peek (`op/t/s`) via `common::json::RawValue` only, then full owned parse.
- Managers are thin fns on `Context`: `ctx.guilds().fetch(id)`, `ctx.messages().send(ch, b)`, `ctx.members().timeout(...)`. Pagination DUAL API: `fetch_page(limit) -> Vec` + `stream_pages() -> impl Stream` (never Stream-only).
- Builders in `rivulus::builders` (mirrors djs Builders): `CreateMessage`, `CreateEmbed`, `SlashCommandBuilder`, `ActionRow`, `Button`, `Modal` — all owned `#[must_use]`, `validate() -> Result<_, Error::Validation>` client-side.
- IDs: `model::{GuildId, ChannelId, UserId, MessageId, ...} = Id<Marker>`; `Permissions` bitflags u64 with string-serde; `common::Result<T>`.
- Token hygiene: `secrecy::SecretString` for token storage; `Debug` redacts; `Display` never prints token. (SpaceBunny fix: replaces raw String.)

## 5. Layering rules (no cycles)
`model` pure (no IO, no thiserror). `core` holds `Error` + json shim + utils. `rest/gateway/cache/standby/interactions/framework` depend only on `model+core` (+`rest` types for builders, never client). `gateway` MUST NOT depend on `rest` (bootstrap via `gateway::Bootstrap { get_gateway_bot(): GetGatewayBotResponse }` trait caller injects). `rivulus` facade wires all. `#![forbid(unsafe_code)]` every crate, zero exceptions (voice stub included).
