//! Node.js binding over the rivulus core (napi-rs).
//!
//! No `#![forbid(unsafe_code)]` here on purpose: `napi-derive` expands to
//! `unsafe extern "C"` entry points, so a crate-level forbid cannot compile.
//! Unsafety stays inside generated glue; hand-written code below uses no
//! `unsafe` blocks.
//!
//! Runtime shape (plan section 2.2, decided): this crate owns one dedicated
//! single-thread tokio runtime, installed into napi at module init. Async
//! entry points run on it; no libuv thread ever blocks.

mod api;
mod errors;
mod runtime;
mod snapshot;
mod state;

pub use api::*;
use napi_derive::napi;

/// Install the dedicated runtime into napi at module load.
#[napi::module_init]
fn init() {
    runtime::install();
}

/// Binding version (crate version of `rivulus-node`). Smoke-test surface.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
