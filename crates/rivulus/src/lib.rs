#![forbid(unsafe_code)]
#![allow(missing_docs)]
//! `rivulus` facade - Client, Context, managers, re-exports.

#[cfg(feature = "cache-inmemory")]
pub use cache;
pub use common;
#[cfg(feature = "framework")]
pub use framework;
#[cfg(feature = "gateway")]
pub use gateway;
#[cfg(feature = "interactions")]
pub use interactions;
pub use model;
#[cfg(feature = "rest")]
pub use rest;
#[cfg(feature = "standby")]
pub use standby;
#[cfg(feature = "voice")]
pub use voice;

#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub mod client;
#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub mod context;
#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub mod dispatcher;
#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub mod managers;

#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub use client::{Client, ClientBuilder, ClientHealth, ShardHealth, ShardStatus};
#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub use context::Context;
#[cfg(all(feature = "gateway", feature = "rest", feature = "cache-inmemory"))]
pub use dispatcher::{lag_policy, Dispatcher, EventHandler, LagPolicy, BROADCAST_CAPACITY};
