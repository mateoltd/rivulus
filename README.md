# rivulus

[![parity](https://img.shields.io/badge/parity-discord.js%20S1--S5-blue)](MIGRATION.md)
[![msrv](https://img.shields.io/badge/msrv-1.75-orange)](plans/09-decisions-locked.md)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-green)](Cargo.toml)

A modular, async Rust library for Discord bots with discord.js-level
capabilities — familiar concepts (`Client`, intents, events, managers,
collectors, sharding, REST) with deterministic memory and no GC.

> Status: `0.1.0` (unreleased, P0–P9). Voice is **stub-only** in v1
> (deferred post-1.0, see below).

## 5-minute quickstart

```toml
# Cargo.toml — short import names, no `rivulus-` prefix in Rust code.
[dependencies]
rivulus = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```rust,no_run
use futures::StreamExt;

struct Ping;

impl rivulus::EventHandler for Ping {
    fn on_dispatch(
        &self,
        _ctx: rivulus::Context,
        ev: rivulus::model::Event,
    ) -> futures::future::BoxFuture<'_, ()> {
        Box::pin(async move {
            if matches!(ev, rivulus::model::Event::MessageCreate(_)) {
                println!("pong!");
            }
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), rivulus::common::Error> {
    let client = rivulus::Client::builder(String::from("DISCORD_TOKEN"))
        .intents(rivulus::model::Intents::GUILDS | rivulus::model::Intents::GUILD_MESSAGES)
        .cache(|_| rivulus::cache::CacheConfig::balanced())
        .sharding(rivulus::gateway::ShardStrategy::Auto)
        .add_handler(Ping)
        .build()?;
    // Live: needs DISCORD_TOKEN.
    // client.login().await?;
    // client.shutdown().await;
    Ok(())
}
```

More in [`crates/rivulus/examples/`](crates/rivulus/examples/): `ping`, `slash`, `buttons-collector`,
`sharded`, `cache-tuning`, `webhook-verify-axum` — all runnable offline
(mock-safe, no `DISCORD_TOKEN` needed).

## discord.js → rivulus (short version)

| discord.js | rivulus (facade + short libs) |
|---|---|
| `new Client({ intents })` / `client.login()` | `rivulus::Client::builder(tok).intents(..).build()?` / `client.login().await` |
| `client.on('messageCreate', f)` | `.add_handler(closure)` / `impl rivulus::EventHandler` |
| `guild.members.fetch(id)` | `ctx.members().fetch(guild, user)` (+ sync `get(guild, user)`) |
| `channel.messages.fetch({ limit })` | `ctx.messages().fetch_page(ch, n)` AND `stream_pages(ch, n)` — one-shot vs single-page stream (no cursor), `0→1` clamp, live-only (mock returns `{}` offline); full semantics in MIGRATION + `ping.rs` |
| `awaitMessages({ filter, max, time })` | `standby::Standby::wait_for(..)` / `standby::Collector` |
| `new SlashCommandBuilder()` + REST put | `interactions::CommandDef::slash(..)` + `bulk_overwrite_payload(..)` |
| `client.shard` / `ShardingManager` | `gateway::ShardStrategy::Auto` + `gateway::ClusterConfig` |
| `Options.makeCache` / sweepers | `cache::CacheConfig::minimal()/balanced()/full()` + `sweeper::Sweeper::spawn(..)` |

Full mapping: [`MIGRATION.md`](MIGRATION.md).

## RAM (measured vs template)

> TEMPLATE — pending P10 measurement. The shape below is what P10 will
> fill with measured RSS (`quick` CI: 100 guilds; nightly: 1k/10k/50k).
> Do not quote these numbers as results.

| Workload | discord.js RSS (template) | rivulus RSS (template) | Notes |
|---|---|---|---|
| 100 guilds, msg-only | — | — | P10 `quick` |
| 1k guilds, full cache | — | — | P10 nightly |
| bytes/guild | — | — | measured, not asserted |

Target ( Crane 01 §2): ~80% lower RAM under equivalent workloads;
CI gate (S2): `<= 20%` of equivalent discord.js RSS with tolerance bands.

Measured (P10, local-only): [`bench/results/2026-09-27.md`](bench/results/2026-09-27.md)
(replay worst p99 0.001ms, REST available p99 0.37ms; djs side pending nightly).
Run: `cargo bench -p rivulus`; compare: `python3 bench/compare.py --baseline
bench/results/replay-2026-09-27.json --current <new>.json` (warn-only).

## Feature flags

| Flag | Default? | Contents |
|---|---|---|
| `gateway` | yes (`default`) | WS shards, `Cluster`, identify buckets |
| `rest` | yes (`default`) | HTTP client, ratelimiter, builders |
| `cache-inmemory` | yes (`default`) | `InMemoryCache`, presets, sweeper |
| `standby` | via `full` | waiters, collectors |
| `framework` | via `full` | prefix parser, guarded registry |
| `interactions` | via `full` | commands, ed25519 verify, responses |
| `voice` | no (stub) | `JoinConfig` placeholder only |

`default = ["gateway", "rest", "cache-inmemory"]`;
`full = ["default", "standby", "framework", "interactions"]`.
There are intentionally NO `simd` / `zstd-gateway` / `voice-dave` /
`interactions-axum` features in v1 (locked, see
[`plans/09-decisions-locked.md`](plans/09-decisions-locked.md)).
The axum webhook sketch is `examples/webhook-verify-axum.rs` and needs
YOUR OWN `axum` dependency — it is example-only, never a crate feature.

Downstream crates (external bots) — copy-paste:

```toml
# Minimal (gateway + rest + in-memory cache):
rivulus = "0.1"
# Full (adds standby collectors, framework guards, interactions):
rivulus = { version = "0.1", features = ["full"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
```

```rust,no_run
// Cache trait import (required for stats()/hit_ratio()):
use rivulus::cache::{Cache, CacheConfig, InMemoryCache};
```

## Voice (deferred)

Voice send/receive is **deferred post-1.0** (decision D6). The `voice`
crate exists as a stub (`JoinConfig` placeholder) so the workspace shape
is stable; it performs no UDP, RTP, or crypto. There is no
`examples/voice-send.rs` in v1.

## Errors

All errors flow through `rivulus::common::Error` (no `unwrap` in library
code, `#![forbid(unsafe_code)]` workspace-wide). See
[`CHANGELOG.md`](CHANGELOG.md) for the P0–P9 history.
