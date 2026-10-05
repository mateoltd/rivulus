//! OAuth2 scopes (Discord wire strings, e.g. `"bot"`).
//!
//! Unknown scopes round-trip as their raw string (manual `Deserialize`,
//! mirroring [`crate::ChannelType`): `#[serde(other)]` would collapse every
//! future scope to a single lossy value, breaking re-serialization.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// OAuth2 scope.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Scope {
    /// Bot scope (`"bot"`).
    Bot,
    /// applications.commands (`"applications.commands"`).
    ApplicationsCommands,
    /// identify (`"identify"`).
    Identify,
    /// guilds (`"guilds"`).
    Guilds,
    /// Unknown future scope (raw string, round-trips).
    Unknown(Box<str>),
}

impl Scope {
    fn s(&self) -> &str {
        match self {
            Self::Bot => "bot",
            Self::ApplicationsCommands => "applications.commands",
            Self::Identify => "identify",
            Self::Guilds => "guilds",
            Self::Unknown(s) => s,
        }
    }
    fn from_s(s: &str) -> Self {
        match s {
            "bot" => Self::Bot,
            "applications.commands" => Self::ApplicationsCommands,
            "identify" => Self::Identify,
            "guilds" => Self::Guilds,
            s => Self::Unknown(Box::from(s)),
        }
    }
}
struct ScopeVisitor;
impl Visitor<'_> for ScopeVisitor {
    type Value = Scope;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("oauth2 scope string")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Scope, E> {
        Ok(Scope::from_s(v))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Scope, E> {
        Ok(Scope::from_s(&v))
    }
}
impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_string(ScopeVisitor)
    }
}
impl Serialize for Scope {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.s())
    }
}
