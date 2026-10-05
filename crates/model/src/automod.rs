//! Auto moderation.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Auto-mod rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoModRule {
    /// Rule id.
    pub id: Id<crate::AutoModMarker>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
}
/// Auto-mod action execution payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoModAction {
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Rule.
    #[serde(default)]
    pub rule_id: Option<Id<crate::AutoModMarker>>,
}
