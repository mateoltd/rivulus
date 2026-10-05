//! Audit log.
use crate::Id;
use serde::{Deserialize, Serialize};
/// Audit log entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry id.
    pub id: Id<crate::AuditMarker>,
    /// Action type.
    #[serde(default)]
    pub action_type: u64,
    /// User.
    #[serde(default)]
    pub user_id: Option<Id<crate::UserMarker>>,
    /// Reason.
    #[serde(default)]
    pub reason: Option<Box<str>>,
}
