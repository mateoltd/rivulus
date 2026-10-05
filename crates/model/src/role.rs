//! Roles.
use crate::{Id, Permissions};
use serde::{Deserialize, Serialize};

/// Guild role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// Role id.
    pub id: Id<crate::RoleMarker>,
    /// Name.
    pub name: Box<str>,
    /// Position.
    pub position: i32,
    /// Permission bits (string).
    pub permissions: Permissions,
    /// Hoisted.
    #[serde(default)]
    pub hoist: bool,
    /// Managed by integration.
    #[serde(default)]
    pub managed: bool,
    /// Mentionable.
    #[serde(default)]
    pub mentionable: bool,
    /// Primary color.
    #[serde(default)]
    pub color: u32,
    /// Flags.
    #[serde(default)]
    pub flags: u64,
}
