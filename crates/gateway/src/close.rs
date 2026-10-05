//! Close-code matrix (4007 corrected: fresh Identify, never Resume).
//!
//! Exhaustive mapping for `4000..=4014` plus transport/unknown.
//! `4007` (bad `seq`) MUST fresh-identify, never resume: the session
//! `seq` is poisoned. All resumes use `resume_gateway_url` from `READY`,
//! never the cached bootstrap `url`. Callers apply exp backoff
//! (`1s -> 120s` with jitter) before `Resume`/`BackoffResume`; when there
//! is no `session_id + seq`, even `Resume`/`BackoffResume` degrades to a
//! fresh `Identify`.

/// Action after a close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Resume `op6 { token, session_id, seq }` on `resume_gateway_url`.
    Resume,
    /// Fresh `Identify` (no resume attempt).
    FreshIdentify,
    /// Fail fast, no retry.
    FailFast {
        /// Human remediation hint.
        help: &'static str,
    },
    /// Resume with extra backoff (rate-limited / transport drop).
    BackoffResume,
}

impl CloseAction {
    /// Whether this action attempts a resume (`Resume` or `BackoffResume`).
    #[must_use]
    pub fn can_resume(self) -> bool {
        matches!(self, Self::Resume | Self::BackoffResume)
    }
    /// Human guidance for this action.
    #[must_use]
    pub fn help(self) -> &'static str {
        match self {
            Self::Resume => "resume with backoff on resume_gateway_url",
            Self::FreshIdentify => "fresh identify, never resume",
            Self::FailFast { help } => help,
            Self::BackoffResume => "backoff then resume on resume_gateway_url",
        }
    }
}

/// Classify a close code.
///
/// Mapping (normative, see `plans/04` §A):
/// `4000/4001/4002/4003/4005/4009` => `Resume`,
/// `4008` + unknown/transport => `BackoffResume`,
/// `4006/4007` => `FreshIdentify` (4007 never resumes),
/// `4004/4010/4011/4012/4013/4014` => `FailFast`, no retry.
#[must_use]
pub fn classify(code: u16) -> CloseAction {
    match code {
        4000 | 4001 | 4002 | 4003 | 4005 | 4009 => CloseAction::Resume,
        4006 | 4007 => CloseAction::FreshIdentify,
        4008 => CloseAction::BackoffResume,
        4004 => CloseAction::FailFast {
            help: "verify Bot token, then stop retrying",
        },
        4010 => CloseAction::FailFast {
            help: "check shard id/total math; large bots need multiple-of-N",
        },
        4011 => CloseAction::FailFast {
            help: "sharding required for large bots",
        },
        4012 => CloseAction::FailFast {
            help: "invalid API version: use v10",
        },
        4013 => CloseAction::FailFast {
            help: "invalid intent(s): check intent bits",
        },
        4014 => CloseAction::FailFast {
            help: "enable privileged intents in Discord developer portal",
        },
        _ => CloseAction::BackoffResume,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_seq_fresh_identifies() {
        assert_eq!(classify(4007), CloseAction::FreshIdentify);
        assert!(!classify(4007).can_resume());
    }

    #[test]
    fn full_matrix() {
        for code in [4000, 4001, 4002, 4003, 4005, 4009] {
            assert_eq!(classify(code), CloseAction::Resume, "code {code}");
            assert!(classify(code).can_resume());
        }
        for code in [4006, 4007] {
            assert_eq!(classify(code), CloseAction::FreshIdentify, "code {code}");
            assert!(!classify(code).can_resume());
        }
        assert_eq!(classify(4008), CloseAction::BackoffResume);
        assert!(classify(4008).can_resume());
        for code in [4004, 4010, 4011, 4012, 4013, 4014] {
            assert!(
                matches!(classify(code), CloseAction::FailFast { .. }),
                "code {code}"
            );
            assert!(!classify(code).can_resume());
            assert!(!classify(code).help().is_empty());
        }
        assert_eq!(classify(4014).help(), classify(4014).help());
        let help_4014 = match classify(4014) {
            CloseAction::FailFast { help } => help,
            _ => "",
        };
        assert!(help_4014.contains("privileged"));
        let help_4010 = match classify(4010) {
            CloseAction::FailFast { help } => help,
            _ => "",
        };
        assert!(help_4010.contains("shard"));
        for code in [1000u16, 1001, 1006, 1011, 4900, 9999] {
            assert_eq!(classify(code), CloseAction::BackoffResume, "code {code}");
        }
    }

    #[test]
    fn fail_fast_has_help() {
        for code in [4004, 4010, 4011, 4012, 4013, 4014] {
            let action = classify(code);
            assert!(!action.can_resume());
            assert!(!action.help().is_empty());
        }
    }
}
