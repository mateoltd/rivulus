//! Invites.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Invite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    /// Code.
    pub code: Box<str>,
    /// Channel.
    #[serde(default)]
    pub channel_id: Option<Id<crate::ChannelMarker>>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
}
