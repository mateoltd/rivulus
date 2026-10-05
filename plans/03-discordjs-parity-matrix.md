# 03 — discord.js Parity Matrix (short namespaces)

Goal: discord.js dev finds every concept in <60s. Docs always show `djs -> rivulus::` table with SHORT imports.

## 1. Client & options
| discord.js | rivulus (facade + short libs) |
|---|---|
| `new Client({intents, partials, shards, rest, sweepers, makeCache})` | `rivulus::Client::builder(tok).intents().partials().cache().rest().sharding()` |
| `client.login(token)` / `destroy()` | `client.login().await` / `client.shutdown().await` + `ShutdownHandle` (CancellationToken) |
| `client.ws`, `client.rest`, `client.options` | `ctx.shard: gateway::ShardMessenger`, `ctx.http: Arc<rest::Client>`, `client.config()` |
| `ShardClientUtil`, `ShardingManager`, `broadcastEval` | `gateway::Cluster` (spawn/respawn/`broadcast()`) |
| `readyAt, uptime, ping(s)` | `client.ready_at()/uptime()/latencies()` per shard |
| `fetchRecommendedShards, generateInvite, OAuth2Scopes` | `gateway::recommended_shards()`, `common::oauth::generate_invite()`, `model::oauth::Scope` |

## 2. Events (~90, typed `model::Event`)
channelCreate/Update/Delete/PinsUpdate, threadCreate/Update/Delete/ListSync/MemberUpdate/MembersUpdate, guildCreate/Update/Delete/Available/Unavailable, memberAdd/Remove/Update/Chunk/Available, roleCreate/Update/Delete, emojiCreate/Update/Delete, stickerCreate/Update/Delete, inviteCreate/Delete, messageCreate/Update/Delete/BulkDelete, reactionAdd/Remove/RemoveAll/RemoveEmoji, presenceUpdate, typingStart, voiceStateUpdate/voiceServerUpdate (correlated), interactionCreate, autoModerationRuleCreate/Update/Delete + ActionExecution, auditLogEntryCreate, entitlementCreate/Update/Delete, scheduledEventCreate/Update/Delete/UserAdd/Remove, pollVoteAdd/Remove, stageInstanceCreate/Update/Delete, soundboardSounds + Create/Update/Delete, subscriptionCreate/Update/Delete, webhooksUpdate, warn/debug/error. Lifecycle `shardReady/Disconnect/Reconnect/Resume` are SYNTHETIC `gateway::ShardEvent` (NOT Discord dispatch, NOT in `model::Event` — `model` stays IO-free); dispatcher forwards both, cache sees only `model::Event`. Client-level synthetic events: `Event::RateLimited { route_template, status, retry_after }`, `Event::Raw { op, t, s, payload }`, `Event::CacheSwept { resource, count }` (facade callbacks wiring `rest_429_total`/`cache_hit_ratio`). `*_Update` carry `(old: Option<Arc<T>>, new: Arc<T>)` via cache diff. `Event::Unknown { kind: String, seq, payload: Box<str> }` retained + counted, never panics.

## 3. Managers -> fns on `Context`
Every manager exposes sync `get(id) -> Option<Arc<T>>` (cache) + async `fetch(id) -> Result<Arc<T>>` (REST) — never fetch-only. Guild/Channel/Message/Member/Role/User/Reaction/Thread/Webhook/Attachment/Poll/Invite/Emoji/Sticker/ScheduledEvent/AutoMod/Entitlement/AuditLog/Onboarding/Stage/Soundboard managers -> `ctx.guilds().fetch/create/edit/delete/prune`, `ctx.messages().send/edit/delete/pin/bulk_delete` (bulk: 2-100, <14d enforced client-side), `ctx.threads().archive/list/members`, `ctx.webhooks().execute(...?wait&thread_id)`, etc. Pagination dual API: `fetch_page() -> Vec` AND `stream_pages() -> Stream`. File uploads via `CreateAttachment { filename, bytes, description }` + `attachment://` refs in embeds/components.

## 4. Interactions & builders (`model` + `interactions` + `rest`)
Slash/User/Message commands; options string/int/number/bool/user/channel/role/mentionable/attachment + subcommand/subcommand-group; `name_localizations/description_localizations`; `default_member_permissions` (bitwise string), `dm_permission`, `nsfw`, `contexts [Guild=0,BotDM=1,PrivateChannel=2]`, `integration_types [GuildInstall=0,UserInstall=1]`, entry-point handler. Interaction types 1-5 + callbacks 1,4,5,6,7,8,9,10,12 (no 11; full numbers in 04D): Pong/ChannelMessage/DeferredChannel/DeferredUpdate/UpdateMessage/AutocompleteResult/Modal/PremiumRequired/LaunchActivity. Lifecycle: ACK <=3s or defer; token valid 15min; followups via `webhooks/{app_id}/{token}`; Autocomplete <=25 choices; Modal `custom_id` routing; Components V2 incl. TextDisplay/Section/Thumbnail/MediaGallery/File/Separator/Container + `is_components_v2` flag + limits (<=5 rows x 5, content <=2000, embeds <=10/6000). Signature `ed25519(timestamp+body)` for webhook mode (axum example only, not a feature). Registration: bulk-overwrite PUT global+guild + diff log.

## 5. Collectors -> `standby` (full djs semantics)
`MessageCollector, ReactionCollector, InteractionCollector, awaitMessages/awaitReactions/awaitModalSubmit` -> `standby::Standby::wait_for(pred, timeout)`, `wait_for_message_in`, `wait_for_reaction_on`, `await_modal_submit(custom_id)`, `stream_for(...) -> impl Stream`, `Collector { filter, max, time, idle, dispose }: Stream<Item=Collected> + end_reason()`. `EndReason { Time, Idle, Limit, User, MessageDelete, ChannelDelete, GuildDelete, ThreadDelete }` — collector auto-ends on delete events. Idle resets per item; dispose removes on reaction-remove. `tests/collectors.rs` asserts filter+max+idle+timeout+end-reason+delete-ends. Matching: `Waiter { kind: EventKind, predicate }` shards by event-type + short-circuits (never O(waiters x events) full-clone fan-out).

## 6. Cache & sweepers & partials (`cache`)
`Options.cacheX/sweepers/Partials` -> `cache::ResourceType` bitflags + per-resource caps (`message_limit` per-channel VecDeque default 0=off, `member_high_water` LRU, `presence` default off) + TTL `sweeper { interval 60s, max_age msg 1h, typing 10s }` sharded `retain` (no full-scan block) + metrics `cache_hit_ratio`. `Partial { User, Message, Channel, GuildMember, Reaction }` for uncached dispatch; `lazy_fetch_partials` bounded (timeout + semaphore, never unbounded spawn). Presets via builder only: `minimal/balanced/full` are builder fns, not separate types. Default mirrors doc RAM table (05 section 3).

## 7. Formatters, utils
`userMention/channelMention/bold/codeBlock/time/escapeMarkdown` -> `rivulus::format::*`. `SnowflakeUtil::timestamp(id)`, `Collection` -> std maps + `cache::CacheView`. Voice: STUB only v1 (see 09 D6); docs state deferred.

