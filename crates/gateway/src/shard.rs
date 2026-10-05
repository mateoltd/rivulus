//! Shard state machine + WS runtime (Hello/Identify/Ready/Heartbeat/Resume).
//!
//! Single-shard WS runtime (`Shard::run`) plus pure logic.
//! `compress` is a connect-URL query param only, never an `Identify` field.
//! First heartbeat is jittered (`interval * fract`). All resumes use
//! `resume_gateway_url` (see `session::Session::gateway_url`).

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use futures::SinkExt as _;
use futures::StreamExt as _;

/// Shard id and total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardId {
    /// Shard index.
    pub id: u32,
    /// Total shards.
    pub total: u32,
}

/// Messenger handle for managers (`ctx.shard`).
#[derive(Debug, Clone)]
pub struct ShardMessenger {
    /// Shard identity.
    pub shard: ShardId,
    /// Last latency in milliseconds.
    pub latency: Arc<AtomicU64>,
}

impl ShardMessenger {
    /// Latency in milliseconds.
    #[must_use]
    pub fn latency_ms(&self) -> u64 {
        self.latency.load(Ordering::Relaxed)
    }
}

/// Identify concurrency queue trait (Redis impl path for multi-host).
pub trait Queue: Send + Sync + 'static {
    /// Wait until this shard may identify.
    fn enqueue(&self, shard: u32) -> futures::future::BoxFuture<'_, ()>;
}

/// In-memory identify queue: 1 identify per 5s per key, ordered start.
#[derive(Debug)]
pub struct InMemoryQueue {
    max_concurrency: u32,
    next_allowed: std::sync::Mutex<HashMap<u32, Instant>>,
}

impl InMemoryQueue {
    /// New with max concurrency.
    #[must_use]
    pub fn new(max_concurrency: u32) -> Self {
        Self {
            max_concurrency: max_concurrency.max(1),
            next_allowed: std::sync::Mutex::new(HashMap::new()),
        }
    }
    /// Rate-limit key for a shard.
    #[must_use]
    pub fn key(&self, shard: u32) -> u32 {
        shard % self.max_concurrency
    }
}

impl Queue for InMemoryQueue {
    fn enqueue(&self, shard: u32) -> futures::future::BoxFuture<'_, ()> {
        Box::pin(async move {
            let k = self.key(shard);
            let wait = {
                let mut map = match self.next_allowed.lock() {
                    Ok(g) => g,
                    Err(poison) => poison.into_inner(),
                };
                let now = Instant::now();
                let next = map.get(&k).copied().unwrap_or(now);
                let wait = next.saturating_duration_since(now);
                let base = if next > now { next } else { now };
                map.insert(k, base + Duration::from_secs(5));
                wait
            };
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        })
    }
}

/// Bootstrap trait (caller injects GetGatewayBot; gateway never depends on rest).
pub trait Bootstrap: Send + Sync + 'static {
    /// Fetch gateway bot info.
    fn get_gateway_bot(
        &self,
    ) -> futures::future::BoxFuture<'_, Result<model::GetGatewayBotResponse, common::Error>>;
}

/// Connect URL (exact; compress is a query param, never an Identify field).
///
/// # Example
///
/// ```
/// let u = gateway::shard::connect_url("wss://gateway.discord.gg");
/// assert!(u.contains("compress=zlib-stream"));
/// ```
#[must_use]
pub fn connect_url(base: &str) -> String {
    let b = base.trim_end_matches('/');
    std::format!("{b}/?v=10&encoding=json&compress=zlib-stream")
}

/// Shard strategy.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum ShardStrategy {
    /// Auto from GET gateway bot.
    Auto,
    /// Single shard.
    Single,
    /// Manual ids and total.
    Manual,
}

/// Synthetic lifecycle event (never a Discord dispatch; never in model::Event).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ShardEvent {
    /// Shard ready.
    Ready(ShardId),
    /// Session resumed.
    Resumed(ShardId),
    /// Disconnected (with close code when known).
    Disconnected {
        /// Shard identity.
        shard: ShardId,
        /// Close code when known.
        code: Option<u16>,
    },
}

/// Shard id from guild id: `(guild_id >> 22) % total`.
#[must_use]
pub fn shard_for_guild(guild_id: u64, total: u32) -> u32 {
    u32::try_from((guild_id >> 22) % u64::from(total.max(1))).unwrap_or(0)
}

/// Per-shard configuration.
pub struct ShardConfig {
    /// Bot token (never `Debug`).
    pub token: secrecy::SecretString,
    /// Intents.
    pub intents: model::Intents,
    /// Shard identity.
    pub shard: ShardId,
    /// `large_threshold` 50-250.
    pub large_threshold: u8,
}

impl ShardConfig {
    /// New; `large_threshold` clamped to `50..=250`.
    #[must_use]
    pub fn new(
        token: secrecy::SecretString,
        intents: model::Intents,
        shard: ShardId,
        large_threshold: u8,
    ) -> Self {
        Self {
            token,
            intents,
            shard,
            large_threshold: large_threshold.clamp(50, 250),
        }
    }
}

impl std::fmt::Debug for ShardConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardConfig")
            .field("token", &"***")
            .field("intents", &self.intents)
            .field("shard", &self.shard)
            .field("large_threshold", &self.large_threshold)
            .finish()
    }
}

/// Identify properties.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct IdentifyProperties {
    os: Box<str>,
    browser: Box<str>,
    device: Box<str>,
}

/// Identify payload (no `compress` field; compress is URL-only).
pub struct IdentifyPayload {
    token: secrecy::SecretString,
    intents: model::Intents,
    properties: IdentifyProperties,
    shard: [u32; 2],
    large_threshold: u8,
}

impl IdentifyPayload {
    /// Build from config.
    #[must_use]
    pub fn build(config: &ShardConfig) -> Self {
        Self {
            token: config.token.clone(),
            intents: config.intents,
            properties: IdentifyProperties {
                os: Box::from(std::env::consts::OS),
                browser: Box::from("rivulus"),
                device: Box::from("rivulus"),
            },
            shard: [config.shard.id, config.shard.total],
            large_threshold: config.large_threshold,
        }
    }
    /// Shard pair.
    #[must_use]
    pub fn shard(&self) -> [u32; 2] {
        self.shard
    }
    /// Intents.
    #[must_use]
    pub fn intents(&self) -> model::Intents {
        self.intents
    }
}

impl std::fmt::Debug for IdentifyPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyPayload")
            .field("token", &"***")
            .field("intents", &self.intents)
            .field("properties", &self.properties)
            .field("shard", &self.shard)
            .field("large_threshold", &self.large_threshold)
            .finish_non_exhaustive()
    }
}

impl serde::Serialize for IdentifyPayload {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use secrecy::ExposeSecret;
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("IdentifyPayload", 5)?;
        st.serialize_field("token", self.token.expose_secret())?;
        st.serialize_field("intents", &self.intents)?;
        st.serialize_field("properties", &self.properties)?;
        st.serialize_field("shard", &self.shard)?;
        st.serialize_field("large_threshold", &self.large_threshold)?;
        st.end()
    }
}

/// Heartbeat payload `op1 { d: seq|null }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HeartbeatPayload {
    /// Last seq, if any.
    pub d: Option<u64>,
}

impl HeartbeatPayload {
    /// New.
    #[must_use]
    pub fn new(seq: Option<u64>) -> Self {
        Self { d: seq }
    }
}

/// Resume payload `op6 { token, session_id, seq }`.
pub struct ResumePayload {
    token: secrecy::SecretString,
    session_id: Box<str>,
    seq: u64,
}

impl ResumePayload {
    /// New.
    #[must_use]
    pub fn new(token: secrecy::SecretString, session_id: Box<str>, seq: u64) -> Self {
        Self {
            token,
            session_id,
            seq,
        }
    }
    /// Session id.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// Seq.
    #[must_use]
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

impl std::fmt::Debug for ResumePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResumePayload")
            .field("token", &"***")
            .field("session_id", &self.session_id)
            .field("seq", &self.seq)
            .finish()
    }
}

impl serde::Serialize for ResumePayload {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use secrecy::ExposeSecret;
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("ResumePayload", 3)?;
        st.serialize_field("token", self.token.expose_secret())?;
        st.serialize_field("session_id", &self.session_id)?;
        st.serialize_field("seq", &self.seq)?;
        st.end()
    }
}

/// Jittered first-heartbeat delay: `interval * fract` (`fract` in `0..1`).
///
/// Caller supplies randomness (no `rand` dep); `fract` outside `0..=1`
/// is clamped.
#[must_use]
pub fn jittered_heartbeat_delay(interval: Duration, fract: f64) -> Duration {
    let clamped = fract.clamp(0.0, 1.0);
    let millis = interval.as_millis() as f64 * clamped;
    Duration::from_millis(millis as u64)
}

/// Chunk reassembly key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ChunkKey {
    nonce: Box<str>,
    guild_id: u64,
}

#[derive(Debug)]
struct ChunkState {
    expected: u32,
    got: BTreeMap<u32, Vec<model::Member>>,
    not_found: Vec<model::UserId>,
    started: Instant,
}

/// Assembled members chunk.
#[derive(Debug, Clone)]
pub struct AssembledChunk {
    /// Guild id.
    pub guild_id: model::GuildId,
    /// Nonce echo.
    pub nonce: Option<Box<str>>,
    /// Members in `chunk_index` order.
    pub members: Vec<model::Member>,
    /// Ids not found.
    pub not_found: Vec<model::UserId>,
}

/// Expired chunk (10s timeout; surfaces `not_found`).
#[derive(Debug, Clone)]
pub struct ExpiredChunk {
    /// Guild id.
    pub guild_id: model::GuildId,
    /// Nonce echo.
    pub nonce: Option<Box<str>>,
    /// Ids not found so far.
    pub not_found: Vec<model::UserId>,
}

/// Pure `op8` chunk reassembler (no IO).
///
/// Keyed by `(nonce, guild_id)`; buffers out-of-order `chunk_index`.
/// Entries expire after 10s via `expire()`.
#[derive(Debug, Default)]
pub struct ChunkAssembler {
    pending: HashMap<ChunkKey, ChunkState>,
}

/// Chunk timeout.
pub const CHUNK_TIMEOUT: Duration = Duration::from_secs(10);

impl ChunkAssembler {
    /// New empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Pending entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    /// Whether empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    /// Insert a chunk; returns the assembled result once all
    /// `chunk_count` pieces arrived.
    pub fn insert(
        &mut self,
        chunk: model::GuildMembersChunk,
        now: Instant,
    ) -> Option<AssembledChunk> {
        let nonce_box: Box<str> = chunk.nonce.clone().unwrap_or_default();
        let key = ChunkKey {
            nonce: nonce_box.clone(),
            guild_id: chunk.guild_id.get(),
        };
        let state = self.pending.entry(key).or_insert_with(|| ChunkState {
            expected: chunk.chunk_count.max(1),
            got: BTreeMap::new(),
            not_found: Vec::new(),
            started: now,
        });
        state.got.insert(chunk.chunk_index, chunk.members.clone());
        for id in &chunk.not_found {
            if !state.not_found.contains(id) {
                state.not_found.push(*id);
            }
        }
        if state.got.len() as u32 == state.expected {
            let key = ChunkKey {
                nonce: nonce_box,
                guild_id: chunk.guild_id.get(),
            };
            let state = self.pending.remove(&key)?;
            let mut members = Vec::new();
            for (_, v) in state.got {
                members.extend(v);
            }
            let nonce = chunk.nonce.clone();
            return Some(AssembledChunk {
                guild_id: chunk.guild_id,
                nonce,
                members,
                not_found: state.not_found,
            });
        }
        None
    }
    /// Evict entries older than 10s; surfaces their `not_found` ids.
    pub fn expire(&mut self, now: Instant) -> Vec<ExpiredChunk> {
        let mut out = Vec::new();
        let mut dead = Vec::new();
        for (k, st) in &self.pending {
            if now.duration_since(st.started) > CHUNK_TIMEOUT {
                dead.push(k.clone());
            }
        }
        for k in dead {
            if let Some(st) = self.pending.remove(&k) {
                if let Some(guild_id) = model::GuildId::new(k.guild_id) {
                    let nonce = if k.nonce.is_empty() {
                        None
                    } else {
                        Some(k.nonce.clone())
                    };
                    out.push(ExpiredChunk {
                        guild_id,
                        nonce,
                        not_found: st.not_found,
                    });
                }
            }
        }
        out
    }
}

/// Production gateway base (no query; [`connect_url`] adds params).
pub const PRODUCTION_BASE: &str = "wss://gateway.discord.gg";
/// Hello wait.
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
/// READY wait after Identify/Resume.
pub const READY_TIMEOUT: Duration = Duration::from_secs(30);
/// Hint for callers: bound supervisor reconnect loops.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 25;

/// Exit from one connection attempt (supervisor maps to backoff/retry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShardExit {
    /// Graceful shutdown (close 1000); do not reconnect.
    Shutdown,
    /// Reconnect then Resume `op6` on `resume_gateway_url`.
    Resume,
    /// Reconnect then fresh Identify (never Resume).
    FreshIdentify,
    /// Reconnect with extra backoff then Resume.
    BackoffResume,
    /// Fail fast, no retry.
    FailFast {
        /// Human remediation hint.
        help: &'static str,
    },
}

impl ShardExit {
    /// Whether this exit attempts a resume.
    #[must_use]
    pub fn can_resume(self) -> bool {
        matches!(self, Self::Resume | Self::BackoffResume)
    }
    /// Map a [`crate::CloseAction`] to an exit.
    #[must_use]
    pub fn from_action(action: crate::CloseAction) -> Self {
        match action {
            crate::CloseAction::Resume => Self::Resume,
            crate::CloseAction::FreshIdentify => Self::FreshIdentify,
            crate::CloseAction::BackoffResume => Self::BackoffResume,
            crate::CloseAction::FailFast { help } => Self::FailFast { help },
        }
    }
}

/// Resolve a base or resume URL to a full connect URL.
///
/// Appends `?v=10&encoding=json&compress=zlib-stream` when the `v=10`
/// marker is absent; passes through URLs that already carry it.
#[must_use]
pub fn resolve_url(raw: &str) -> String {
    if raw.contains("v=10") {
        raw.to_owned()
    } else {
        connect_url(raw)
    }
}

/// Exponential backoff `1s -> 120s` with jitter.
///
/// `attempt` 0 gives roughly 1s; large attempts cap at 120s.
/// `fract` in `0..1` (clamped) scales the delay `0.5x..1.5x`.
#[must_use]
pub fn backoff_delay(attempt: u32, fract: f64) -> Duration {
    let clamped = fract.clamp(0.0, 1.0);
    let exp = 1u64.checked_shl(attempt.min(10)).unwrap_or(1024).min(120);
    let jittered = (exp as f64 * (0.5 + clamped)).clamp(0.5, 120.0);
    Duration::from_millis((jittered * 1000.0) as u64)
}

/// Map a close code (or transport drop) to an exit.
///
/// `None` is a transport drop with no code. Resume-flavoured exits
/// degrade to [`ShardExit::FreshIdentify`] when `can_resume_store` is
/// false (no `session_id + seq`).
#[must_use]
pub fn map_close_to_exit(code: Option<u16>, can_resume_store: bool) -> ShardExit {
    match code {
        None => {
            if can_resume_store {
                ShardExit::BackoffResume
            } else {
                ShardExit::FreshIdentify
            }
        }
        Some(c) => {
            let base = ShardExit::from_action(crate::close::classify(c));
            match base {
                ShardExit::Resume | ShardExit::BackoffResume if !can_resume_store => {
                    ShardExit::FreshIdentify
                }
                other => other,
            }
        }
    }
}

/// Map an InvalidSession `d` flag to an exit.
#[must_use]
pub fn invalid_session_exit(resumable: bool) -> ShardExit {
    if resumable {
        ShardExit::Resume
    } else {
        ShardExit::FreshIdentify
    }
}

/// Pseudo-random `0..1` fraction for jitter (no `rand` dep).
///
/// Derived from wall-clock nanos; falls back to `0.5` when the clock
/// is unavailable.
#[must_use]
pub fn random_fract() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| f64::from(d.subsec_nanos() % 1000) / 1000.0)
        .unwrap_or(0.5)
}

/// Parse a Hello `d` payload to a heartbeat interval.
#[must_use]
pub fn parse_hello_interval(raw: &serde_json::value::RawValue) -> Option<Duration> {
    #[derive(serde::Deserialize)]
    struct HelloData {
        heartbeat_interval: u64,
    }
    serde_json::from_str::<HelloData>(raw.get())
        .ok()
        .map(|h| Duration::from_millis(h.heartbeat_interval.max(1)))
}

/// Parse one gateway envelope to an interval when it is a Hello.
fn hello_interval_from_bytes(bytes: &[u8]) -> Option<Duration> {
    let header: common::json::Header<'_> = common::json::from_slice(bytes).ok()?;
    if crate::Opcode::from_u8(header.op) != crate::Opcode::Hello {
        return None;
    }
    parse_hello_interval(header.d)
}

/// Single-shard WS runtime.
pub struct Shard;

impl Shard {
    /// Run one connection attempt from connect to disconnect.
    ///
    /// Flow: resolve URL (`base_url` override or production; stored
    /// `resume_gateway_url` wins when present), connect, read Hello
    /// (10s timeout), sleep `reset_after` when the start limit is
    /// exhausted, Identify (paced via `identify_queue`) or Resume,
    /// then heartbeat (jittered first) + dispatch loop until close,
    /// transport drop, InvalidSession, Reconnect `op7`, missed ACKs,
    /// or `shutdown`.
    ///
    /// Dispatch handling: `seq` bumps ONLY on `op0`; `READY` stores the
    /// session and emits `model::Event::Ready`; `RESUMED` emits
    /// `model::Event::Resumed`; voice updates forward as typed events;
    /// other kinds emit `model::Event::Unknown` (capped 8KiB) plus
    /// `metrics::report_unknown`. Heartbeat ACKs update
    /// `messenger.latency`. Miss 2 ACKs closes and resumes. Shutdown
    /// sends close 1000 and returns [`ShardExit::Shutdown`].
    #[allow(clippy::too_many_arguments)]
    pub async fn run<S>(
        cfg: ShardConfig,
        messenger: ShardMessenger,
        store: Arc<S>,
        identify_queue: Arc<InMemoryQueue>,
        send_queue: Arc<crate::SendQueue>,
        session_limit: Option<model::SessionStartLimit>,
        base_url: Option<String>,
        out: tokio::sync::mpsc::Sender<model::Event>,
        shutdown: tokio_util::sync::CancellationToken,
    ) -> ShardExit
    where
        S: crate::SessionStore,
    {
        if shutdown.is_cancelled() {
            return ShardExit::Shutdown;
        }
        if let Some(lim) = session_limit.as_ref() {
            if lim.remaining == 0 && lim.reset_after > 0 {
                tracing::warn!(
                    reset_after_ms = lim.reset_after,
                    "session start limit exhausted; sleeping"
                );
                tokio::select! {
                    _ = shutdown.cancelled() => return ShardExit::Shutdown,
                    _ = tokio::time::sleep(Duration::from_millis(lim.reset_after)) => {}
                }
            }
        }
        let fallback_base = base_url.unwrap_or_else(|| PRODUCTION_BASE.to_owned());
        let snap0 = store.snapshot();
        let url = resolve_url(snap0.gateway_url(&fallback_base));
        let (ws, _) = match tokio_tungstenite::connect_async(url.clone()).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("gateway connect failed: {e}");
                return map_close_to_exit(None, store.can_resume());
            }
        };
        let (mut sink, mut stream) = ws.split();
        let mut zlib = crate::ZlibStream::new();
        let metrics = crate::Metrics::new();

        let interval: Duration = {
            let hello = tokio::time::timeout(HELLO_TIMEOUT, async {
                loop {
                    let item = stream.next().await;
                    match item {
                        None => return Err(None),
                        Some(Err(_)) => return Err(None),
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(frame))) => {
                            let code = frame.as_ref().map(|f| u16::from(f.code));
                            return Err(code);
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(data))) => {
                            let _ = sink
                                .send(tokio_tungstenite::tungstenite::Message::Pong(data))
                                .await;
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {}
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(s))) => {
                            if let Some(iv) = hello_interval_from_bytes(s.as_bytes()) {
                                return Ok(iv);
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(b))) => {
                            match zlib.feed(&b) {
                                Ok(msgs) => {
                                    for m in msgs {
                                        if let Some(iv) = hello_interval_from_bytes(&m) {
                                            return Ok(iv);
                                        }
                                    }
                                }
                                Err(_) => return Err(None),
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Frame(_))) => {}
                    }
                }
            })
            .await;
            match hello {
                Err(_) => return map_close_to_exit(None, store.can_resume()),
                Ok(Err(code)) => {
                    let exit = map_close_to_exit(code, store.can_resume());
                    if exit == ShardExit::FreshIdentify {
                        store.clear();
                    }
                    return exit;
                }
                Ok(Ok(iv)) => iv,
            }
        };

        let snap = store.snapshot();
        let mut seq: Option<u64> = snap.seq;
        if snap.can_resume() {
            let (sid, s) = match (snap.session_id.clone(), snap.seq) {
                (Some(id), Some(v)) => (id, v),
                _ => {
                    store.clear();
                    return ShardExit::FreshIdentify;
                }
            };
            tokio::select! {
                _ = shutdown.cancelled() => {
                    let _ = sink
                        .send(close_normal())
                        .await;
                    return ShardExit::Shutdown;
                }
                _ = send_queue.acquire(false) => {}
            }
            if shutdown.is_cancelled() {
                let _ = sink.send(close_normal()).await;
                return ShardExit::Shutdown;
            }
            let payload = ResumePayload::new(cfg.token.clone(), sid, s);
            let txt = match resume_envelope(&payload) {
                Some(t) => t,
                None => {
                    return ShardExit::FailFast {
                        help: "resume serialize failed",
                    };
                }
            };
            if sink
                .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                .await
                .is_err()
            {
                return map_close_to_exit(None, store.can_resume());
            }
        } else {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    let _ = sink.send(close_normal()).await;
                    return ShardExit::Shutdown;
                }
                _ = identify_queue.enqueue(cfg.shard.id) => {}
            }
            tokio::select! {
                _ = shutdown.cancelled() => {
                    let _ = sink.send(close_normal()).await;
                    return ShardExit::Shutdown;
                }
                _ = send_queue.acquire(false) => {}
            }
            if shutdown.is_cancelled() {
                let _ = sink.send(close_normal()).await;
                return ShardExit::Shutdown;
            }
            let payload = IdentifyPayload::build(&cfg);
            let txt = match identify_envelope(&payload) {
                Some(t) => t,
                None => {
                    return ShardExit::FailFast {
                        help: "identify serialize failed",
                    };
                }
            };
            if sink
                .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                .await
                .is_err()
            {
                return map_close_to_exit(None, store.can_resume());
            }
        }

        let mut deadline =
            tokio::time::Instant::now() + jittered_heartbeat_delay(interval, random_fract());
        let ready_deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        let mut ready_received = false;
        let mut unacked: u32 = 0;
        let mut last_sent: Option<Instant> = None;

        loop {
            if !ready_received && tokio::time::Instant::now() >= ready_deadline {
                tracing::warn!("READY timeout");
                let exit = map_close_to_exit(None, store.can_resume());
                if exit == ShardExit::FreshIdentify {
                    store.clear();
                }
                return exit;
            }
            tokio::select! {
                _ = shutdown.cancelled() => {
                    let _ = sink.send(close_normal()).await;
                    return ShardExit::Shutdown;
                }
                _ = tokio::time::sleep_until(deadline) => {
                    if unacked >= 2 {
                        tracing::warn!("missed 2 heartbeat ACKs; closing");
                        let _ = sink.send(close_abnormal()).await;
                        return map_close_to_exit(None, store.can_resume());
                    }
                    let txt = heartbeat_envelope(seq);
                    if sink
                        .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                        .await
                        .is_err()
                    {
                        return map_close_to_exit(None, store.can_resume());
                    }
                    last_sent = Some(Instant::now());
                    unacked = unacked.saturating_add(1);
                    deadline = tokio::time::Instant::now() + interval;
                }
                msg = stream.next() => {
                    match msg {
                        None => {
                            return map_close_to_exit(None, store.can_resume());
                        }
                        Some(Err(_)) => {
                            return map_close_to_exit(None, store.can_resume());
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(frame))) => {
                            let code = frame.as_ref().map(|f| u16::from(f.code)).unwrap_or(1006);
                            let exit = map_close_to_exit(Some(code), store.can_resume());
                            if exit == ShardExit::FreshIdentify {
                                store.clear();
                            }
                            return exit;
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(data))) => {
                            let _ = sink.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await;
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {}
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Frame(_))) => {}
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(s))) => {
                            if s.len() > crate::MESSAGE_CAP {
                                return map_close_to_exit(None, store.can_resume());
                            }
                            match handle_bytes(
                                s.as_bytes(),
                                &store,
                                &metrics,
                                &messenger,
                                &mut seq,
                                &mut ready_received,
                                &mut unacked,
                                &mut last_sent,
                                &mut sink,
                                &out,
                            )
                            .await
                            {
                                None => {}
                                Some(exit) => return exit,
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(b))) => {
                            match zlib.feed(&b) {
                                Ok(msgs) => {
                                    let mut pending_exit: Option<ShardExit> = None;
                                    for m in msgs {
                                        match handle_bytes(
                                            &m,
                                            &store,
                                            &metrics,
                                            &messenger,
                                            &mut seq,
                                            &mut ready_received,
                                            &mut unacked,
                                            &mut last_sent,
                                            &mut sink,
                                            &out,
                                        )
                                        .await
                                        {
                                            None => {}
                                            Some(exit) => {
                                                pending_exit = Some(exit);
                                                break;
                                            }
                                        }
                                    }
                                    if let Some(exit) = pending_exit {
                                        return exit;
                                    }
                                }
                                Err(_) => {
                                    return map_close_to_exit(None, store.can_resume());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Close frame for graceful shutdown (1000).
fn close_normal() -> tokio_tungstenite::tungstenite::Message {
    tokio_tungstenite::tungstenite::Message::Close(Some(
        tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: std::borrow::Cow::Borrowed("shutdown"),
        },
    ))
}

/// Close frame for missed-ACK watchdog (abnormal, triggers resume path).
fn close_abnormal() -> tokio_tungstenite::tungstenite::Message {
    tokio_tungstenite::tungstenite::Message::Close(Some(
        tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Abnormal,
            reason: std::borrow::Cow::Borrowed("missed acks"),
        },
    ))
}

/// Serialize an Identify envelope `{"op":2,"d":{...}}`.
fn identify_envelope(payload: &IdentifyPayload) -> Option<String> {
    let d = serde_json::to_value(payload).ok()?;
    serde_json::to_string(&serde_json::json!({"op": 2, "d": d})).ok()
}

/// Serialize a Resume envelope `{"op":6,"d":{...}}`.
fn resume_envelope(payload: &ResumePayload) -> Option<String> {
    let d = serde_json::to_value(payload).ok()?;
    serde_json::to_string(&serde_json::json!({"op": 6, "d": d})).ok()
}

/// Serialize a heartbeat envelope `{"op":1,"d":seq|null}`.
fn heartbeat_envelope(seq: Option<u64>) -> String {
    match seq {
        Some(v) => std::format!("{{\"op\":1,\"d\":{v}}}"),
        None => std::string::String::from("{\"op\":1,\"d\":null}"),
    }
}

/// Handle one decompressed gateway envelope.
///
/// Returns `Some(exit)` when the connection should terminate
/// (Reconnect `op7`, InvalidSession, FailFast close is handled by the
/// caller via close frames). Updates `seq` ONLY on `op0`, persists it
/// BEFORE fan-out, and updates latency on `op11`.
#[allow(clippy::too_many_arguments)]
async fn handle_bytes<S>(
    bytes: &[u8],
    store: &Arc<S>,
    metrics: &crate::Metrics,
    messenger: &ShardMessenger,
    seq: &mut Option<u64>,
    ready_received: &mut bool,
    unacked: &mut u32,
    last_sent: &mut Option<Instant>,
    sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Message,
    >,
    out: &tokio::sync::mpsc::Sender<model::Event>,
) -> Option<ShardExit>
where
    S: crate::SessionStore,
{
    let header: common::json::Header<'_> = match common::json::from_slice(bytes) {
        Ok(h) => h,
        Err(_) => return None,
    };
    match crate::Opcode::from_u8(header.op) {
        crate::Opcode::Dispatch => {
            if let Some(s) = header.s {
                store.set_seq(s);
                *seq = Some(s);
            }
            metrics.inc_event();
            let kind = header.t.unwrap_or("");
            if kind == "READY" {
                match serde_json::from_str::<model::Ready>(header.d.get()) {
                    Ok(ready) => {
                        store.set_ready(ready.session_id.clone(), ready.resume_gateway_url.clone());
                        *ready_received = true;
                        let _ = out.send(model::Event::Ready(Arc::new(ready))).await;
                    }
                    Err(_) => {
                        let raw = header.d.get();
                        let ev = model::Event::unknown("READY", header.s, raw);
                        metrics.report_unknown("READY");
                        let _ = out.send(ev).await;
                    }
                }
            } else if kind == "RESUMED" {
                *ready_received = true;
                metrics.inc_resume();
                let _ = out.send(model::Event::Resumed).await;
            } else if kind == "VOICE_STATE_UPDATE" {
                match serde_json::from_str::<model::VoiceState>(header.d.get()) {
                    Ok(v) => {
                        let _ = out.send(model::Event::VoiceStateUpdate(Arc::new(v))).await;
                    }
                    Err(_) => {
                        let ev = model::Event::unknown(kind, header.s, header.d.get());
                        metrics.report_unknown(kind);
                        let _ = out.send(ev).await;
                    }
                }
            } else if kind == "VOICE_SERVER_UPDATE" {
                match serde_json::from_str::<model::VoiceServerUpdate>(header.d.get()) {
                    Ok(v) => {
                        let _ = out.send(model::Event::VoiceServerUpdate(Arc::new(v))).await;
                    }
                    Err(_) => {
                        let ev = model::Event::unknown(kind, header.s, header.d.get());
                        metrics.report_unknown(kind);
                        let _ = out.send(ev).await;
                    }
                }
            } else {
                let ev = model::Event::unknown(kind, header.s, header.d.get());
                metrics.report_unknown(kind);
                let _ = out.send(ev).await;
            }
            None
        }
        crate::Opcode::Heartbeat => {
            let txt = heartbeat_envelope(*seq);
            let _ = sink
                .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                .await;
            None
        }
        crate::Opcode::Reconnect => {
            if store.can_resume() {
                Some(ShardExit::Resume)
            } else {
                Some(ShardExit::FreshIdentify)
            }
        }
        crate::Opcode::InvalidSession => {
            let resumable = serde_json::from_str::<bool>(header.d.get()).unwrap_or(false);
            let exit = invalid_session_exit(resumable);
            if exit == ShardExit::FreshIdentify {
                store.clear();
            }
            Some(exit)
        }
        crate::Opcode::Hello => {
            if let Some(_iv) = parse_hello_interval(header.d) {
                // Interval updates mid-connection are ignored v1;
                // the initial Hello sets the cadence.
            }
            None
        }
        crate::Opcode::HeartbeatAck => {
            *unacked = 0;
            if let Some(sent) = *last_sent {
                let ms = sent.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
                messenger.latency.store(ms, Ordering::Relaxed);
            }
            None
        }
        crate::Opcode::Unknown(_) => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_key() {
        let q = InMemoryQueue::new(16);
        assert_eq!(q.key(20), 4);
    }

    #[test]
    fn identify_has_no_compress() {
        let cfg = ShardConfig::new(
            secrecy::SecretString::from(String::from("tok")),
            model::Intents::GUILDS,
            ShardId { id: 1, total: 4 },
            50,
        );
        let payload = IdentifyPayload::build(&cfg);
        let v = serde_json::to_value(&payload).expect("ser");
        assert!(v.get("compress").is_none());
        assert_eq!(v["shard"], serde_json::json!([1, 4]));
        assert_eq!(v["properties"]["browser"], "rivulus");
        assert_eq!(v["properties"]["device"], "rivulus");
        assert!(v.get("token").is_some());
        let url = connect_url("wss://gateway.discord.gg");
        assert!(url.contains("compress=zlib-stream"));
        assert_eq!(
            url,
            "wss://gateway.discord.gg/?v=10&encoding=json&compress=zlib-stream"
        );
    }

    #[test]
    fn token_never_debug() {
        let cfg = ShardConfig::new(
            secrecy::SecretString::from(String::from("super-secret")),
            model::Intents::empty(),
            ShardId { id: 0, total: 1 },
            50,
        );
        let dbg = std::format!("{cfg:?}");
        assert!(!dbg.contains("super-secret"));
        let payload = IdentifyPayload::build(&cfg);
        let dbg = std::format!("{payload:?}");
        assert!(!dbg.contains("super-secret"));
        let resume = ResumePayload::new(
            secrecy::SecretString::from(String::from("super-secret")),
            Box::from("sess"),
            7,
        );
        let dbg = std::format!("{resume:?}");
        assert!(!dbg.contains("super-secret"));
    }

    #[test]
    fn shard_config_debug_redacts_token() {
        let raw = "shard-config-secret-token";
        let cfg = ShardConfig::new(
            secrecy::SecretString::from(String::from(raw)),
            model::Intents::GUILDS,
            ShardId { id: 3, total: 8 },
            100,
        );
        let dbg = std::format!("{cfg:?}");
        assert!(
            !dbg.contains(raw),
            "ShardConfig Debug must redact the Bot token"
        );
        assert!(dbg.contains("***"));
    }

    #[test]
    fn heartbeat_shape() {
        let hb = HeartbeatPayload::new(Some(42));
        let v = serde_json::to_value(&hb).expect("ser");
        assert_eq!(v["d"], 42);
        let hb = HeartbeatPayload::new(None);
        let v = serde_json::to_value(&hb).expect("ser");
        assert!(v["d"].is_null());
    }

    #[test]
    fn resume_shape() {
        let r = ResumePayload::new(
            secrecy::SecretString::from(String::from("t")),
            Box::from("sess"),
            9,
        );
        let v = serde_json::to_value(&r).expect("ser");
        assert_eq!(v["session_id"], "sess");
        assert_eq!(v["seq"], 9);
    }

    #[test]
    fn jitter_bounds() {
        let interval = Duration::from_millis(1000);
        assert_eq!(jittered_heartbeat_delay(interval, 0.0), Duration::ZERO);
        assert_eq!(
            jittered_heartbeat_delay(interval, 1.0),
            Duration::from_millis(1000)
        );
        let mid = jittered_heartbeat_delay(interval, 0.5);
        assert_eq!(mid, Duration::from_millis(500));
    }

    fn member(guild: model::GuildId, user: u64) -> model::Member {
        model::Member {
            guild_id: Some(guild),
            user_id: model::UserId::new(user).expect("user"),
            user: None,
            nick: None,
            roles: Vec::new(),
            joined_at: None,
            communication_disabled_until: None,
            deaf: false,
            mute: false,
            pending: false,
        }
    }

    #[test]
    fn chunk_reassembly_out_of_order() {
        let guild = model::GuildId::new(123).expect("guild");
        let mut asm = ChunkAssembler::new();
        let now = Instant::now();
        let c1 = model::GuildMembersChunk {
            guild_id: guild,
            members: vec![member(guild, 2)],
            chunk_index: 1,
            chunk_count: 2,
            nonce: Some(Box::from("n1")),
            not_found: Vec::new(),
        };
        assert!(asm.insert(c1, now).is_none());
        let c0 = model::GuildMembersChunk {
            guild_id: guild,
            members: vec![member(guild, 1)],
            chunk_index: 0,
            chunk_count: 2,
            nonce: Some(Box::from("n1")),
            not_found: Vec::new(),
        };
        let done = asm.insert(c0, now).expect("assembled");
        assert_eq!(done.members.len(), 2);
        assert!(asm.is_empty());
    }

    #[test]
    fn chunk_timeout_surfaces_not_found() {
        let guild = model::GuildId::new(9).expect("guild");
        let mut asm = ChunkAssembler::new();
        let now = Instant::now();
        let missing = model::UserId::new(777).expect("user");
        let c0 = model::GuildMembersChunk {
            guild_id: guild,
            members: Vec::new(),
            chunk_index: 0,
            chunk_count: 2,
            nonce: Some(Box::from("n2")),
            not_found: vec![missing],
        };
        assert!(asm.insert(c0, now).is_none());
        let later = now + Duration::from_secs(11);
        let expired = asm.expire(later);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].not_found, vec![missing]);
        assert!(asm.is_empty());
    }

    #[test]
    fn backoff_caps_at_120s() {
        let first = backoff_delay(0, 0.5);
        assert!(first >= Duration::from_millis(500));
        assert!(first <= Duration::from_secs(2));
        let capped = backoff_delay(20, 1.0);
        assert_eq!(capped, Duration::from_secs(120));
        let min = backoff_delay(20, 0.0);
        assert!(min <= Duration::from_secs(120));
        assert!(min >= Duration::from_secs(1));
    }

    #[test]
    fn close_maps_with_degrade() {
        assert_eq!(
            map_close_to_exit(Some(4007), true),
            ShardExit::FreshIdentify
        );
        assert_eq!(map_close_to_exit(Some(4009), true), ShardExit::Resume);
        assert_eq!(
            map_close_to_exit(Some(4009), false),
            ShardExit::FreshIdentify
        );
        assert_eq!(map_close_to_exit(None, true), ShardExit::BackoffResume);
        assert_eq!(map_close_to_exit(None, false), ShardExit::FreshIdentify);
        assert!(matches!(
            map_close_to_exit(Some(4014), true),
            ShardExit::FailFast { .. }
        ));
    }

    #[test]
    fn invalid_session_flags() {
        assert_eq!(invalid_session_exit(true), ShardExit::Resume);
        assert_eq!(invalid_session_exit(false), ShardExit::FreshIdentify);
    }

    #[test]
    fn resolve_url_appends_once() {
        let full = resolve_url("wss://gateway.discord.gg");
        assert_eq!(
            full,
            "wss://gateway.discord.gg/?v=10&encoding=json&compress=zlib-stream"
        );
        let passthrough = resolve_url(&full);
        assert_eq!(passthrough, full);
    }

    #[test]
    fn hello_interval_parses() {
        let raw = r#"{"heartbeat_interval":100}"#;
        let boxed: Box<serde_json::value::RawValue> = serde_json::from_str(raw).expect("raw");
        assert_eq!(
            parse_hello_interval(&boxed),
            Some(Duration::from_millis(100))
        );
        let bytes = br#"{"op":10,"d":{"heartbeat_interval":50}}"#;
        assert_eq!(
            hello_interval_from_bytes(bytes),
            Some(Duration::from_millis(50))
        );
        let not_hello = br#"{"op":11,"d":null}"#;
        assert_eq!(hello_interval_from_bytes(not_hello), None);
    }

    #[test]
    fn heartbeat_envelope_shapes() {
        assert_eq!(heartbeat_envelope(Some(7)), "{\"op\":1,\"d\":7}");
        assert_eq!(heartbeat_envelope(None), "{\"op\":1,\"d\":null}");
    }
}
