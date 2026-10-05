//! Snapshot shapes crossing the boundary (plan section 3, pinned by M2).
//!
//! IDs cross as strings, always (u64 does not fit in a double). Timestamps
//! cross as RFC 3339 strings via the serde form core already uses.

use napi_derive::napi;

/// One message snapshot (see `node/contract.md`).
#[napi(object)]
pub struct MessageSnapshot {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub content: String,
    pub timestamp: String,
}

impl MessageSnapshot {
    /// Snapshot a core message. Timestamp renders as RFC 3339 (the same form
    /// core serde uses); an unrenderable timestamp yields empty rather than
    /// failing the whole page.
    pub fn of(message: &rivulus::model::Message) -> Self {
        let timestamp = message
            .timestamp
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        Self {
            id: message.id.to_string(),
            channel_id: message.channel_id.to_string(),
            author_id: message.author_id.to_string(),
            content: message.content.to_string(),
            timestamp,
        }
    }
}

/// Health numbers for `getHealth` (see `node/contract.md`).
#[napi(object)]
pub struct HealthSnapshot {
    pub ready: bool,
    pub shards: u32,
    pub guilds_cached: u32,
    pub events_dropped: u32,
}

/// One shard latency row for `getLatencies` (milliseconds as `f64`: JS
/// numbers cannot hold `u64` exactly).
#[napi(object)]
pub struct LatencyRow {
    pub shard: u32,
    pub total: u32,
    pub latency_ms: f64,
}

/// Cache numbers for `getCacheStats`.
#[napi(object)]
pub struct CacheSnapshot {
    pub hit_ratio: f64,
    pub guilds: u32,
    pub channels: u32,
    pub messages: u32,
}
