# DOGFOOD — fresh-consumer friction log (phased, offline-first)

Started: 2026-10-04 UTC | Soak: running (see `.tmp/soak-72h.log`) | Token: REDACTED everywhere, SecretString only, never stored.
Method: each agent is FRESH — README.md + MIGRATION.md + crates/rivulus/examples/* only. No crates/*/src, no .tmp/, no plans/, no live token. Offline mock-safe builds at /tmp/dogfood-*.

## Batch 1 (8 agents, all exit 0) — per-agent table

| # | Assignment | Crate | Time-to-first-RUN | Exit | Key friction (verbatim gáps) |
|---|------------|-------|-------------------|------|------------------------------|
| 1 | ping (struct+closure, fetch_page offline) | /tmp/dogfood-ping | ~3.7 min (222s) | 0 | README split across 3 files for full pattern; `client.context/ShardMessenger/health/latencies/ChannelId::new/fetch_page` only in ping.rs, not README/MIGRATION. No compiler errors. Last: `health ready: false` + `fetch_page (expected offline): unauthorized: check token` |
| 2 | slash-register + answer | /tmp/dogfood-slash | ~4.0 min (239s) | 0 | `examples/` link ambiguous (root vs crates/rivulus/examples); `bulk_overwrite_payload(&[ping])` moves value (no borrow pattern); downstream `features=["full"]` undocumented; `respond(content,bool)` 2nd arg unexplained; body-only bytes (len 58), no app_id/route helper. No compiler errors. Last: `payload len: 58 / first command name: ping / ack json: {"type":4,...}` |
| 3 | buttons-collector MessageDelete end | /tmp/dogfood-buttons | ~3.7 min | 0 | `examples/` path ambiguous; `full` vs `--all-features` unclear; `Collector{dispose}` in MIGRATION not in example; `ev.kind()` for Resumed prints OTHER. No errors. Last: `end_reason: Some(Limit) / collected: 2 / delete end_reason: Some(MessageDelete)` |
| 4 | sharded Auto | /tmp/dogfood-sharded | ~3.5 min (213s) | 0 | `examples/` path wrong; external-crate `path=` vs `rivulus="0.1"` missing; `Cluster (from_config,broadcast)` vs `Cluster spawns at login()` conflict; `ordered_start_list(4,2)` hardcoded, not wired to recommended_shards; bucket formula only in comment; `SessionStartLimit` fields + `shard_ids` shape undocumented. No errors. Last: `total 4 / ordered [0,1,2,3] / shard ids [...]` |
| 5 | cache-tuning minimal/full | /tmp/dogfood-cache | ~3.6 min (217s) | 0 | `Sweeper::spawn` needs 2 knobs (example-only); `get_guild/get_channel/get_message` vs MIGRATION `get(id)`; `Cache` trait import required for `stats()` (E0599); empty `hit_ratio=1.00` (0/0 guard). Verbatim E0599/E0308/E0063/E0618 (Id::new->Option, Guild/Channel/Message missing fields). Last: `guilds:1 channels:1 messages:1 hit:3 miss:1 hit_ratio:0.75 sweep_once:0` |
| 6 | webhook-verify-axum | /tmp/dogfood-webhook | ~4 min | 0 | `examples/` path wrong; `full` vs `interactions` feature unclear; filename implies axum but NO axum dep (comment sketch only); `PUBLIC_KEY_HEX` undefined types; `verify(pubkey,timestamp,body,sig)` types/concat rule missing (must cross-ref example). No build errors. Last: `ok / invalid-tampered (expected): signature invalid` |
| 7 | fetch_page + stream_pages | /tmp/dogfood-pages | ~2.8–3.6 min (217s) | 0 | Dual-API when-to-use undocumented; `MIGRATION: Every manager paginates` overclaims (only messages evidenced); `?` success-only vs `match` offline handling; `all runnable offline` unqualified (fetch_page errors offline); `fail with REST error` actual is auth `unauthorized: check token`. Gaps: limit 0->1, max 100, cursor before/after/around absent, live-only note only in ping.rs. Last: `fetch_page err: unauthorized... / stream_pages first err: unauthorized...` |
| 8 | modal + autocomplete | /tmp/dogfood-modal | ~4 min (237s) | 0 (fallback) | KEY GAP: modal submit + autocomplete NOT buildable from docs/examples. `ResponseKind::Modal/AutocompleteResult` named but no constructors/accessors/cap helpers; `standby::wait_for(kind,timeout,pred)` no kind/pred example; only callback-4 shown, no 8/9/5 shapes. Verbatim E0425 `respond_autocomplete/respond_modal not found`, E0618, E0559 `Modal has no field custom_id/content`. Fallback slash payload exit 0. Last: `GAP modal... / GAP autocomplete... / fallback OK` |

All 8 `cargo run` exit 0 (modal via slash fallback). No tokens used, no live calls, no internals read (per-agent attestations).

## Top-10 friction list (batch 1, ranked by fresh-consumer cost)

1. `examples/` path ambiguous (README `examples/` vs actual `crates/rivulus/examples/`) — hit by 6/8 agents. Fix: OPEN (one-line README link fix).
2. Modal + autocomplete unbuildable (no show/submit/text-input, no focused-option/choices responder, no callback 8/9/5 shapes) — agent 8 with compiler proof. Fix: OPEN (needs API + example + MIGRATION row).
3. `bulk_overwrite_payload` moves value, no borrow pattern; body-only bytes, no app_id/route PUT helper; `respond(content,bool)` 2nd arg unexplained; no `answer(id,token)` helper — agent 2. Fix: OPEN.
4. Cache discovery: `get_guild/get_channel/get_message` vs `get(id)`, `Cache` trait import for `stats()`, `Sweeper` 2-knob requirement, `resource_types` preset table missing, `hit_ratio` location — agent 5 (+E0599/E0063). Fix: OPEN (docs table + example import).
5. Dual-pagination semantics: one-shot vs stream, termination, cursor absence (no before/after/around), limit 0->1/max100, live-only note only in ping.rs — agent 7. Fix: OPEN.
6. Sharded Auto wiring: `ClusterConfig::new(total,max_concurrency)` order, `recommended_shards->ordered_start_list/shard_ids` chain, bucket formula, `SessionStartLimit` fields — agents 4 (docs split). Fix: OPEN.
7. Verify API: `verify()` types/concat (`timestamp||body`), hex formats, `MAX_BODY_BYTES`/`Validation` vs `SignatureInvalid`, feature gate — agent 6 (example-only). Fix: OPEN.
8. Collector semantics: `dispose`, `Time/Idle` expiry demo, `MessageDelete` Id::new Option constructors, `ev.kind()==OTHER` for Resumed — agent 3. Fix: OPEN.
9. `client.context/ShardMessenger/health/latencies/uptime/ChannelId::new` only in ping.rs, not README/MIGRATION — agent 1. Fix: OPEN (MIGRATION row).
10. Downstream `Cargo.toml features=["full"]` undocumented (guessed, worked) — agents 2/6. Fix: OPEN (README snippet).

## Fixed vs left open (batch 1 → batch 2 triage)

- Fixed: #1 `examples/` link (README `examples/` → `crates/rivulus/examples/`, fmt clean). TLS `manual-roots -> webpki-roots` was prior live-login turn.
- Filed as owner decisions (needs API design, not docs-only): #2 modal/autocomplete (no constructors/accessors/cap helpers — compiler-proof E0425/E0559), #3 slash PUT helper (`answer(id,token)`, guild PUT route, `respond(bool)` semantics).
- Left open (docs-only, batch-3 candidates): #4 cache table, #5 pagination semantics, #6 sharded wiring, #7 verify concat, #8 collector dispose/Time/Idle, #9 context/health row, #10 downstream features snippet.
- Next batches (owner scale tens-to-hundreds, max ~20 concurrent): batch 3 = 12 agents (rate-limit 429 handling, InvalidSession resume, close-code 4007 matrix, chunk reassembly, gateway resume drop→resume, audit builders, multipart, framework guards, voice-stub negative, dispatcher_lag policy, standby timeout/idle matrix, error help() table). Loop batches, aggregate here.

## Batch 2 — COMPLEX live (5 agents, fresh docs-only builders, live-run in main thread, token isolated)

Discipline: builders NEVER saw token (offline `cargo build` only, `printenv DISCORD_TOKEN` empty per attestations). Live runs in main thread only via `DISCORD_TOKEN=REDACTED env`, SecretString, redacted logs (no token in any output/log, grep exit 1). Discord concurrency respected (sequential, 6s gaps, max_concurrency=1). Test-guild limits (honest, not hidden): 1 guild (djs 100+ blocker stands), 4 channels, 0 messages (quiet channel — busy-channel fetch returns 0).

| Track | Builder time | Build | Live evidence (redacted) | Complexity notes |
|-------|--------------|-------|--------------------------|------------------|
| A sharded Auto | ~9 min (4 probes) | exit 0, `/tmp/target/debug/dogfood-complex-sharded` | `recommended_shards=1 session_remaining=996 ClusterConfig{total:1,max_concurrency:1} ordered=[0] bucket 0->0 READY=yes login=697ms lat=[0/1:0ms] health ready=true status=Disconnected seq=None guilds_cached=0` EXIT 0 | Live Auto sizing from GET /gateway/bot (not mock). Supervisor restarts: none needed (stable). Per-shard latencies placeholder (known). Many-guilds: BLOCKED (1 guild). Gaps: no documented rest fetch method (E0599 `Client::new/get_gateway_bot` guesses), Strict `String` token docs (no SecretString guidance). |
| B stateful | ~11.6 min wall (1.1 code) | exit 0, `dogfood-complex-stateful` | `minimal/balanced/full types+limits printed hit_ratio=1.00 sweep_once=0 client cache limit=0` offline; `wait_for OTHER Collector Limit(2) MessageDelete` ok; `guilds 2 mock / channels 2 mock` offline; `DISCORD_TOKEN present (redacted) live login ok shutdown done` EXIT 0 | Cache minimal vs full under real traffic: traffic is quiet (0 msgs) so hit_ratio stays 1.00 (empty-guard) — honest limit. Standby+button collectors with timeouts/deletes exercised (Limit + MessageDelete). fetch_page+stream_pages first-item bound coded; live fetch after login deferred to soak (quiet channel returns 0). |
| C interactions | ~10.9 min | exit 0, `dogfood-complex-interact` (60169240 B) | `bulk bytes=115 names=ping,pong diff added:pong ack={"type":4,...} modal GAP autocomplete GAP framework ping ran registered=1 verify=invalid(expected) closed=true payload-only (no DISCORD_GUILD_ID, skipping PUT)` EXIT 0 | Slash register via bulk_overwrite + answer shapes live-ready; PUT intentionally payload-only (no guild id → safe default, no writes this turn). Modal/autocomplete compiler-proof missing (`ResponseKind::AutocompleteResult(vec)` E0618, `Registry::guard` E0599, `bulk_overwrite_guild_commands` E0599). Guards: `parse_args/Registry/dispatch(Permissions::empty())` exercised. ed25519 verify hit (invalid expected). |
| D resilience | ~11 min (E0502 fix) | exit 0, `dogfood-complex-resil` (2nd try) | `READY login=573ms killed(shutdown) once resuming via re-login → resume login error: timeout: READY timeout` + `fetch 0..4 error: not found: /channels/{channel_id}/messages (backoff-ready)` + `redaction_ok=yes` EXIT 0 | Kill-and-resume: shutdown is TERMINAL (shared CancellationToken) — re-login after shutdown times out (real finding, not silently restarted). 429: no 429 tripped (404s, mock channel) but backoff-ready path exercised, max 5 paced fetches, never hammered. 30s READY discipline via timeout wrapper (docs show bare `login().await`). Redaction verified (len-only, value never printed, output audit). |
| E adversarial | ~0.6 min | exit 0, `cargo run --offline` (no token) | `content 2000 ok / 2001+ err="validation: content > 2000 chars" (not 4000 as tasked) emoji5000 err same / audit 600 err="must be 1-512 chars" valid "ok reason"->"ok%20reason" / Event::unknown kind always UNKNOWN / unknowns 37 cap 10 capped / adversarial_ok=yes panics=0 process_alive=yes` EXIT 0 | Malformed/unknown/emoji/audit: no panic, count-and-cap enforced (UNKNOWN_CAP=10). Gaps: `validate` path is `common::validate` (E0433), `truncate_source` not found (6 probes), `Event: Deserialize/from_json` missing (use `Event::unknown(kind,seq,payload)` 3-arg), `gateway::{OpCode,CloseCode}` missing (E0425). Validation truth: content limit 2000 (not 4000), audit 1–512 percent-encoded. |

LIVE totals this turn: 4 live logins (sharded/stateful/interact-payload/resilience incl. failed resume) + soak holder = session_remaining 996→~995 range (1000 budget, plenty). No 401/403 (token valid). No 429 tripped live (quiet guild). No panics. No token in any log/output (grep exit 1 on soak log + DOGFOOD.md).

## Soak pointer (checkpoint, no restart)

- `soak_short` smoke: 1 passed (seeded chaos, <30s).
- 72h live soak: RUNNING PID 464269, t=5735s (~95min), 96 heartbeats, `guilds_raw=1 channels=4 messages=0` every 5min, no drops. No token in log (grep exit 1). Checkpoints in turn reports. Reconnect/resume (4007-fresh, miss-2-ACKs) owned by `gateway::Cluster`, observed-only (no silent restarts).

## Batch 3 — 12 agents (fresh docs-only builders, live-run in main, token isolated)

Builders NEVER saw token (offline `cargo build` only). Live runs sequential, 6s gaps, redacted. Offline-only 7 ran offline in subagents (exit 0 per attestations).

| # | Track | Build | Live evidence (redacted) | Complexity |
|---|-------|-------|--------------------------|------------|
| B3-1 | 429 rapid 5×fetch | 0.03min, exit 0 | `5× error: not found: /channels/{channel_id}/messages (mock ch 3) ok=0 ratelimited_like=0` EXIT 0 | No 429 tripped (404s, quiet guild). Gap: no `Error::RateLimited` strings in docs; string-match workaround. Discovery gap: mock ch, not live discovery. |
| B3-2 | InvalidSession | 0.02min, exit 0 | `offline shards 4 ordered [0..3] / live login READY observed-or-none InvalidSession none-observed shutdown complete` EXIT 0 | No InvalidSession live (stable). Gap: zero InvalidSession/resumable docs. |
| B3-3 | close 4007 | 0.42min wall, exit 0 | `offline shards 4 buckets / live: NO login attempted (builder only math) none-observed` — PARTIAL | Gap: no 4007/fresh-Identify/can_resume docs. Builder partial noted. |
| B3-4 | chunk | 0.02min (+E0599 fix), exit 0 | `presets printed / GAP chunk-request-api NOT FOUND / fallback fetch / logged-in chunk_received=0 shutdown ok` EXIT 0 | No chunk API (0 hits). `Cache` import E0599 (same as #4). |
| B3-5 | resume observe | 0.03min, exit 0 | `READY 697ms still_ready=true after 5s no-drop / ShardEvent NOT in model::Event` EXIT 0 | Stable, no drop. Shutdown NOT called (terminal per batch-2 — correct). Gap: no resume/drop API docs. |
| B3-6 | audit builders | 0.006min, exit 0 offline | `message str {...} webhook query '' bytes 21 audit ok (no send)` | Gaps: `embed(CreateEmbed)` vs Embed E0308, `.query(true,None)` is getter E0061, build double-encode trap. |
| B3-7 | multipart | 0.5min, exit 0 offline | `files[0] hello.txt 16B total 371B boundary ----RivulusBoundaryB3M7` | Gap: zero multipart docs (guessed from spec). |
| B3-8 | framework guards | ~2min, exit 0 offline | `nope→DENY NotFound / ping allow / mod allow / ADMIN allow` | Gap: no Guard/all/any (E0433/E0425), single Option<Permissions> only. |
| B3-9 | voice negative | ~1–2min, exit 0 offline | `JoinConfig ok / VoiceStub (no panic)` | Gap: variant unnamed (E0532 leak), no trigger fn (E0425 ×6). |
| B3-10 | dispatcher lag | ~12min, exit 0 offline | `256× Resumed flood done / lag UNKNOWN (dispatch()->(), no policy in docs)` | Gaps: dispatch Event vs Arc E0308, no LagPolicy/broadcast (E0599 ×6). |
| B3-11 | standby matrix | 0.5min, exit 0 offline | `Limit/Time/Idle/MessageDelete all Some DONE 4` | Time=total vs idle=since-last confirmed. Gaps: push Option, defaults, priority. |
| B3-12 | error help | ~12min, exit 0 offline | `17-line table: 4014→intents, 4010→shard math, Unauthorized→verify token, 8/11→see docs` | Gap: 8/11 help=`see docs` with no docs; Gateway help field ignored. |

## ETC live smoke (main thread, guild-scoped, redacted)

- Slash PUT (guild POST CreateGuildCommand): `app_id_present=true (decoded, never printed token) guilds_raw=1 slash_put=yes has_id=true name=dogfood-smoke-ping` EXIT 0. Guild ALLOWS (no 403). One smoke command left (clearly named, safe to delete). Bulk-PUT route gap stands (used POST single).
- Button live (no writes): `login READY=yes` then `wait_for Any 10s pred=false → ended error=timeout: standby timeout` EXIT 0. Quiet guild → Timeout proven live; clicks not testable (0 interactions). No messages sent.
- djs-equiv: still BLOCKED (1 guild vs 100+ needed). No measurement.

## Triage updates (batch-4 docs-fix round closes)

- FIXED batch-4 (worker-doable, MIGRATION.md + README.md only, no API change; evidence: fmt 0, clippy 0, test 328/0, doc 0):
  - #4 cache table (get_guild/get_channel/get_message/get(guild,user), Cache import, presets with limits, Sweeper 2 knobs, hit_ratio 1.00 guard, CacheStats fields)
  - #5 pagination (one-shot vs single-page stream, no cursor, 0→1 clamp/100 cap, live-only mock-{} note, @me/guilds partial owner_id fallback)
  - #6 sharded wiring (GET /gateway/bot → recommended_shards → ClusterConfig::new(total,max_concurrency) order → ordered_start_list → bucket=shard%max_concurrency → shard_ids; SessionStartLimit fields)
  - #7 verify concat (pubkey 64hex/sig 128hex lowercase no-0x, timestamp opaque &str, body &[u8], timestamp||body concat, MAX_BODY_BYTES→Validation vs SignatureInvalid, full/interactions gate)
  - #8 collector dispose/Time/Idle (push()->Option, dispose optional, time=total non-Option vs idle=since-last Option, Id::new Option, kind()==OTHER for Resumed)
  - #9 context/health row (ShardMessenger ctor, latencies placeholder, health fields incl Disconnected/None/guilds_cached, ready_at/shutdown_handle)
  - #10 downstream features snippet (rivulus full + tokio + Cache import copy-paste)
  - B3 strings: 429 shape (`RateLimited { bucket, retry_after_secs f64, global }`, 3 internal waits, pacing guidance), InvalidSession resumable, 4007 fresh-Identify + miss-2-ACKs + help() computed-from-code table (4014/4010/Unauthorized specific, 8/11 see-docs listed with exact shapes + non_exhaustive note), 30s READY/shutdown-terminal, redaction (SecretString ***, len-only), audit embed/query traps (CreateEmbed not Embed, query() getter, build double-encode), multipart row (no v1 builder), bulk borrow trap + PUT route note (POST single vs bulk array, no bulk Route helper — helper itself stays owner-decision)
- FILED owner-decision (untouched, list as-is): modal/autocomplete API, slash answer/PUT helper, chunk request API, voice trigger fn, lag_policy/broadcast_capacity, guard combinators.
- OPEN: none worker-doable — all batch-1/3 docs gaps now FIXED above. Remaining are owner-decision APIs + live soak/dogfood scale (tens-to-hundreds per bar-raising; batch-1 8 + batch-2 5 + batch-3 12 = 25 agents so far).
- Loop closes when owner accepts filed APIs (or requests implementation) + soak continues (no restart).

## Batch 5 — scale round (20 agents; total 8+5+12+20=45)

Builders NEVER saw token (offline build only). Live runs sequential 6s gaps, redacted. Soak untouched (running).

| # | Track | Build | Live evidence (redacted) |
|---|-------|-------|--------------------------|
| B5-1 | ratelimit-a | 0.5min exit 0 | `health false→true / raw parsers guilds=0/ch=0 / 5×404 ch=3 ok=0 ratelimited=0` EXIT 0. No 429 (bad ID). Gap: no raw GET example, SecretString pattern. |
| B5-2 | invalid matrix | <2min exit 0 | `READY 1261ms InvalidSession none 429 none shutdown ok` EXIT 0. Stable. |
| B5-3 | chunk-a | ~1min exit 0 | `login 1145ms ready true / guilds=1 / members fetch_page err=not found /members` EXIT 0. Chunk API gap stands (fallback). |
| B5-4 | chunk-b | 0.5min exit 0 | `login 659ms / GUILD_ID 0 → skip fetch` PARTIAL (needs env/raw discovery). |
| B5-5 | audit-send | 1.5min (E0422/E0308/E0599/E0061 fixed) exit 0 | `login 647ms / guild discovery err owner_id (typed gap) → no send` BLOCKED pre-send (needs raw fallback). Findings: `send(ch,&str)` not builder, `ShardId` in gateway not model, `DeleteMessage{u64,u64}+execute(3 args)->Vec<u8>`. |
| B5-6 | multipart-send | ~2min (E0308/E0559/E0599 fixed) exit 0 | `payload 59B file 18B total 350B / payload-only (no channel ID → no send)` BLOCKED (needs channel ID + multipart POST Route — none exists). Findings: `build()->Result`, `Config(Box<str>)`, no single `delete` (only `bulk_delete(&[Id])`). |
| B5-7 | cache-traffic | 0.4min exit 0 | `presets printed / live stats 0/0 hit 1.00 sweep 0` EXIT 0. Quiet guild, no traffic (honest). |
| B5-8 | pages-cursor | ~1min (E0753 fixed) exit 0 | `login 951ms raw_guilds=1 raw_ch=4 fetch 0+0 stream 0 second=ends identical cursor_absent=yes errs=0` EXIT 0. STRONG cursor-absence proof live. |
| B5-9 | standby-live | 0.3min exit 0 | `logged in / wait_for 8s pred=false → Timeout` EXIT 0. Quiet guild path live. |
| B5-10 | resume | ~2min exit 0 | `READY 565ms 5s observe no-drop (no shutdown, terminal)` EXIT 0. |
| B5-11 | guards-a | ~1min exit 0 (fallback) | `guards::and E0433 ×7 / and_guard E0425 ×8 / single-guard fallback ok`. Composition gap compiler-proof (owner-decision, untouched). |
| B5-12 | guards-b | 1.6min exit 0 | `ADMIN.contains(SEND)=false (bypass special) / secure+empty→Forbidden / missing+ADMIN→NotFound` + E0061 arg-order traps. |
| B5-13 | lag-a | ~1–2min exit 0 | `128 flood ok / lag_policy/broadcast E0599 ×4 / BROADCAST_CAPACITY=2048 / lag_policy(Resumed)=Count` (owner-decision surface, probed only). |
| B5-14 | lag-b | ~2min exit 0 | `ClientBuilder broadcast_capacity/lag_policy E0599 ×2 / gateway::LagPolicy E0433 / fallback dispatch ok`. |
| B5-15 | standby-def | ~1min exit 0 | `default() = { max: Some(100), time: 60s, idle: None }`. Defaults were undocumented (batch-4 did not add — NEW gap for batch-6). |
| B5-16 | standby-prio | 1.1min exit 0 | `Time wins when both expired (even idle shorter); Idle only after ≥1 item; polling alone stays None`. Priority undocumented (NEW gap). |
| B5-17 | errors-a | 0.5min exit 0 | `Config/ShardDown/ShardBackpressure need (Box<str>); Backpressure name wrong (is ShardBackpressure); VoiceStub help specific not see-docs`. |
| B5-18 | errors-b | 1.3min exit 0 | `InvalidSession T/F table / Gateway help ignores struct field / 4007 help=see docs (rule docs-only) / Box<str> vs &str E0308`. |
| B5-19 | audit-lens | 1.5min exit 0 | `CreateEmbed has NO footer/* (E0599 ×10); build()->Ok(Embed) not bytes; msg len 94`. Footer gap NEW (owner-decision? No — builder method gap, file for owner). |
| B5-20 | multipart-lens | ~1min exit 0 | `small 341B / large 1MB cap note (task-imposed) / no multipart example (MIGRATION one-liner correct)`. |

LIVE totals batch-5: 10 live logins (all READY yes, no 401/403, no 429, no InvalidSession, no 4007). Writes: NONE this batch (audit-send blocked pre-send, multipart payload-only; slash PUT write was prior turn). No panics. No token in outputs (len-only or redacted).

Count: batch-1 8 + batch-2 5 + batch-3 12 + batch-5 20 = 45 agents.

## Triage updates (batch-5 closes — scale, no fixes)

- FIXED this turn: none (scale round; batch-4 already closed worker-doable).
- NEW worker-doable gaps for batch-6 fix round: standby `default()` values, Time-wins priority + idle-anchor + poll-stays-None rules, `send(ch,&str)` vs builder confusion, `ShardId` path (gateway not model), `DeleteMessage{u64}+execute` shapes, `Config(Box<str>)`, `bulk_delete(&[Id])` (no single delete), `CreateEmbed::footer` absence, multipart boundary/header example, `register_guarded` arg order, ADMIN-bypass semantics, `Backpressure`→`ShardBackpressure` name, VoiceStub help specificity, Gateway `help` field ignored, `CloseCode`/`Box<str>`/`f64` shapes.
- FILED owner-decision (untouched, as-is): modal/autocomplete, slash helper, chunk API, voice trigger, lag_policy/broadcast, guard composition (B5-11 compiler-proof E0433/E0425).
- Soak: running untouched (checkpoint in turn report).

## Batch 6 — fix round (NEW gaps, docs-only, no API redesign)

Evidence (docs-only, one run covers all): `fmt --check` exit 0, `clippy --workspace --all-features -D warnings` exit 0, `test --workspace --all-features` 328/0, `doc -D warnings` exit 0. Soak untouched.

FIXED:
- Standby `default()={max:100,time:60s,idle:None}` + Time-wins + idle-needs-≥1-item + poll-stays-None rules (MIGRATION collectors row).
- `send(channel_id: ChannelId, content: &str)` vs builder (E0308), `CreateMessage::build()->Result<Vec<u8>>` + `CreateEmbed` builder form.
- `gateway::ShardId` path (E0422), `shutdown()` no-args/`()` + `let _ =` idiom, SecretString boundary note.
- `DeleteMessage{u64,u64}+execute(3 args)->Vec<u8>` + `.get()` on Id + `bulk_delete(&[Id])` only (no single delete).
- `Config(Box<str>)` tuple (E0559), `Gateway.help:&'static str` ignored + recomputed, `CloseCode(u16)` wrapper, `retry_after_secs:f64`, `Rest.errors:Option<Box<str>>`, `non_exhaustive` match-only.
- Footer absence (`title/desc/color(u32)` only, all footer/author/field/image E0599) + build return shapes.
- Multipart minimal example (payload_json + files[0] + boundary header; no POST Route helper).
- `register_guarded(name,handler,required)` 3-arg order (E0061) + `dispatch→Ok/Forbidden/NotFound` + missing stays NotFound + ADMIN-bypass-is-dispatch (`contains==false`) + no combinators (E0433/E0425, owner-decision untouched).
- `ShardBackpressure` full name (bare Backpressure E0599) + VoiceStub help specificity + Gateway help-field-ignored + 4007 docs-only note.
- (Batch-4 items remain FIXED; no regressions.)

FILED owner-decision (untouched, as-is): modal/autocomplete API, slash answer/PUT helper, chunk request API, voice trigger fn, lag_policy/broadcast_capacity, guard composition.
OPEN worker-doable: none.
Count: 45 agents (8+5+12+20). Soak running.

## Batch 7 — validation round (12 fresh validators, 12/12 GONE)

Each rebuilt from README+MIGRATION (+1 example) only, offline unless noted. Builders never saw token.

| Gap | Orig time | Valid time | Verdict | Notes |
|-----|-----------|------------|---------|-------|
| cache table | 3.6min (E0599/E0063) | 1.0min | GONE | No E0599/E0063. NEW minor: E0308 by-value Id (not &Id, 3 sites) + hit_ratio 0.00 empty (miss 3). |
| pagination | 2.8–3.6min | 0.3min | GONE | One-shot vs stream + clamp + live-only now copyable. NEW minor: MIGRATION cites ping.rs first-item but file is full-while. |
| sharded wiring | 3.5min | 0.5min | GONE | Full chain docs-sufficient. NEW: none. |
| verify concat | 4min | 0.5min | GONE | Types+concat+gate explicit. NEW: none. |
| collector dispose | 3.7min | 0.012min | GONE | Explicit cfg + push Option + delete end first-try. NEW: none. |
| context/health | 3.7min | 0.33min | GONE | Builder+messenger+health first-try. NEW: none. |
| features snippet | 4min guess | ~1min | GONE | Snippet works first try. NEW minor ×3: hit_ratio on CacheStats, bulk free-fn not method, Registry::new() 0 args. |
| standby defaults | ~1min undoc | ~2min | GONE | default() now seconds via row. NEW: idle-needs-item over-general (long/short unseeded → Idle, scoping needed). |
| send/delete | 1.5min (4 E-types) | 0.3min | GONE | Clean build, no E0422/E0308/E0599/E0061. NEW: none. |
| multipart | 0.5–1min guess | 1.5min | GONE | Shape sufficient first-try. NEW: none (slower = thorough read). |
| guards order | 1.6min (E0061) | 0.4min | GONE | 3-arg order first-try. NEW: none. |
| error-help | ~12min | 0.75min (16×) | GONE | All E0533/E0618/E0308/E0599 gone. NEW minor: Rest needs status/code/message (docs listed errors only). |

Confirmatory live (main, ≤3 allowed, 2 used): pages `login 1988ms READY guilds=1 ch=4 fetch 0+0 stream 0 ends cursor_absent=yes` EXIT 0; standby-live `logged in → Timeout` EXIT 0. No 401/403. No token in outputs.

Count: 8+5+12+20+12=57 agents. Soak running untouched.
NEW follow-ups for batch-8 (all minor, worker-doable): Id by-value, ping.rs first-item note, Registry::new/hit_ratio/bulk paths, standby idle scoping, Rest fields.
FILED owner-decision (untouched): modal/autocomplete, slash helper, chunk, voice trigger, lag_policy/broadcast, guard composition.

## Batch 8 — confirmatory rebuild (6 validators, 6/6 GONE)

| Note | Valid time | Verdict | Evidence |
|------|------------|---------|----------|
| Id by-value | ~1min | GONE | No E0308; by-value fetch/send/bulk_delete compile; offline errs documented (unauthorized, 2-100). |
| ping first-item | ~3min | GONE | MIGRATION now states full-while vs single-next; first-item pattern first-try. |
| Registry/hit_ratio/bulk | 0.5min | GONE | new() 0-arg + stats().hit_ratio()=1 + free-fn bulk first-try. |
| idle scoping | 0.4min | GONE | Idle-only→Idle (items=0), both-expired→Time. Refined row matches. |
| Rest fields | <1min | GONE | Full 4-field Rest compiles, no E0063. help=see docs. |
| combined sanity | ~6min | GONE | All five compile; NEW minor: handler must be Arc<dyn Fn(HandlerCtx)> (E0308 on ||"pong"), hit_ratio guard pre-miss only. Suggest docs example (polish, not blocker). |

Count: 57+6=63 agents. Soak running. Filed owner-decision untouched.

## Batch 9 — final polish validation (2 validators, 2/2 GONE)

| Note | Valid time | Verdict |
|------|------------|---------|
| Arc handler pattern | ~2min | GONE (copy-paste `Arc::new(\|_ctx: HandlerCtx\| {})` first-try, no E0308). |
| hit_ratio pre-miss guard | 0.4min | GONE (`pre=1.00 post=0.0000` after 1 miss, first-try). |

Evidence batch-9: fmt 0, clippy 0, test 328/0, doc 0 (docs-only MIGRATION edits).

Count: 63+2=65 agents.

## LIVE PHASE COMPLETE

Worker-doable: NONE remaining (all batch-1/3/5/7/8 + batch-9 polish FIXED + validated GONE).
Filed owner-decision (untouched, for owner accept): modal/autocomplete API, slash answer/PUT helper, chunk request API, voice trigger fn, lag_policy/broadcast_capacity, guard composition.
Soak: STILL RUNNING (72h, PID 464269) — checkpoints continue separately; do not close soak with this gate.
djs-equiv: BLOCKED (1 guild vs 100+ needed).
Live writes so far: guild slash POST `dogfood-smoke-ping` (prior turn); no message sends (audit/multipart blocked pre-send, honestly recorded).
