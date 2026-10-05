#![allow(missing_docs)]
//! Gateway intents (exact v10 bits + discord.js aliases).
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

bitflags::bitflags! {
    /// Gateway intents.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Intents: u64 {
        const GUILDS = 1 << 0;
        const GUILD_MEMBERS = 1 << 1;
        const GUILD_MODERATION = 1 << 2;
        const GUILD_EMOJIS_AND_STICKERS = 1 << 3;
        const GUILD_INTEGRATIONS = 1 << 4;
        const GUILD_WEBHOOKS = 1 << 5;
        const GUILD_INVITES = 1 << 6;
        const GUILD_VOICE_STATES = 1 << 7;
        const GUILD_PRESENCES = 1 << 8;
        const GUILD_MESSAGES = 1 << 9;
        const GUILD_MESSAGE_REACTIONS = 1 << 10;
        const GUILD_MESSAGE_TYPING = 1 << 11;
        const DIRECT_MESSAGES = 1 << 12;
        const DIRECT_MESSAGE_REACTIONS = 1 << 13;
        const DIRECT_MESSAGE_TYPING = 1 << 14;
        const MESSAGE_CONTENT = 1 << 15;
        const GUILD_SCHEDULED_EVENTS = 1 << 16;
        const AUTO_MODERATION_CONFIGURATION = 1 << 20;
        const AUTO_MODERATION_EXECUTION = 1 << 21;
        const GUILD_MESSAGE_POLLS = 1 << 24;
        const DIRECT_MESSAGE_POLLS = 1 << 25;
        /// discord.js alias: GUILD_BANS.
        const GUILD_BANS = Self::GUILD_MODERATION.bits();
        /// discord.js alias: GUILD_EXPRESSIONS.
        const GUILD_EXPRESSIONS = Self::GUILD_EMOJIS_AND_STICKERS.bits();
    }
}

impl Intents {
    /// Whether privileged intents are present.
    #[must_use]
    pub fn privileged(self) -> bool {
        self.intersects(Self::GUILD_MEMBERS | Self::GUILD_PRESENCES | Self::MESSAGE_CONTENT)
    }
}

struct IntentVisitor;
impl Visitor<'_> for IntentVisitor {
    type Value = Intents;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("intents as int or string")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Intents, E> {
        Ok(Intents::from_bits_truncate(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Intents, E> {
        Ok(Intents::from_bits_truncate(v as u64))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Intents, E> {
        v.parse::<u64>()
            .map(Intents::from_bits_truncate)
            .map_err(|_| E::custom("bad intents"))
    }
}
impl<'de> Deserialize<'de> for Intents {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(IntentVisitor)
    }
}
impl Serialize for Intents {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u64(self.bits())
    }
}
