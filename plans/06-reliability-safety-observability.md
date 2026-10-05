# 06 — Reliability, Safety & Observability (`common::Error` only)

## 1. Error model (`common::Error`, `thiserror`; `model` has NO error type)
Variants: `Gateway { code: CloseCode, can_resume, help }`, `InvalidSession { resumable }`, `RateLimited { bucket: Box<str>, retry_after_secs: f64, global }`, `Rest { status: u16, code: i64, message: Box<str>, errors: Option<Box<str>> }`, `Unauthorized/Forbidden/NotFound` (no retry), `Timeout`, `Network`, `Deserialize { event: Box<str>, reason: Box<str> }` (source truncated 500 chars), `Validation(Box<str>)`, `CacheMiss`, `ShardDown/ShardBackpressure`, `SignatureInvalid`, `Config(Box<str>)`, `VoiceStub` (v1: "voice deferred, see 09 D6"). `help()` per variant (4014 -> portal steps; 4010 -> shard math). `common::Result<T>`. Secrets: `SecretString` for Bot token AND webhook URL tokens; `Debug` redacts both (`https://discord.com/api/webhooks/…` => `…/***`); `debug!` never logs `Identify.token`. Token-redaction test asserts `format!("{:?}", client)` never contains either. Audit-reason: reject `[\r\n]`, validate decoded 1-512 then percent-encode; multipart `filename` sanitized + file cap. Request-id: `AtomicU64`, not uuid. No silent drops: unknown dispatch -> `warn!` + `gateway_unknown_events_total`.

## 2. Reconnect/resume matrix (ordered, max_concurrency-aware)
| scenario | behavior |
|---|---|
| clean 1000 / no seq | fresh Identify + re-READY |
| Reconnect op7 / 4007/4009 / transport drop w/ seq+session | Resume op6 `{token,session_id,seq}`; exp backoff 1s->120s jitter |
| 4006 | fresh Identify immediately (no session) |
| InvalidSession `d:true` | Resume after 1-5s; `d:false` fresh Identify after 1-5s |
| 4008 gateway-rl | backoff 5s*attempt + SendQueue throttle |
| 4010/4011/4012/4013/4014 | fail fast `ShardDeath` + help, no retry |
| miss 2 HeartbeatAcks | close + resume path |
Session per shard `{session_id, resume_url, seq, identify}` in `gateway::SessionStore` (in-mem hot + batched persist; `seq` persisted BEFORE fan-out so resume never replays). `Cluster` supervises via `select!` + watchdog; `health()/restart(shard_id)`; graceful `ShutdownHandle` (CancellationToken + signal + 5s drain: close 1000, cancel chunks, drain dispatcher). `login()` fails after 30s READY timeout. Standby bounds: 1024 waiters/shard, `lazy Partial::fetch` semaphore 10 + 5s timeout. Soak SLO (`--short`): >=99% resume-ok, 0 panics, p99 resume <15s.

## 3. Ratelimit safety
Pre-emptive buckets + global 50rps token bucket (`Semaphore` refill); 429 -> sleep `retry_after` + penalty; shared-scope excluded from ban-meter; ban-meter 401/403/429 per 10min warn 50%/error 80%. Request-ids (`uuid` short) in tracing spans.

## 4. Validation & partials
Builders `validate()` before IO: content <=2000, embeds <=10/6000 total, components <=5x5, names lowercase <=32, audit-reason 1-512 URL-encoded, bulk-delete 2-100 + <14d, autocomplete <=25. `model::Partial` explicit; `ChannelType::Unknown(u8)` round-trips.

## 5. Observability
`tracing` spans per shard/event/route-template (`shard_id, event_name, route_template` — never bucket hash). `debug!` payloads redacted; `warn!` unknowns. Optional `metrics`: `gateway_events_total, gateway_resumes_total, gateway_unknown_events_total, rest_requests_total{route_template,status}, rest_429_total{scope}, cache_hit_ratio, ws_latency_ms, standby_lag_total`. `Client::health()` -> per-shard `{status, latency_ms, seq, guilds_cached}`.

