//! `GET /gateway/bot` pure types (no IO; consumed via `gateway::Bootstrap`).
use serde::{Deserialize, Serialize};
/// Session start limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStartLimit {
    /// Total.
    pub total: u64,
    /// Remaining.
    pub remaining: u64,
    /// Reset after ms.
    pub reset_after: u64,
    /// Max concurrency.
    pub max_concurrency: u64,
}
/// Get gateway bot response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetGatewayBotResponse {
    /// WS url.
    pub url: Box<str>,
    /// Recommended shards.
    pub shards: u32,
    /// Session limit.
    pub session_start_limit: SessionStartLimit,
}
