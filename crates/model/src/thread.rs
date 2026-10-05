//! Threads.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Thread metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadMetadata {
    /// Archived.
    #[serde(default)]
    pub archived: bool,
    /// Auto archive minutes.
    #[serde(default)]
    pub auto_archive_duration: u64,
    /// Locked.
    #[serde(default)]
    pub locked: bool,
}
/// Thread channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    /// Thread id.
    pub id: Id<crate::ChannelMarker>,
    /// Parent.
    #[serde(default)]
    pub parent_id: Option<Id<crate::ChannelMarker>>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
}
/// Thread member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadMember {
    /// Thread id.
    #[serde(default)]
    pub id: Option<Id<crate::ChannelMarker>>,
    /// User id.
    #[serde(default)]
    pub user_id: Option<Id<crate::UserMarker>>,
}
