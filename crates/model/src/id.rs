//! Strongly typed snowflake IDs (`Id<T>`).
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Marker for ID types.
///
/// # Example
/// ```
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let id: model::GuildId = "123".parse()?;
/// assert_eq!(id.get(), 123);
/// # Ok(())
/// # }
/// ```
pub trait Resource: Send + Sync + 'static {}

/// Guild marker.
#[derive(Debug)]
pub struct GuildMarker;
/// Channel marker.
#[derive(Debug)]
pub struct ChannelMarker;
/// User marker.
#[derive(Debug)]
pub struct UserMarker;
/// Message marker.
#[derive(Debug)]
pub struct MessageMarker;
/// Role marker.
#[derive(Debug)]
pub struct RoleMarker;
/// Emoji marker.
#[derive(Debug)]
pub struct EmojiMarker;
/// Sticker marker.
#[derive(Debug)]
pub struct StickerMarker;
/// Webhook marker.
#[derive(Debug)]
pub struct WebhookMarker;
/// Application marker.
#[derive(Debug)]
pub struct ApplicationMarker;
/// Scheduled event marker.
#[derive(Debug)]
pub struct ScheduledMarker;
/// Stage instance marker.
#[derive(Debug)]
pub struct StageMarker;
/// Entitlement marker.
#[derive(Debug)]
pub struct EntitlementMarker;
/// SKU marker.
#[derive(Debug)]
pub struct SkuMarker;
/// Audit log entry marker.
#[derive(Debug)]
pub struct AuditMarker;
/// Auto-moderation rule marker.
#[derive(Debug)]
pub struct AutoModMarker;
/// Interaction marker.
#[derive(Debug)]
pub struct InteractionMarker;
/// Thread member key is composite; this marks thread-scoped ids (reuse channel).
#[derive(Debug)]
pub struct ThreadMarker;

impl Resource for GuildMarker {}
impl Resource for ChannelMarker {}
impl Resource for UserMarker {}
impl Resource for MessageMarker {}
impl Resource for RoleMarker {}
impl Resource for EmojiMarker {}
impl Resource for StickerMarker {}
impl Resource for WebhookMarker {}
impl Resource for ApplicationMarker {}
impl Resource for ScheduledMarker {}
impl Resource for StageMarker {}
impl Resource for EntitlementMarker {}
impl Resource for SkuMarker {}
impl Resource for AuditMarker {}
impl Resource for AutoModMarker {}
impl Resource for InteractionMarker {}
impl Resource for ThreadMarker {}

/// Strongly typed Discord snowflake ID (transparent `NonZeroU64`).
// (manual PartialEq/Eq/Hash below - unconditional over T)
#[repr(transparent)]
pub struct Id<T> {
    inner: NonZeroU64,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}
impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl<T> Eq for Id<T> {}
impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.inner.hash(h);
    }
}

impl<T> Id<T> {
    /// Build from raw, `None` when zero.
    #[must_use]
    pub fn new(n: u64) -> Option<Self> {
        NonZeroU64::new(n).map(|inner| Self {
            inner,
            marker: PhantomData,
        })
    }
    /// Raw value.
    #[must_use]
    pub fn get(self) -> u64 {
        self.inner.get()
    }
    /// Milliseconds since Unix epoch embedded in the snowflake.
    #[must_use]
    pub fn timestamp_millis(self) -> i64 {
        common::utils::snowflake_timestamp(self.get())
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.inner)
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.inner.get();
        let mut buf = itoa::Buffer::new();
        f.write_str(buf.format(n))
    }
}

impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

struct IdVisitor<T>(PhantomData<fn() -> T>);

impl<T> Visitor<'_> for IdVisitor<T> {
    type Value = Id<T>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a snowflake as string or integer")
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Id<T>, E> {
        v.parse::<Id<T>>().map_err(|_| E::custom("bad snowflake"))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Id<T>, E> {
        Id::new(v).ok_or_else(|| E::custom("snowflake is zero"))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Id<T>, E> {
        u64::try_from(v)
            .map_err(|_| E::custom("bad snowflake"))
            .and_then(|n| Id::new(n).ok_or_else(|| E::custom("snowflake is zero")))
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(IdVisitor(PhantomData))
    }
}

impl<T> FromStr for Id<T> {
    type Err = common::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let n: u64 = s
            .parse()
            .map_err(|_| common::Error::Validation(Box::from("bad snowflake")))?;
        Id::new(n).ok_or_else(|| common::Error::Validation(Box::from("snowflake is zero")))
    }
}

/// Guild ID.
pub type GuildId = Id<GuildMarker>;
/// Channel ID.
pub type ChannelId = Id<ChannelMarker>;
/// User ID.
pub type UserId = Id<UserMarker>;
/// Message ID.
pub type MessageId = Id<MessageMarker>;
/// Role ID.
pub type RoleId = Id<RoleMarker>;
/// Emoji ID.
pub type EmojiId = Id<EmojiMarker>;
/// Sticker ID.
pub type StickerId = Id<StickerMarker>;
/// Webhook ID.
pub type WebhookId = Id<WebhookMarker>;
/// Application ID.
pub type ApplicationId = Id<ApplicationMarker>;
/// Scheduled event ID.
pub type ScheduledId = Id<ScheduledMarker>;
/// Stage instance ID.
pub type StageId = Id<StageMarker>;
/// Entitlement ID.
pub type EntitlementId = Id<EntitlementMarker>;
/// SKU ID.
pub type SkuId = Id<SkuMarker>;
/// Audit log entry ID.
pub type AuditId = Id<AuditMarker>;
/// Auto-moderation rule ID.
pub type AutoModId = Id<AutoModMarker>;
/// Interaction ID.
pub type InteractionId = Id<InteractionMarker>;
/// Thread ID (channels namespace).
pub type ThreadId = Id<ThreadMarker>;
