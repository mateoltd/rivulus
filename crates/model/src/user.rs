//! Users.
use crate::Id;
use serde::{Deserialize, Serialize};

/// Discord user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// User id.
    pub id: Id<crate::UserMarker>,
    /// Username.
    pub username: Box<str>,
    /// Discriminator (legacy).
    #[serde(default)]
    pub discriminator: Box<str>,
    /// Global display name.
    #[serde(default)]
    pub global_name: Option<Box<str>>,
    /// Avatar hash.
    #[serde(default)]
    pub avatar: Option<Box<str>>,
    /// Bot flag.
    #[serde(default)]
    pub bot: bool,
    /// System flag.
    #[serde(default)]
    pub system: bool,
}
