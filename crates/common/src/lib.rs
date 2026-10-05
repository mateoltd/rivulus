//! `common` — foundation for `rivulus` (error, json shim, validate, oauth, utils, format).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod format;
pub mod json;
pub mod oauth;
pub mod utils;
pub mod validate;

pub use error::{next_request_id, Error, Result};

/// Prelude for `core`.
pub mod prelude {
    pub use crate::error::{Error, Result};
}
