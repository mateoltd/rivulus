//! Scheduled events.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Scheduled event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledEvent {
    /// Event id.
    pub id: Id<crate::ScheduledMarker>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
}
