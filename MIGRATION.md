# MIGRATION — discord.js → rivulus

Every row shows the discord.js concept and the rivulus equivalent with
SHORT imports (never a `rivulus-` prefix in Rust code). See also
`plans/03-discordjs-parity-matrix.md` for the full matrix.

## Client & login

```js
// discord.js
const client = new Client({ intents: [GatewayIntentBits.Guilds] });
await client.login(process.env.DISCORD_TOKEN);
```

```rust,no_run
// rivulus
let client = rivulus::Client::builder(String::from("DISCORD_TOKEN"))
    .intents(rivulus::model::Intents::GUILDS)
    .build()?;
client.login().await?; // + client.shutdown().await on exit
```

| discord.js | rivulus |
|---|---|
| `new Client({ intents, partials, shards, sweepers, makeCache })` | `rivulus::Client::builder(tok).intents().cache().sharding()` |
| `client.login(token)` / `destroy()` | `client.login().await` / `client.shutdown().await` + `shutdown_handle()` |
| `client.ws` / `client.rest` | `ctx.shard: gateway::ShardMessenger` / `ctx.http: Arc<rest::Client>` |
| `client.uptime` / `client.ws.ping` | `client.uptime()` / `client.latencies()` per shard — pre-spawn placeholder is single `(gateway::ShardId { id: 0, total: 1 }, 0)` (trap: `ShardId` lives in `gateway`, NOT `model` — E0422); post-login mirrors messenger latencies |
| `ShardingManager` / `broadcastEval` | `gateway::Cluster` (`from_config`, `broadcast`) — `login()` spawns it; supervisor restarts are internal (no restart API in docs scope) |
| handlers + health | `client.context(gateway::ShardMessenger { shard: gateway::ShardId { id: 0, total: 1 }, latency: Arc::new(AtomicU64::new(0)) })` → `ctx`; `client.health()` → `{ shards: [{ shard, status: Disconnected (pre-spawn placeholder), latency_ms, seq: None (no tracking yet), guilds_cached }], ready: bool }`; `client.ready_at()` / `shutdown_handle()` — `shutdown()` takes no args and returns `()` (use `let _ =` to compile against either shape); tokens are `SecretString` at the boundary (`String::from(..)` in snippets converts implicitly) |

## Intents & events

```js
// discord.js
client.on('messageCreate', (m) => { /* ... */ });
```

```rust,no_run
// rivulus — struct handler or plain closure, both are `EventHandler`.
let client = rivulus::Client::builder(String::from("t"))
    .add_handler(|_ctx: rivulus::Context, ev: rivulus::model::Event| async move {
        if matches!(ev, rivulus::model::Event::MessageCreate(_)) { /* ... */ }
    })
    .build()?;
```

| discord.js | rivulus |
|---|---|
| `client.on('messageCreate' / 'guildMemberAdd' / ...)` (~90 events) | `rivulus::EventHandler::on_dispatch(ctx, model::Event)` (typed `model::Event`) |
| `client.on('shardReady' / 'shardDisconnect')` | synthetic `gateway::ShardEvent` (NOT Discord dispatch, NOT in `model::Event`) |
| partials (`Partials.Message`) | `model::Partial` + bounded `lazy_fetch_partials` |

## Managers (on `Context`)

Every manager exposes sync cache lookup + async `fetch(..)` (REST),
plus — for list routes only — the dual pagination API `fetch_page()` AND
`stream_pages()`. Cache lookups are `get_guild` / `get_channel` /
`get_message` / `get(guild, user)` (not a single `get(id)`); the `Cache`
trait must be in scope for `stats()` / `hit_ratio()`.

```rust,no_run
// rivulus — cache first, REST fallback, no fetch-only calls.
let msg = ctx.messages().fetch(channel_id, message_id).await?;
// One-shot page (see pagination notes below):
let page = ctx.messages().fetch_page(channel_id, 50).await?;
```

Pagination semantics (applies to `messages` / `channels` / `guilds` /
`members` list routes only):
- One-shot vs stream: `fetch_page` returns one `Vec` now;
  `stream_pages` yields the same one page then ends (single-page unfold —
  Discord list routes here expose no cursor, so there is no
  before/after/around progression; `ping.rs` shows first-item-only
  `tokio::pin!` + `next().await` vs full `while next` loops).
- Limits: `0` maps to `1` (client-side clamp); Discord caps history at
  100 — larger values are truncated server-side, never an error here.
- `Id` params are by-value (`ChannelId`, not `&ChannelId` — passing `&`
  fails E0308; `Id::new(n)` returns `Option`, zero → `None`).
- Live-only: the mock REST server returns `{}` (not an array), so
  `fetch_page` / `stream_pages` fail offline with a REST/auth error and
  succeed live (`ListMessages` history). Handle `Err` gracefully offline
  (`match`, not `?`); `ping.rs:63-76` shows the full `while next` loop —
  for first-item-only use `tokio::pin!` + a single `next().await` (as in
  validators), not the full loop.
- `/users/@me/guilds` shape note: Discord returns partial guilds (no
  `owner_id`) there, so `Guilds::fetch_page` surfaces a `Deserialize`
  error live — use raw JSON IDs for discovery, then typed
  `channels().fetch_page` / `messages().fetch_page` (see live soak).

| discord.js | rivulus |
|---|---|
| `guild.members.fetch(id)` / `client.users.fetch(id)` | `ctx.members().fetch(guild, user)` / sync `get(guild, user)` |
| `guild.channels.fetch(id)` | `ctx.channels().fetch(id)` / sync `get(id)` |
| `guilds.fetch(id)` | `ctx.guilds().fetch(id)` / sync `get(id)` |
| `channel.messages.fetch({ limit })` | `ctx.messages().fetch_page(ch, n)` AND `stream_pages(ch, n)` |
| `channel.send({ content, embeds })` | `ctx.messages().send(channel_id: ChannelId, content: &str)` — trap: takes `&str`, NOT a `CreateMessage` builder (E0308). For embeds build `rest::CreateMessage::new().content(..).embed(CreateEmbed)` (takes the builder, not built `Embed`); `build()` returns `Result<Vec<u8>>` (needs `?`, do NOT `json::to_vec` again) |
| single vs bulk delete | No single-message `delete(channel, id)` in v1 — only `ctx.messages().bulk_delete(channel, &[MessageId])` (slice, even for one id; Discord needs 2+ server-side). Raw single: `Route::DeleteMessage { channel_id: u64, message_id: u64 }` (both `u64` — call `.get()` on `Id`) via `ctx.http.execute(&route, None, None) -> Result<Vec<u8>>` (3 args) |
| `webhook.send({ content })` | `rest::ExecuteWebhook::new().content(..)` — trap: `.query()` is a 0-arg getter (returns `String`, `''` when unset); there is no `.query(wait, thread)` setter in v1; `build()` returns `Vec<u8>` |
| attachments (multipart) | manual `multipart/form-data` (`payload_json` + `files[N]`, boundary header) — no attachment builder in v1; minimal shape: `--BOUNDARY Content-Disposition: form-data; name="payload_json" (application/json) + Content-Disposition: form-data; name="files[0]"; filename="hello.txt" (text/plain)` with `Content-Type: multipart/form-data; boundary=BOUNDARY`; there is no multipart POST Route helper (JSON `execute` only); `ping.rs`/`slash.rs` cover text-only sends |
| embeds: footer absent | `CreateEmbed::new().title(..).description(..).color(u32)` builds; there is NO `.footer(..)` / `footer_text` / `author` / `field` / `image` / `thumbnail` in v1 (E0599 on all probes) — title/desc/color only. `CreateEmbed::build()` returns `Result<Embed>` (not bytes); `CreateMessage::build()` returns `Result<Vec<u8>>` (needs `?`). |

## Slash commands & interactions

```js
// discord.js
await rest.put(Routes.applicationCommands(appId), { body: [new SlashCommandBuilder()...] });
interaction.reply('Pong!');
```

```rust,no_run
// rivulus
let ping = rivulus::interactions::CommandDef::slash("ping", "Reply with pong");
ping.validate()?;
// Trap: takes a slice by value-move of the vec — borrow for reuse:
// `bulk_overwrite_payload(&[ping])` moves `ping`; use
// `std::slice::from_ref(&ping)` + clone, or keep `desired` vec alive.
let body = rivulus::interactions::bulk_overwrite_payload(&[ping]);
// `body` is the PUT array bytes for `/applications/{app_id}/commands`
// (global) or `/applications/{app_id}/guilds/{guild_id}/commands`
// (guild-scoped, preferred for smoke tests) — there is no bulk-PUT Route
// helper in v1; single-command POST routes are `CreateApplicationCommand` /
// `CreateGuildCommand`. Then:
let ack = rivulus::interactions::respond("Pong!", false)?;
```

| discord.js | rivulus |
|---|---|
| `SlashCommandBuilder` / `REST.put(applicationCommands)` | free fn `interactions::bulk_overwrite_payload(&[CommandDef]) -> Vec<u8>` (not a method — E0599 on `cmd.bulk_overwrite_payload()`) + `diff_log` |
| `interaction.reply()` / `deferReply()` (<=3s) | `interactions::respond(..)` / `ResponseKind::DeferredChannel` (`ack_deadline_ok`) — trap: `respond(content, ephemeral: bool)` 2nd arg is the ephemeral flag; there is no `answer(id, token)` helper in v1 (guild PUT route likewise manual — see below) |
| autocomplete (<=25) / modals | `ResponseKind::AutocompleteResult` (cap enforced) / `ResponseKind::Modal` — owner-decision gaps (no constructors/accessors; see DOGFOOD.md, do not implement here) |
| webhook signature check (express/fastify) | `interactions::verify(pubkey_hex, timestamp, body, sig_hex)` — types: `pubkey_hex: &str` (32B/64 lowercase hex, no `0x`), `timestamp: &str` (opaque, e.g. `"1699000000"`), `body: &[u8]`, `sig_hex: &str` (64B/128 hex); concat is `timestamp_bytes || body_bytes`; `MAX_BODY_BYTES` over-limit → `Validation`, bad sig → `SignatureInvalid` (fail-closed); needs `full`/`interactions` feature; axum sketch is example-only, not a feature |
| prefix commands (`!ping`) | `framework::parse_args("!ping", "!")` + `framework::Registry` guards — `Registry::new()` takes 0 args (not `new("!")` — E0061); `register(name, handler)` then `register_guarded(name, handler, required: Permissions)` (3 args in that order — E0061 otherwise); handler is `Arc<dyn Fn(HandlerCtx) + Send + Sync>` — copy-paste: `Arc::new(\|_ctx: rivulus::framework::HandlerCtx\| {})` (bare `\|\|"pong"` fails E0308); `dispatch(name, caller: Permissions)` returns `Ok` or `Err(Forbidden)` / `Err(NotFound(name))` (missing command stays `NotFound` even for ADMIN). `ADMINISTRATOR` bypass is dispatch logic, NOT `contains()`: `ADMIN.contains(SEND)` is `false`. There are no `and/or/not` guard combinators in v1 (single `Option<Permissions>` per command; composition probes fail E0433/E0425 — owner-decision, do not implement). |

## Collectors → standby

```js
// discord.js
const c = channel.createMessageCollector({ filter, max: 25, time: 60_000, idle: 30_000 });
```

```rust,no_run
// rivulus
let mut c = rivulus::standby::Collector::with_filter(
    rivulus::standby::CollectorConfig {
        max: Some(25),
        time: std::time::Duration::from_secs(60),
        idle: Some(std::time::Duration::from_secs(30)),
    },
    |_| true,
);
```

| discord.js | rivulus |
|---|---|
| `awaitMessages` / `awaitReactions` / `awaitModalSubmit` | `standby::Standby::wait_for(kind, timeout, pred)` — `kind` is `WaiterKind::Any` unless filtering; `pred` is `\|_\| true` for any (typed closure, no annotation needed) |
| `MessageCollector` / `ReactionCollector` / `InteractionCollector` | `standby::Collector { filter, max, time, idle, dispose }` + `end_reason()` — `push(ev) -> Option<EndReason>` (`None` until end); `dispose` is optional cleanup config (example omits it); `Id::new(n)` returns `Option` (zero → `None`, use `let Some..else Validation`) |
| collector `end` (time/idle/limit) + delete-ends | `standby::EndReason::{Time, Idle, Limit, MessageDelete, ChannelDelete, ...}` — `time` = total since creation (non-`Option`, pass `Duration` or `default()`), `idle` = since last collected item (`Option`); `Time`/`Idle` expiry need real waiting (short-duration probes in tests); `ev.kind()` for `Resumed` prints `OTHER` |
| collector defaults + priority (measured) | `CollectorConfig::default()` = `{ max: Some(100), time: 60s, idle: None }` — rely on it for delete-ends, go explicit for timeout tests. Both-expired → `Time` wins. Idle scoping: idle needs ≥1 collected item ONLY for the both-expired tie (unseeded both-expired → `Time`); idle-only expiry (long `time` + short `idle`, sleep past `idle` but not `time`) → `Idle` with `items=0` even unseeded. Polling alone stays `None` — `end_reason()` surfaces only after a `push`; expiring `push` collects nothing (`items=0` unseeded). |

## Cache & sweepers

```js
// discord.js
new Client({ makeCache: Options.cacheWithLimits({ MessageManager: 50 }), sweepers: {...} });
```

```rust,no_run
// rivulus — presets are builder fns, not separate types.
let client = rivulus::Client::builder(String::from("t"))
    .cache(|_| rivulus::cache::CacheConfig::balanced())
    .build()?;
```

| discord.js | rivulus |
|---|---|
| `Options.cacheWithLimits` / `makeCache` | `cache::CacheConfig::{minimal(), balanced(), full()}` — presets: `minimal` = GUILD\|CHANNEL\|ROLE\|USER, `message_limit=0`; `balanced` adds MEMBER\|VOICE_STATE\|THREAD, `message_limit=0`; `full` adds MESSAGE\|REACTION\|PRESENCE\|THREAD_MEMBER\|STAGE\|SCHEDULED_EVENT\|EMOJI\|STICKER\|INVITE\|SOUNDBOARD\|ENTITLEMENT\|AUTOMOD, `message_limit=50`. `Cache` trait must be in scope (`use cache::{Cache, CacheConfig, InMemoryCache}`); `hit_ratio()` is on `CacheStats`, i.e. `cache.stats().hit_ratio()` (not `cache.hit_ratio()` — E0599); empty pre-miss `hit_ratio()=1.00` (0/0 guard — holds only before any `get_*` miss; after 1 miss it drops below 1.00); `CacheStats` misses increment on every cache-miss `get_*` |
| `sweepers` (messages, threads, ...) | `cache::sweeper::Sweeper::spawn(cache, SweeperConfig { interval_secs, message_max_age_secs })` — BOTH knobs required (not one); `sweep_once(&cache)` returns removed count; `CacheConfig::sweep_interval_secs` vs `SweeperConfig::interval_secs` are separate (config vs task); `UNIX_EPOCH`-old mocks get swept (removed 1) while `now_utc()` mocks survive |
| `Collection` | std maps + `cache::CacheView` — lookups are `get_guild` / `get_channel` / `get_message` / `get_member(guild, user)` (not `get(id)`); `CacheStats { guilds, channels, roles, users, members, messages, hit, miss }` + `.hit_ratio()` |

## Sharding

```js
// discord.js
new ShardingManager('./bot.js', { totalShards: 'auto' });
```

```rust,no_run
// rivulus — live Auto chain (not hardcoded):
// 1. GET /gateway/bot via rest::Client -> GetGatewayBotResponse
//    { url, shards, session_start_limit: { total, remaining, reset_after,
//      max_concurrency } }
// 2. let total = gateway::recommended_shards(&bot); // pure helper
// 3. let config = gateway::ClusterConfig::new(total, max_concurrency); // (total, max_concurrency) order
// 4. let order = gateway::ordered_start_list(total, max_concurrency); // identify buckets
// 5. per shard: gateway::bucket(shard_id, max_concurrency) // = shard % max_concurrency, 1 identify / 5s / key
// 6. let ids = gateway::shard_ids(&config); // Vec<ShardId { id, total }>
// Client::builder(..).sharding(Auto).build()?.login() runs this chain
// internally (RestBootstrap) and spawns Cluster in bucket order.
let client = rivulus::Client::builder(String::from("t"))
    .sharding(rivulus::gateway::ShardStrategy::Auto)
    .build()?;
let order = rivulus::gateway::ordered_start_list(4, 2); // example values only
```

| discord.js | rivulus |
|---|---|
| `ShardingManager` + `ShardClientUtil` | `gateway::ClusterConfig` + `gateway::Cluster` |
| identify rate limits (1/5s/key) | `gateway::bucket(shard, max_concurrency)` + `ordered_start_list` |
| `fetchRecommendedShards` | `gateway::recommended_shards(&bot_response)` |

## Errors & resilience (read-only; Cluster owns reconnect)

| discord.js | rivulus |
|---|---|
| 429 handling | `common::Error::RateLimited { bucket, retry_after_secs: f64, global: bool }` after up-to-3 internal waits (Reset-After preferred); catch via `matches!(e, Error::RateLimited { .. })` — Display is `rate limited bucket=.. retry_after=..s global=..`. No `retry_after` helper in docs scope; pace read-only fetches (~200ms) and never hammer. Quiet guilds trip no 429 (404s instead for bad IDs). |
| InvalidSession | gateway `InvalidSession { resumable: bool }` — `true` → resume may succeed, `false` → fresh Identify. No resume-URL/seq API in docs scope; `Cluster` supervises internally. None observed on stable live runs. |
| close codes (4007 rule) | `common::Error::Gateway { code: CloseCode(u16), can_resume, help }` + `CloseCode` at `common::error::CloseCode`; `help()` is computed from code (the struct `help` field is ignored): 4014 → privileged-intents, 4010 → shard math, `Unauthorized` → verify token then stop, everything else → `see docs`. 4007 → fresh Identify (no resume); miss-2-ACKs → reconnect. No close-code table in v1 docs beyond this row — see `common::error` docs. |
| `Error::help()` table | Specific (not `see docs`): `Gateway 4014` → privileged-intents, `Gateway 4010` → shard math, `Unauthorized` → verify token then stop, `VoiceStub` → `voice is stub-only in v1` (Display itself points at `plans/09 D6`, help returns the stub line). Generic `see docs`: `Gateway` (other codes incl 4007/4000/4011 — the 4007 fresh-Identify rule is docs-text only, NOT encoded in `help()`), `Forbidden`, `RateLimited`, `Rest` (all statuses), `NotFound`, `Timeout`, `Network`, `Deserialize`, `Validation`, `Config`, `CacheMiss`, `ShardDown`, `ShardBackpressure` (note: full name — bare `Backpressure` does not exist, E0599), `SignatureInvalid`, `InvalidSession`. Full `Rest` shape (docs list only `errors` — needs all four): `Rest { status: u16, code: i64, message: Box<str>, errors: Option<Box<str>> }` (missing `status`/`code`/`message` → E0063). Other exact shapes: `Config(Box::from(..))` is a TUPLE variant (no `reason` field — E0559); `Gateway { code: CloseCode(x), can_resume: bool, help: &"static str }` (help is `&str`, not `Box<str>` — E0308; and the field is IGNORED, `help()` recomputes from code); `CloseCode` wraps `u16` (`CloseCode(4014)`, not bare int); `retry_after_secs: f64` (not int); `#[non_exhaustive]` → `match` needs `..`/`_` (construction itself needs no `..`). |
| 30s READY / shutdown | `login()` fails after 30s without READY (`Timeout("READY timeout")`); wrap with `tokio::time::timeout` + `login_elapsed_ms` logging. `shutdown()` cancels the shared token and drains ≤5s — it is TERMINAL (re-login after shutdown times out; rebuild `Client` instead). |
| redaction | Tokens are `secrecy::SecretString` (never `Display`/`Debug`-printed; `Client` debug shows `***`). `Error` Displays never include tokens — audit own prints for `DISCORD_TOKEN` substrings; log `token_len` at most, never the value. |

## Explicit v1 exclusions

Activities/RPC, Social SDK, Lobby (track only), voice send/recv (stub +
spec only, `voice-dave` experimental later), ETF (JSON only v1),
`interactions-axum` as example-only (not a crate feature).
