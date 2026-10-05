//! Mirror placeholder (informational only — never compiled).
//!
//! The workspace root has no package, so `benches/*.rs` here are NOT built
//! by Cargo. The authoritative, executed copy lives at
//! `crates/rivulus/benches/replay.rs`
//! (`[[bench]] harness = false`, std-only, bench-only `CountingAlloc`).
//! Run it with `cargo bench -p rivulus --bench replay`.
