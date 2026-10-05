//! Mirror placeholder (informational only — never compiled).
//!
//! The workspace root has no package, so `benches/*.rs` here are NOT built
//! by Cargo. The authoritative, executed copy lives at
//! `crates/rivulus/benches/rest_ratelimit.rs`
//! (`[[bench]] harness = false`, hyper-less scripted mock).
//! Run it with `cargo bench -p rivulus --bench rest_ratelimit`.
