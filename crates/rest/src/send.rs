//! Retry policy for REST execution (used by [`crate::Client`]).
//!
//! Rules (per plans/04B + 06§3): never retry `401`/`403`/`404`
//! (webhook `404` marks dead upstream); retry idempotent
//! `GET`/`PUT`/`DELETE` on `5xx` with jittered backoff, max 3 attempts;
//! never auto-retry `POST` message-create (duplicate risk).

/// Decide whether a failed attempt should be retried.
///
/// `attempts` is 1-based (1 = first attempt just failed).
/// Returns `true` only for idempotent `GET`/`PUT`/`DELETE` on `5xx`
/// while `attempts < 3`.
#[must_use]
pub fn should_retry(method: crate::Method, status: u16, attempts: u32) -> bool {
    if status == 401 || status == 403 || status == 404 || status == 429 {
        return false;
    }
    if !(500..600).contains(&status) {
        return false;
    }
    if attempts >= 3 {
        return false;
    }
    matches!(
        method,
        crate::Method::Get | crate::Method::Put | crate::Method::Delete
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_retry_auth_or_missing() {
        for status in [401, 403, 404, 429] {
            assert!(!should_retry(crate::Method::Get, status, 1));
            assert!(!should_retry(crate::Method::Post, status, 1));
        }
    }

    #[test]
    fn idempotent_5xx_retries_max3() {
        assert!(should_retry(crate::Method::Get, 500, 1));
        assert!(should_retry(crate::Method::Put, 503, 2));
        assert!(should_retry(crate::Method::Delete, 500, 2));
        assert!(!should_retry(crate::Method::Get, 500, 3));
        assert!(!should_retry(crate::Method::Get, 500, 4));
    }

    #[test]
    fn never_retry_post_message_create() {
        assert!(!should_retry(crate::Method::Post, 500, 1));
        assert!(!should_retry(crate::Method::Post, 502, 2));
        assert!(!should_retry(crate::Method::Patch, 500, 1));
    }

    #[test]
    fn client_errors_not_retried() {
        assert!(!should_retry(crate::Method::Get, 400, 1));
        assert!(!should_retry(crate::Method::Get, 200, 1));
    }
}
