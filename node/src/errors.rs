//! Typed error strings for the Node boundary.
//!
//! Every rejection carries an `Error` whose `.message` is exactly one of the
//! documented strings (the M2 contract pins each one). No `.code` field.

use napi::{Error, Status};

/// Build a boundary error with an exact contract message.
pub fn err(msg: impl Into<String>) -> Error {
    Error::new(Status::GenericFailure, msg.into())
}

/// Unknown or already-closed numeric handle.
pub fn unknown_handle() -> Error {
    err("unknown-handle")
}

/// Handle was closed; the operation cannot proceed.
pub fn client_closed() -> Error {
    err("client-closed")
}

/// Rejected `sharding` option value (only `"auto"` is accepted).
pub fn bad_sharding() -> Error {
    err("bad-sharding")
}

/// Bad numeric or missing id argument.
pub fn bad_id(what: &str) -> Error {
    err(format!("bad-id: {what}"))
}

/// Wrap a core error by its `Display` text (parity rule: same strings as
/// Rust surfaces; never includes the token, which core never prints).
pub fn core(error: impl std::fmt::Display) -> Error {
    err(error.to_string())
}
