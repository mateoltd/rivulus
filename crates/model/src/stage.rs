//! Stage instances.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Stage instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageInstance {
    /// Stage id.
    pub id: Id<crate::StageMarker>,
    /// Guild.
    #[serde(default)]
    pub guild_id: Option<Id<crate::GuildMarker>>,
    /// Topic.
    #[serde(default)]
    pub topic: Option<Box<str>>,
}
