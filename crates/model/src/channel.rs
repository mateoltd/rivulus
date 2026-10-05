//! Channels + channel types (manual Unknown round-trip).
use crate::Id;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Channel type with forward-compat unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChannelType {
    /// Guild text.
    GuildText,
    /// DM.
    Dm,
    /// Guild voice.
    GuildVoice,
    /// Group DM.
    GroupDm,
    /// Guild category.
    GuildCategory,
    /// Guild announcement.
    GuildAnnouncement,
    /// Announcement thread.
    AnnouncementThread,
    /// Public thread.
    PublicThread,
    /// Private thread.
    PrivateThread,
    /// Guild stage voice.
    GuildStageVoice,
    /// Guild directory.
    GuildDirectory,
    /// Guild forum.
    GuildForum,
    /// Guild media.
    GuildMedia,
    /// Unknown (round-trips).
    Unknown(u8),
}

impl ChannelType {
    fn n(self) -> u8 {
        match self {
            Self::GuildText => 0,
            Self::Dm => 1,
            Self::GuildVoice => 2,
            Self::GroupDm => 3,
            Self::GuildCategory => 4,
            Self::GuildAnnouncement => 5,
            Self::AnnouncementThread => 10,
            Self::PublicThread => 11,
            Self::PrivateThread => 12,
            Self::GuildStageVoice => 13,
            Self::GuildDirectory => 14,
            Self::GuildForum => 15,
            Self::GuildMedia => 16,
            Self::Unknown(n) => n,
        }
    }
    fn from_n(n: u8) -> Self {
        match n {
            0 => Self::GuildText,
            1 => Self::Dm,
            2 => Self::GuildVoice,
            3 => Self::GroupDm,
            4 => Self::GuildCategory,
            5 => Self::GuildAnnouncement,
            10 => Self::AnnouncementThread,
            11 => Self::PublicThread,
            12 => Self::PrivateThread,
            13 => Self::GuildStageVoice,
            14 => Self::GuildDirectory,
            15 => Self::GuildForum,
            16 => Self::GuildMedia,
            n => Self::Unknown(n),
        }
    }
}
struct CtVisitor;
impl Visitor<'_> for CtVisitor {
    type Value = ChannelType;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("channel type u8")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<ChannelType, E> {
        Ok(ChannelType::from_n(v as u8))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<ChannelType, E> {
        Ok(ChannelType::from_n(v as u8))
    }
}
impl<'de> Deserialize<'de> for ChannelType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(CtVisitor)
    }
}
impl Serialize for ChannelType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.n())
    }
}

/// Channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    /// Channel id.
    pub id: Id<crate::ChannelMarker>,
    /// Kind.
    #[serde(rename = "type")]
    pub kind: ChannelType,
    /// Owning guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
    /// Topic.
    #[serde(default)]
    pub topic: Option<Box<str>>,
    /// Position.
    #[serde(default)]
    pub position: Option<i32>,
    /// Permission overwrites.
    #[serde(default)]
    pub permission_overwrites: Vec<crate::Overwrite>,
    /// Parent category.
    #[serde(default)]
    pub parent_id: Option<Id<crate::ChannelMarker>>,
}
