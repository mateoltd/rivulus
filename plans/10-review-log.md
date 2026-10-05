# 10 — Review Log & Disposition (independent + SpaceBunny XHigh)

Two subagent reviews ran 2026-09-26 (third aborted, no output — not counted). This log records disposition so agents don't re-litigate.

## Independent senior review — applied
- Outlines (~30 lines/file) not shippable -> FIXED: 04/05/08 expanded to normative contracts (intents bits, chunk reassembly, route list, close codes, caps).
- Gateway inaccuracies (op7/op9 confusion, presence rl, GUILD_CREATE dance) -> FIXED in 04A (opcodes 0,1,2,3,4,6,7,8,9,10,11; no voice ops; `d:bool` handling; chunk nonce/index/count/not_found).
- Ratelimiter vagueness (bucket key, shared/global, ban-meter) -> FIXED in 04B/06§3 (method+template+major, Bucket authoritative, shared excluded, global semaphore, ban-meter thresholds).
- Cache fantasy (hash-compare, Arc<User> embed) -> FIXED in 04C/05§2 (canonical users map, author_id lookup, Arc-clone diff, sharded retain).
- Interactions V1-only -> FIXED in 03§4/04D (callbacks 1,4,5,6,7,8,9,10,12; 3s ACK; 15min token; contexts/integration_types; V2 components; <=25 autocomplete).
- Voice over-scope -> FIXED: deferred per owner (09 D6); 04E = stub + future spec only; P7 stub acceptance.
- Untestable gates (72h/30min/10k absolute) -> FIXED in 01§4/07/08 (short chaos + #[ignore] live/soak-72h + tolerance bands).
- Layering (gateway->rest, error in model, axum feature) -> FIXED in 02§5 (Bootstrap trait, Error in core, axum example-only).
- Features/MSRV vagueness -> FIXED in 09 (no simd/zstd/dave/axum features v1; MSRV 1.75 pinned).
- REJECTED (out of scope for owner locks): papaya-as-default, simd-json v1, zstd v1, `model-full` feature name (no such feature now).

## SpaceBunny XHigh — applied
1. WS buffer-reuse/zero-copy fiction (tungstenite owns buffer; borrowed Event can't cross await) -> FIXED: owned Event norm (05§2), header-peek only, P3 spike single-stack decision.
2. `Arc<str>` != interning -> FIXED: canonical users map + Box<str> cold default (02§2/04C/05§2).
3. Allocator/profiling absence -> FIXED: entry-overhead note + flame/alloc-count benches (05§§1,4); no jemalloc v1 (documented non-goal, std allocator).
4. DashMap vs papaya + global RwLock -> FIXED: dashmap6+foldhash locked (09 D4), papaya P10 spike only, no global RwLock (05§5).
5. broadcast/try_send-drop event loss -> FIXED: lag-aware broadcast + backpressure errors, never silent drop (05§2/08 P5).
6. Global ratelimiter contention -> FIXED: semaphore refill documented (04B).
7. Token leakage (String Debug) -> FIXED: SecretString + redaction (02§4/06§1/08 P8).
8. Multipart/audit-injection -> FIXED: audit-reason validation + payload_json (04B/06§4).
9. Builder I/O surprise + single-handler + Stream-only pagination -> FIXED: sync build, add_handler multi + Fn blanket, dual pagination (02§4).
10. `#[serde(other)]` on u8 + full-guild diff + unbounded lazy-fetch -> FIXED: manual Unknown(u8), Arc diff, bounded fetch (04C/06§4).
11. MSRV 1.75 vs async-trait/papaya/OnceLock -> FIXED: RPITIT + std OnceLock + MSRV verify task (02§2/08 P0); async-trait rejected.
12. Bench CI flakiness + DAVE scope + voice nonce-reuse -> FIXED: tolerance bands (05§4), DAVE experimental-deferred (09 D6/04E), no voice v1.
- Nits applied: no `snowflake` crate (own Id), sync `build()`, `default=[gateway,rest,cache-inmemory]` (no model-full), central SendQueue, truncated error sources (500 chars), low-cardinality metric labels, grep-unwrap allows cfg(test), deny.toml diff gate.

## Aborted review (deespeek v4.1)
- Spawn aborted after 6 iterations, empty result. No findings to dispose. Re-run optionally post-P1a; not blocking.

## Remaining risks (accepted, tracked)
- WS crate final choice pending P3 spike (tungstenite default, websockets candidate).
- RAM table numbers pending P10 measurement (template only).
- `tokio::sync::broadcast` lag handling needs P5 integration test (specified, not yet proven).


## Round 2 (2026-09-26, post-lock) — applied
Three fresh reviews against the locked plan (short namespaces, reqwest/serde_json/dashmap6/MSRV-1.75 locked, voice deferred). All three confirmed NO hard blocker requires unlocking. Fixes applied:
- 4007 corrected (fresh Identify, never Resume) + exhaustive close-code table + `invalid_seq_fresh_identifies` test (04A/06§2/08 P3).
- `EventHandler` BoxFuture object-safe form + blanket impl + P5 multi-handler compile check (02§4/08 P1a).
- `model::gateway_bot` pure types + `gateway::Bootstrap/Queue` traits + `rest::Route {method/path_template/major_id}` (02§5/04A/08 P1a) — unblocks parallel P2/P3.
- WS URL exact + `compress` query-param + `resume_gateway_url` rule + jitter + `large_threshold` + Ready barrier + `remaining==0` sleep (04A).
- Decompression dual caps (frame 256K + message 8M) + resync loop + reset (04A); single-pass header peek, `Value` forbidden (04A/05§2).
- Ratelimit: Reset-After preference, Scope trio, semaphore refill note, std Mutex, `Route`-only bucket keys (04B).
- UA with URL, `webpki-roots` wiring + P0 TLS smoke, `secrecy/futures/time/percent-encoding` in lock text (02§2/09 D2/08 P0).
- Webhook tokens = secrets (redaction/tests), audit-reason CRLF reject, filename sanitize (04B/06§1).
- Interaction type numbers 1-5, component numbers incl. 9-14/17/18, V1/V2 split validation, `common::validate` shared (03§4/04D/06§4/08 P1).
- Intent aliases GUILD_BANS/GUILD_EXPRESSIONS, op8 presences-require-intent, chunk 10s timeout + out-of-order buffer, limit:0=all (04A).
- `ResourceType` (19 flags) + `Partial` (7 variants) enumerated; `UpdateCache` per-event modules; dyn-safe Cache; Unknown 8K cap (04C).
- Synthetic `gateway::ShardEvent` split from `model::Event`; client synthetic `RateLimited/Raw/CacheSwept` (03§2).
- Managers get+fetch dual, Collector EndReason + auto-end on deletes, Waiter kind-sharding (03§§3,5).
- Broadcast(2048) + per-kind Lagged policy + `tests/dispatcher_lag.rs` (05§2/08 P5); bench-only CountingAlloc (05§4); O(shards) wording (05§1).
- Observability: metric allowlist, trace-per-event/debug-dispatch, AtomicU64 request-ids (05§2/06§§1,5).
- Shutdown 5s drain, login 30s READY timeout, standby 1024/lazy-fetch sem-10/5s bounds, soak SLO (06§2).
- P1a verbatim freeze + mock `spawn()->SocketAddr` + gate; P0 no-toolchain-toml + geiger scope + facade-shadow grep (08 P0/P1a, 07§2).
- `framework` constrained (parser+registry+guards only), no voice example v1 (01§3/07§3/08).
- Fictional `GET /users/@me/guilds?shard` deleted (was never a Discord route).
