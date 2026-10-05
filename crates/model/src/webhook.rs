//! Webhooks.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Webhook.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Webhook {
    /// Webhook id.
    pub id: Id<crate::WebhookMarker>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Channel.
    #[serde(default)]
    pub channel_id: Option<Id<crate::ChannelMarker>>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
    /// Token (secret; redacted in Debug via wrapper in rest).
    #[serde(default)]
    pub token: Option<Box<str>>,
}
