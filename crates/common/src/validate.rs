//! Shared validation limits (mirrors twilight-validate; used by builders + interactions + framework).
use crate::error::{Error, Result};

/// Max message content chars.
pub const CONTENT_MAX: usize = 2000;
/// Max embeds per message.
pub const EMBEDS_MAX: usize = 10;
/// Max embed total chars.
pub const EMBED_TOTAL_MAX: usize = 6000;
/// Max action rows.
pub const ROWS_MAX: usize = 5;
/// Max buttons per row.
pub const ROW_BUTTONS_MAX: usize = 5;
/// Max autocomplete choices.
pub const AUTOCOMPLETE_MAX: usize = 25;
/// Max bulk delete.
pub const BULK_MAX: usize = 100;
/// Min bulk delete.
pub const BULK_MIN: usize = 2;
/// Audit reason decoded max.
pub const AUDIT_REASON_MAX: usize = 512;
/// Command name max.
pub const COMMAND_NAME_MAX: usize = 32;
/// V2 top-level components max.
pub const V2_TOP_MAX: usize = 40;
/// V2 container children max.
pub const V2_CONTAINER_MAX: usize = 10;
/// V2 TextDisplay max.
pub const V2_TEXT_MAX: usize = 4000;

/// Validate message content length.
///
/// # Errors
/// Returns [`Error::Validation`] when too long.
pub fn content(s: &str) -> Result<()> {
    if s.chars().count() > CONTENT_MAX {
        return Err(Error::Validation(Box::from("content > 2000 chars")));
    }
    Ok(())
}
/// Validate audit reason (decoded 1-512, no CRLF), returns percent-encoded form.
///
/// # Errors
/// Returns [`Error::Validation`] on empty/too long/CRLF.
pub fn audit_reason(reason: &str) -> Result<String> {
    if reason.is_empty() || reason.chars().count() > AUDIT_REASON_MAX {
        return Err(Error::Validation(Box::from(
            "audit reason must be 1-512 chars",
        )));
    }
    if reason.contains('\r') || reason.contains('\n') {
        return Err(Error::Validation(Box::from(
            "audit reason must not contain CRLF",
        )));
    }
    Ok(
        percent_encoding::utf8_percent_encode(reason, percent_encoding::NON_ALPHANUMERIC)
            .to_string(),
    )
}
/// Validate command name (lowercase, <=32).
///
/// # Errors
/// Returns [`Error::Validation`] on violation.
pub fn command_name(name: &str) -> Result<()> {
    if name.is_empty() || name.chars().count() > COMMAND_NAME_MAX {
        return Err(Error::Validation(Box::from(
            "command name must be 1-32 chars",
        )));
    }
    if name.chars().any(|c| c.is_ascii_uppercase() || c == ' ') {
        return Err(Error::Validation(Box::from(
            "command name must be lowercase without spaces",
        )));
    }
    Ok(())
}
/// Validate bulk delete count.
///
/// # Errors
/// Returns [`Error::Validation`] outside 2-100.
pub fn bulk_count(n: usize) -> Result<()> {
    if !(BULK_MIN..=BULK_MAX).contains(&n) {
        return Err(Error::Validation(Box::from("bulk delete must be 2-100")));
    }
    Ok(())
}
/// Sanitize multipart filename (no path separators / CRLF).
///
/// # Errors
/// Returns [`Error::Validation`] on violation.
pub fn filename(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\r')
        || name.contains('\n')
    {
        return Err(Error::Validation(Box::from("bad filename")));
    }
    Ok(())
}
