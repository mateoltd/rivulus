//! `cache` - Cache trait + in-memory impl.
#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod inmemory;
pub mod sweeper;
pub mod r#trait;

pub use inmemory::InMemoryCache;
pub use r#trait::{Cache, CacheConfig, CacheDiff, CacheStats, ResourceType};
