#![allow(missing_docs)]
//! Guild permissions + channel overwrites.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

bitflags::bitflags! {
    /// Guild-level permissions (u64 bits, serde as string per API v8+).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Permissions: u64 {
        const CREATE_INSTANT_INVITE = 1 << 0;
        const KICK_MEMBERS = 1 << 1;
        const BAN_MEMBERS = 1 << 2;
        const ADMINISTRATOR = 1 << 3;
        const MANAGE_CHANNELS = 1 << 4;
        const MANAGE_GUILD = 1 << 5;
        const ADD_REACTIONS = 1 << 6;
        const VIEW_AUDIT_LOG = 1 << 7;
        const PRIORITY_SPEAKER = 1 << 8;
        const STREAM = 1 << 9;
        const VIEW_CHANNEL = 1 << 10;
        const SEND_MESSAGES = 1 << 11;
        const SEND_TTS_MESSAGES = 1 << 12;
        const MANAGE_MESSAGES = 1 << 13;
        const EMBED_LINKS = 1 << 14;
        const ATTACH_FILES = 1 << 15;
        const READ_MESSAGE_HISTORY = 1 << 16;
        const MENTION_EVERYONE = 1 << 17;
        const USE_EXTERNAL_EMOJIS = 1 << 18;
        const VIEW_GUILD_INSIGHTS = 1 << 19;
        const CONNECT = 1 << 20;
        const SPEAK = 1 << 21;
        const MUTE_MEMBERS = 1 << 22;
        const DEAFEN_MEMBERS = 1 << 23;
        const MOVE_MEMBERS = 1 << 24;
        const USE_VAD = 1 << 25;
        const CHANGE_NICKNAME = 1 << 26;
        const MANAGE_NICKNAMES = 1 << 27;
        const MANAGE_ROLES = 1 << 28;
        const MANAGE_WEBHOOKS = 1 << 29;
        const MANAGE_GUILD_EXPRESSIONS = 1 << 30;
        const USE_APPLICATION_COMMANDS = 1 << 31;
        const REQUEST_TO_SPEAK = 1 << 32;
        const MANAGE_EVENTS = 1 << 33;
        const MANAGE_THREADS = 1 << 34;
        const CREATE_PUBLIC_THREADS = 1 << 35;
        const CREATE_PRIVATE_THREADS = 1 << 36;
        const USE_EXTERNAL_STICKERS = 1 << 37;
        const SEND_MESSAGES_IN_THREADS = 1 << 38;
        const USE_EMBEDDED_ACTIVITIES = 1 << 39;
        const MODERATE_MEMBERS = 1 << 40;
        const VIEW_CREATOR_MONETIZATION_ANALYTICS = 1 << 41;
        const USE_SOUNDBOARD = 1 << 42;
        const CREATE_GUILD_EXPRESSIONS = 1 << 43;
        const CREATE_EVENTS = 1 << 44;
        const USE_EXTERNAL_SOUNDS = 1 << 45;
        const SEND_VOICE_MESSAGES = 1 << 46;
        const SEND_POLLS = 1 << 49;
        const USE_EXTERNAL_APPS = 1 << 50;
    }
}

struct PermVisitor;
impl Visitor<'_> for PermVisitor {
    type Value = Permissions;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("permissions as string or integer")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Permissions, E> {
        v.parse::<u64>()
            .map(Permissions::from_bits_truncate)
            .map_err(|_| E::custom("bad permissions"))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Permissions, E> {
        Ok(Permissions::from_bits_truncate(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Permissions, E> {
        Ok(Permissions::from_bits_truncate(v as u64))
    }
}

impl<'de> Deserialize<'de> for Permissions {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(PermVisitor)
    }
}

impl Serialize for Permissions {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut buf = itoa::Buffer::new();
        s.serialize_str(buf.format(self.bits()))
    }
}

/// Check flags: `(perms & FLAG) == FLAG`.
///
/// # Example
/// ```
/// let p = model::Permissions::SEND_MESSAGES | model::Permissions::ADD_REACTIONS;
/// assert!(p.contains(model::Permissions::SEND_MESSAGES));
/// ```
#[must_use]
pub fn has(perms: Permissions, flag: Permissions) -> bool {
    perms.contains(flag)
}

/// Channel permission overwrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overwrite {
    /// Overwritten role or member id.
    pub id: crate::Id<crate::RoleMarker>,
    /// Overwrite kind: 0 = role, 1 = member.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Allowed bits (string).
    pub allow: Permissions,
    /// Denied bits (string).
    pub deny: Permissions,
}

/// Compute effective channel permissions: base minus timeout, plus overwrites.
#[must_use]
pub fn effective(
    base: Permissions,
    overwrites: &[Overwrite],
    member_roles: &[crate::RoleId],
    is_admin: bool,
    timed_out: bool,
) -> Permissions {
    if is_admin || base.contains(Permissions::ADMINISTRATOR) {
        return Permissions::all();
    }
    let mut perms = base;
    if timed_out {
        perms = Permissions::VIEW_CHANNEL | Permissions::READ_MESSAGE_HISTORY;
    }
    for o in overwrites {
        let applies = o.kind == 0 && member_roles.iter().any(|r| r.get() == o.id.get());
        if applies || o.kind == 0 && o.id.get() == 0 {
            perms = (perms & !o.deny) | o.allow;
        }
    }
    perms
}
