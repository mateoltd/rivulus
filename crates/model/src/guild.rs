//! Guilds + related payloads.
use crate::Id;
use serde::{Deserialize, Serialize};

/// Guild.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guild {
    /// Guild id.
    pub id: Id<crate::GuildMarker>,
    /// Name.
    pub name: Box<str>,
    /// Owner id.
    pub owner_id: Id<crate::UserMarker>,
    /// Member count (when present).
    #[serde(default)]
    pub member_count: Option<u64>,
    /// Roles.
    #[serde(default)]
    pub roles: Vec<crate::Role>,
    /// Emojis.
    #[serde(default)]
    pub emojis: Vec<crate::Emoji>,
    /// Stickers.
    #[serde(default)]
    pub stickers: Vec<crate::Sticker>,
    /// Unavailable flag (GUILD_CREATE dance).
    #[serde(default)]
    pub unavailable: bool,
}

/// Unavailable guild reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnavailableGuild {
    /// Guild id.
    pub id: Id<crate::GuildMarker>,
    /// Always true when unavailable.
    #[serde(default = "default_true")]
    pub unavailable: bool,
}
fn default_true() -> bool {
    true
}

/// Guild members chunk (op8 reassembly output).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuildMembersChunk {
    /// Guild id.
    pub guild_id: Id<crate::GuildMarker>,
    /// Members in this chunk.
    #[serde(default)]
    pub members: Vec<crate::Member>,
    /// Chunk index.
    pub chunk_index: u32,
    /// Chunk count.
    pub chunk_count: u32,
    /// Nonce echo.
    #[serde(default)]
    pub nonce: Option<Box<str>>,
    /// Ids not found.
    #[serde(default)]
    pub not_found: Vec<Id<crate::UserMarker>>,
}
