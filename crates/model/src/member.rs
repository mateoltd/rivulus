//! Guild members (user stored by id; canonical `User` lives in cache).
//!
//! Wire note: Discord sends the user as a `user` OBJECT and never sends
//! `user_id`. On parse, `user_id` is taken from an explicit `user_id` key
//! when present (model-shape payloads) and otherwise derived from
//! `user.id`; parsing fails only when neither is present.
use crate::Id;
use serde::{de, Deserialize, Serialize};
use time::OffsetDateTime;

/// Guild member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Member {
    /// Owning guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// User id (lookup canonical `User` in cache; never duplicate; derived
    /// from `user.id` on Discord-shaped payloads).
    pub user_id: Id<crate::UserMarker>,
    /// Inline user payload when present (used at insert to intern, not stored duplicated).
    #[serde(default)]
    pub user: Option<crate::User>,
    /// Nickname.
    #[serde(default)]
    pub nick: Option<Box<str>>,
    /// Role ids.
    #[serde(default)]
    pub roles: Vec<Id<crate::RoleMarker>>,
    /// Joined at.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub joined_at: Option<OffsetDateTime>,
    /// Timeout until.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub communication_disabled_until: Option<OffsetDateTime>,
    /// Deafened.
    #[serde(default)]
    pub deaf: bool,
    /// Muted.
    #[serde(default)]
    pub mute: bool,
    /// Pending verification.
    #[serde(default)]
    pub pending: bool,
}

/// Serde helper: `user_id` is optional on the wire (Discord omits it).
#[derive(Deserialize)]
struct MemberDe {
    #[serde(default)]
    guild_id: Option<Id<crate::GuildMarker>>,
    #[serde(default)]
    user_id: Option<Id<crate::UserMarker>>,
    #[serde(default)]
    user: Option<crate::User>,
    #[serde(default)]
    nick: Option<Box<str>>,
    #[serde(default)]
    roles: Vec<Id<crate::RoleMarker>>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    joined_at: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    communication_disabled_until: Option<OffsetDateTime>,
    #[serde(default)]
    deaf: bool,
    #[serde(default)]
    mute: bool,
    #[serde(default)]
    pending: bool,
}

impl<'de> Deserialize<'de> for Member {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let h = MemberDe::deserialize(d)?;
        let user_id = h
            .user_id
            .or_else(|| h.user.as_ref().map(|u| u.id))
            .ok_or_else(|| de::Error::missing_field("user"))?;
        Ok(Self {
            guild_id: h.guild_id,
            user_id,
            user: h.user,
            nick: h.nick,
            roles: h.roles,
            joined_at: h.joined_at,
            communication_disabled_until: h.communication_disabled_until,
            deaf: h.deaf,
            mute: h.mute,
            pending: h.pending,
        })
    }
}
