//! Typed errors for all of `rivulus` (`model` holds no error type).
use std::sync::atomic::{AtomicU64, Ordering};

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Next process-local request id (cheap `AtomicU64`, never uuid per-request).
#[must_use]
pub fn next_request_id() -> u64 {
    REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

/// Discord gateway close code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseCode(
    /// Numeric close code.
    pub u16,
);

/// All errors produced by `rivulus`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// Gateway failure with resume guidance.
    #[error("gateway {code:?} can_resume={can_resume}: {help}")]
    Gateway {
        /// Close code.
        code: CloseCode,
        /// Whether resume is possible.
        can_resume: bool,
        /// Human remediation hint.
        help: &'static str,
    },
    /// Invalid session (d flag decides resume vs identify).
    #[error("invalid session resumable={resumable}")]
    InvalidSession {
        /// Whether a resume may succeed.
        resumable: bool,
    },
    /// Rate limited (429 or pre-emptive wait).
    #[error("rate limited bucket={bucket} retry_after={retry_after_secs}s global={global}")]
    RateLimited {
        /// Bucket key.
        bucket: Box<str>,
        /// Seconds to wait.
        retry_after_secs: f64,
        /// Whether the global limiter tripped.
        global: bool,
    },
    /// REST error payload.
    #[error("rest {status} code={code} message={message}")]
    Rest {
        /// HTTP status.
        status: u16,
        /// Discord error code.
        code: i64,
        /// Discord message.
        message: Box<str>,
        /// Inner errors, if any.
        errors: Option<Box<str>>,
    },
    /// 401 - never retry, fail fast.
    #[error("unauthorized: check token")]
    Unauthorized,
    /// 403 - never retry.
    #[error("forbidden: missing permissions")]
    Forbidden,
    /// 404.
    #[error("not found: {0}")]
    NotFound(Box<str>),
    /// Timeout.
    #[error("timeout: {0}")]
    Timeout(Box<str>),
    /// Network failure.
    #[error("network: {0}")]
    Network(Box<str>),
    /// Deserialize failure (source truncated to 500 chars by constructors).
    #[error("deserialize {event}: {reason}")]
    Deserialize {
        /// Event or route name.
        event: Box<str>,
        /// Reason (truncated).
        reason: Box<str>,
    },
    /// Client-side validation failure.
    #[error("validation: {0}")]
    Validation(Box<str>),
    /// Cache miss.
    #[error("cache miss")]
    CacheMiss,
    /// Shard unavailable.
    #[error("shard down: {0}")]
    ShardDown(Box<str>),
    /// Backpressure: bounded queue full.
    #[error("shard backpressure: {0}")]
    ShardBackpressure(Box<str>),
    /// Ed25519 signature invalid.
    #[error("signature invalid")]
    SignatureInvalid,
    /// Config error.
    #[error("config: {0}")]
    Config(Box<str>),
    /// Voice is deferred (stub only v1).
    #[error("voice deferred post-1.0, see plans/09 D6")]
    VoiceStub,
}

impl Error {
    /// Human remediation hint.
    #[must_use]
    pub fn help(&self) -> &'static str {
        match self {
            Self::Gateway { code, .. } if code.0 == 4014 => {
                "enable privileged intents in Discord developer portal"
            }
            Self::Gateway { code, .. } if code.0 == 4010 => {
                "check shard id/total math; large bots need multiple-of-N"
            }
            Self::Unauthorized => "verify Bot token, then stop retrying",
            Self::VoiceStub => "voice is stub-only in v1",
            _ => "see docs",
        }
    }
}

/// Truncate a source string for logs (max 500 chars, char-boundary safe).
#[must_use]
pub fn truncate_source(s: &str) -> Box<str> {
    const MAX: usize = 500;
    if s.len() <= MAX {
        Box::from(s)
    } else {
        let mut end = MAX;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        Box::from(&s[..end])
    }
}

/// Result alias.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_ids_monotonic_increasing() {
        let a = next_request_id();
        let b = next_request_id();
        let c = crate::next_request_id();
        assert!(b > a, "request ids must increase: {a} -> {b}");
        assert!(c > b, "request ids must increase: {b} -> {c}");
    }
}
