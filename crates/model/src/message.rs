//! Messages (author by id; inline author interned at cache insert).
//!
//! Wire note: Discord sends the author as an `author` OBJECT and never sends
//! `author_id`. On parse, `author_id` is taken from an explicit `author_id`
//! key when present (model-shape payloads) and otherwise derived from
//! `author.id`; parsing fails only when neither is present.
use crate::Id;
use serde::{de, Deserialize, Serialize};
use time::OffsetDateTime;

/// Message flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MessageFlags(pub u64);

/// Message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// Message id.
    pub id: Id<crate::MessageMarker>,
    /// Channel id.
    pub channel_id: Id<crate::ChannelMarker>,
    /// Guild id (when in guild).
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Author id (canonical `User` in cache; derived from `author.id` on
    /// Discord-shaped payloads, explicit on model-shaped payloads).
    pub author_id: Id<crate::UserMarker>,
    /// Inline author (interned at insert, not stored duplicated).
    #[serde(default)]
    pub author: Option<crate::User>,
    /// Content.
    pub content: Box<str>,
    /// Timestamp.
    #[serde(with = "time::serde::rfc3339")]
    pub timestamp: OffsetDateTime,
    /// Edited timestamp.
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub edited_timestamp: Option<OffsetDateTime>,
    /// TTS.
    #[serde(default)]
    pub tts: bool,
    /// Mention everyone.
    #[serde(default)]
    pub mention_everyone: bool,
    /// Flags.
    #[serde(default)]
    pub flags: Option<u64>,
    /// Attachments.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Embeds.
    #[serde(default)]
    pub embeds: Vec<Embed>,
}

/// Serde helper: `author_id` is optional on the wire (Discord omits it).
#[derive(Deserialize)]
struct MessageDe {
    id: Id<crate::MessageMarker>,
    channel_id: Id<crate::ChannelMarker>,
    #[serde(default)]
    guild_id: Option<Id<crate::GuildMarker>>,
    #[serde(default)]
    author_id: Option<Id<crate::UserMarker>>,
    #[serde(default)]
    author: Option<crate::User>,
    content: Box<str>,
    #[serde(with = "time::serde::rfc3339")]
    timestamp: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    edited_timestamp: Option<OffsetDateTime>,
    #[serde(default)]
    tts: bool,
    #[serde(default)]
    mention_everyone: bool,
    #[serde(default)]
    flags: Option<u64>,
    #[serde(default)]
    attachments: Vec<Attachment>,
    #[serde(default)]
    embeds: Vec<Embed>,
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let h = MessageDe::deserialize(d)?;
        let author_id = h
            .author_id
            .or_else(|| h.author.as_ref().map(|u| u.id))
            .ok_or_else(|| de::Error::missing_field("author"))?;
        Ok(Self {
            id: h.id,
            channel_id: h.channel_id,
            guild_id: h.guild_id,
            author_id,
            author: h.author,
            content: h.content,
            timestamp: h.timestamp,
            edited_timestamp: h.edited_timestamp,
            tts: h.tts,
            mention_everyone: h.mention_everyone,
            flags: h.flags,
            attachments: h.attachments,
            embeds: h.embeds,
        })
    }
}

/// Attachment metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment id.
    pub id: Id<crate::MessageMarker>,
    /// Filename.
    pub filename: Box<str>,
    /// Size bytes.
    #[serde(default)]
    pub size: u64,
    /// URL.
    #[serde(default)]
    pub url: Box<str>,
    /// Description.
    #[serde(default)]
    pub description: Option<Box<str>>,
}

/// Embed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Embed {
    /// Title.
    #[serde(default)]
    pub title: Option<Box<str>>,
    /// Description.
    #[serde(default)]
    pub description: Option<Box<str>>,
    /// Color.
    #[serde(default)]
    pub color: Option<u32>,
}
