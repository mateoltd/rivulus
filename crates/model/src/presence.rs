//! Presence + voice state + typing.
//!
//! Wire note: Discord sends the presence user as a `user` OBJECT and never
//! sends `user_id`. On parse, `user_id` is taken from an explicit `user_id`
//! key when present (model-shape payloads) and otherwise derived from
//! `user.id`; parsing fails only when neither is present.
use crate::Id;
use serde::{de, Deserialize, Serialize};

/// Presence status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Status {
    /// Online.
    Online,
    /// Idle.
    Idle,
    /// Dnd.
    Dnd,
    /// Invisible.
    Invisible,
    /// Offline.
    Offline,
}

/// Activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    /// Name.
    pub name: Box<str>,
    /// Kind.
    #[serde(rename = "type")]
    #[serde(default)]
    pub kind: u8,
}

/// Presence update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Presence {
    /// User id (derived from `user.id` on Discord-shaped payloads).
    pub user_id: Id<crate::UserMarker>,
    /// Inline user payload when present (used at insert to intern, not stored duplicated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<crate::User>,
    /// Guild id.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Status.
    #[serde(default)]
    pub status: Option<Status>,
    /// Activities.
    #[serde(default)]
    pub activities: Vec<Activity>,
}

/// Serde helper: `user_id` is optional on the wire (Discord omits it).
#[derive(Deserialize)]
struct PresenceDe {
    #[serde(default)]
    user_id: Option<Id<crate::UserMarker>>,
    #[serde(default)]
    user: Option<crate::User>,
    #[serde(default)]
    guild_id: Option<Id<crate::GuildMarker>>,
    #[serde(default)]
    status: Option<Status>,
    #[serde(default)]
    activities: Vec<Activity>,
}

impl<'de> Deserialize<'de> for Presence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let h = PresenceDe::deserialize(d)?;
        let user_id = h
            .user_id
            .or_else(|| h.user.as_ref().map(|u| u.id))
            .ok_or_else(|| de::Error::missing_field("user"))?;
        Ok(Self {
            user_id,
            user: h.user,
            guild_id: h.guild_id,
            status: h.status,
            activities: h.activities,
        })
    }
}

/// Voice state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceState {
    /// Guild id.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Channel id.
    #[serde(default)]
    pub channel_id: Option<Id<crate::ChannelMarker>>,
    /// User id.
    pub user_id: Id<crate::UserMarker>,
    /// Session id.
    #[serde(default)]
    pub session_id: Option<Box<str>>,
    /// Mute/deaf.
    #[serde(default)]
    pub mute: bool,
    /// Deafened.
    #[serde(default)]
    pub deaf: bool,
}

/// Voice server update (forwarded only v1; never connects).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceServerUpdate {
    /// Token.
    pub token: Box<str>,
    /// Guild id.
    pub guild_id: Id<crate::GuildMarker>,
    /// Endpoint.
    pub endpoint: Option<Box<str>>,
}
