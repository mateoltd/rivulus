//! JSON shim - `serde_json` only in v1 (no simd). Single-pass header peek supported.
use serde::{Deserialize, Serialize};

pub use serde_json::value::RawValue;

/// Deserialize from bytes.
///
/// # Example
/// ```
/// let v: std::collections::HashMap<String, u8> =
///     common::json::from_slice(b"{\"a\":1}").unwrap_or_default();
/// assert_eq!(v["a"], 1);
/// ```
pub fn from_slice<'a, T: Deserialize<'a>>(b: &'a [u8]) -> Result<T, serde_json::Error> {
    serde_json::from_slice(b)
}
/// Deserialize from a [`RawValue`].
pub fn from_raw<T: for<'a> Deserialize<'a>>(r: &RawValue) -> Result<T, serde_json::Error> {
    serde_json::from_str(r.get())
}
/// Serialize to bytes.
pub fn to_vec<T: Serialize + ?Sized>(v: &T) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(v)
}
/// Gateway envelope header for single-pass dispatch (op/t/s + borrowed payload).
#[derive(Debug, Deserialize)]
pub struct Header<'a> {
    /// Opcode.
    pub op: u8,
    /// Dispatch name (op 0 only).
    #[serde(default, borrow)]
    pub t: Option<&'a str>,
    /// Sequence (op 0 only).
    #[serde(default)]
    pub s: Option<u64>,
    /// Raw payload.
    #[serde(borrow)]
    pub d: &'a RawValue,
}
