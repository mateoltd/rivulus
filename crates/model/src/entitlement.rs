//! Entitlements + SKUs + subscriptions.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Entitlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entitlement {
    /// Entitlement id.
    pub id: Id<crate::EntitlementMarker>,
    /// SKU.
    pub sku_id: Id<crate::SkuMarker>,
    /// User.
    #[serde(default)]
    pub user_id: Option<Id<crate::UserMarker>>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
}
/// SKU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sku {
    /// SKU id.
    pub id: Id<crate::SkuMarker>,
    /// Name.
    #[serde(default)]
    pub name: Option<Box<str>>,
}
