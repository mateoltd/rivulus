//! Handler context (`http`, `cache`, `standby`, `shard`).
use std::sync::Arc;

/// Context handed to handlers and managers.
#[derive(Clone)]
pub struct Context {
    /// REST client.
    pub http: Arc<rest::Client>,
    /// Cache.
    pub cache: Arc<dyn cache::Cache>,
    /// Standby router.
    #[cfg(feature = "standby")]
    pub standby: Arc<standby::Standby>,
    /// Originating shard.
    pub shard: gateway::ShardMessenger,
}

impl std::fmt::Debug for Context {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}
