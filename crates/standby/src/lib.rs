//! `standby` - event waiters and collectors (discord.js Collectors port).
#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod collector;
pub mod waiter;

pub use collector::{Collected, Collector, CollectorConfig, CollectorStream, EndReason};
pub use waiter::{Standby, WaiterKind};
