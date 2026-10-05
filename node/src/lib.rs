//! Node.js binding over the rivulus core (napi-rs, M1 scaffold).
//!
//! No `#![forbid(unsafe_code)]` here on purpose: `napi-derive` expands to
//! `unsafe extern "C"` entry points, so a crate-level forbid cannot compile.
//! Unsafety stays inside generated glue; hand-written code below uses no
//! `unsafe` blocks (the M1 gate greps for them).
//!
//! Runtime shape (plan section 2.2, decided): this crate owns one dedicated
//! single-thread tokio runtime on its own OS thread. Async napi entry
//! points (M3) hand futures to it; nothing ever blocks a libuv thread.

pub mod runtime;

use napi_derive::napi;

/// Binding version (crate version of `rivulus-node`). Smoke-test surface.
#[napi]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
