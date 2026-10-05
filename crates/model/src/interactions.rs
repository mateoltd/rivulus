//! Interactions: types 1-5, callbacks 1,4,5,6,7,8,9,10,12 (no 11).
//!
//! Discord sends interaction `type` as a JSON integer. Both int enums carry
//! an `Unknown(u8)` round-trip arm (manual `Deserialize`, mirroring
//! [`crate::ChannelType`]) so future Discord numbers parse and re-serialize
//! byte-identically instead of failing.
use crate::Id;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
/// Interaction type (Discord `type` int 1-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InteractionType {
    /// Ping = 1.
    Ping,
    /// ApplicationCommand = 2.
    ApplicationCommand,
    /// MessageComponent = 3.
    MessageComponent,
    /// Autocomplete = 4.
    Autocomplete,
    /// ModalSubmit = 5.
    ModalSubmit,
    /// Unknown future type (round-trips).
    Unknown(u8),
}

impl InteractionType {
    fn n(self) -> u8 {
        match self {
            Self::Ping => 1,
            Self::ApplicationCommand => 2,
            Self::MessageComponent => 3,
            Self::Autocomplete => 4,
            Self::ModalSubmit => 5,
            Self::Unknown(n) => n,
        }
    }
    fn from_n(n: u8) -> Self {
        match n {
            1 => Self::Ping,
            2 => Self::ApplicationCommand,
            3 => Self::MessageComponent,
            4 => Self::Autocomplete,
            5 => Self::ModalSubmit,
            n => Self::Unknown(n),
        }
    }
}
struct InteractionTypeVisitor;
impl Visitor<'_> for InteractionTypeVisitor {
    type Value = InteractionType;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("interaction type u8")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<InteractionType, E> {
        Ok(InteractionType::from_n(v as u8))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<InteractionType, E> {
        Ok(InteractionType::from_n(v as u8))
    }
}
impl<'de> Deserialize<'de> for InteractionType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(InteractionTypeVisitor)
    }
}
impl Serialize for InteractionType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.n())
    }
}
/// Interaction callback type (Discord `type` int; no 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CallbackType {
    /// Pong = 1.
    Pong,
    /// ChannelMessageWithSource = 4.
    ChannelMessage,
    /// DeferredChannelMessage = 5.
    DeferredChannel,
    /// DeferredUpdate = 6.
    DeferredUpdate,
    /// UpdateMessage = 7.
    UpdateMessage,
    /// AutocompleteResult = 8.
    AutocompleteResult,
    /// Modal = 9.
    Modal,
    /// PremiumRequired = 10.
    PremiumRequired,
    /// LaunchActivity = 12.
    LaunchActivity,
    /// Unknown future callback (round-trips).
    Unknown(u8),
}

impl CallbackType {
    fn n(self) -> u8 {
        match self {
            Self::Pong => 1,
            Self::ChannelMessage => 4,
            Self::DeferredChannel => 5,
            Self::DeferredUpdate => 6,
            Self::UpdateMessage => 7,
            Self::AutocompleteResult => 8,
            Self::Modal => 9,
            Self::PremiumRequired => 10,
            Self::LaunchActivity => 12,
            Self::Unknown(n) => n,
        }
    }
    fn from_n(n: u8) -> Self {
        match n {
            1 => Self::Pong,
            4 => Self::ChannelMessage,
            5 => Self::DeferredChannel,
            6 => Self::DeferredUpdate,
            7 => Self::UpdateMessage,
            8 => Self::AutocompleteResult,
            9 => Self::Modal,
            10 => Self::PremiumRequired,
            12 => Self::LaunchActivity,
            n => Self::Unknown(n),
        }
    }
}
struct CallbackTypeVisitor;
impl Visitor<'_> for CallbackTypeVisitor {
    type Value = CallbackType;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("callback type u8")
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<CallbackType, E> {
        Ok(CallbackType::from_n(v as u8))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<CallbackType, E> {
        Ok(CallbackType::from_n(v as u8))
    }
}
impl<'de> Deserialize<'de> for CallbackType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(CallbackTypeVisitor)
    }
}
impl Serialize for CallbackType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.n())
    }
}
/// Command option value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    /// String.
    Str(Box<str>),
    /// Integer.
    Int(i64),
    /// Boolean.
    Bool(bool),
}
/// Command option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOption {
    /// Name (validated lowercase <=32).
    #[serde(rename = "name")]
    pub name: Box<str>,
    /// Kind (3 str, 4 int, 5 bool, 6 user, 7 channel, 8 role, 9 mentionable, 10 number, 11 attachment).
    #[serde(rename = "type")]
    pub kind: u8,
    /// Required.
    #[serde(default)]
    pub required: bool,
    /// Value.
    #[serde(default)]
    pub value: Option<OptionValue>,
}
/// Interaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interaction {
    /// Interaction id.
    pub id: Id<crate::InteractionMarker>,
    /// Application id.
    pub application_id: Id<crate::ApplicationMarker>,
    /// Type (1-5, Discord `type` key).
    #[serde(rename = "type")]
    pub kind: InteractionType,
    /// Token (secret, 15min).
    pub token: Box<str>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Channel.
    #[serde(default)]
    pub channel_id: Option<Id<crate::ChannelMarker>>,
    /// Custom id (components/modals).
    #[serde(default)]
    pub custom_id: Option<Box<str>>,
}
