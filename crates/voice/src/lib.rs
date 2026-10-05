//! `voice` — STUB ONLY in v1 (deferred post-1.0).
//!
//! Voice send/recv is explicitly deferred (see `plans/09` D6 and `plans/04`
//! §E). This crate compiles an empty [`JoinConfig`] plus a [`JoinConfig::deferred`]
//! helper returning [`common::Error::VoiceStub`], with no WS/UDP/crypto
//! dependencies (only `common` + `model`; see `crates/voice/Cargo.toml`).
//! There is no voice acceptance in v1 gates (`plans/08` P7).
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Join config placeholder (stub).
///
/// v1 carries no voice parameters (no guild/channel/token/ssrc fields).
/// Construct via [`JoinConfig::new`]; any join attempt must go through
/// [`JoinConfig::deferred`], which always returns [`common::Error::VoiceStub`].
/// Full voice spec lives in `plans/04` §E; deferral rationale in `plans/09` D6.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct JoinConfig;

impl JoinConfig {
    /// New stub config (carries no voice parameters).
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Always fails with [`common::Error::VoiceStub`].
    ///
    /// Voice is deferred post-1.0 (`plans/09` D6); v1 only forwards
    /// `VOICE_STATE_UPDATE` / `VOICE_SERVER_UPDATE` as `model::Event`
    /// and never connects (`plans/04` §A + §E).
    #[must_use]
    pub fn deferred() -> common::Error {
        common::Error::VoiceStub
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_returns_voice_stub() {
        let err = JoinConfig::deferred();
        assert!(matches!(err, common::Error::VoiceStub));
        assert_eq!(err.to_string(), "voice deferred post-1.0, see plans/09 D6");
        let cfg = JoinConfig::new();
        let _ = format!("{cfg:?}");
    }
}
