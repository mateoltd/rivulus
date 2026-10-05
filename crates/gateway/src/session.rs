//! Per-shard session (`session_id`, `resume_gateway_url`, `seq`, identify).
//!
//! `seq` is persisted BEFORE fan-out so resume never replays a dispatch
//! that was already handed to handlers. All resumes use `resume_url`
//! (`resume_gateway_url` from `READY`), never the bootstrap `url`.

use std::sync::Mutex;

/// Session state.
#[derive(Debug, Clone, Default)]
pub struct Session {
    /// Session id from READY.
    pub session_id: Option<Box<str>>,
    /// Resume URL from READY (ALL resumes use this).
    pub resume_url: Option<Box<str>>,
    /// Last dispatch seq (persisted BEFORE fan-out).
    pub seq: Option<u64>,
}

impl Session {
    /// Whether a resume can be attempted (needs both id and seq).
    #[must_use]
    pub fn can_resume(&self) -> bool {
        self.session_id.is_some() && self.seq.is_some()
    }
    /// Resume URL with fallback to the bootstrap gateway URL.
    #[must_use]
    pub fn gateway_url<'a>(&'a self, fallback: &'a str) -> &'a str {
        self.resume_url.as_deref().unwrap_or(fallback)
    }
}

/// Thread-safe session store.
pub trait SessionStore: Send + Sync + 'static {
    /// Set READY values.
    fn set_ready(&self, session_id: Box<str>, resume_url: Box<str>);
    /// Bump seq (call BEFORE fan-out so resume never replays).
    fn set_seq(&self, seq: u64);
    /// Snapshot.
    fn snapshot(&self) -> Session;
    /// Whether resume is possible.
    fn can_resume(&self) -> bool {
        self.snapshot().can_resume()
    }
    /// Clear on fresh identify.
    fn clear(&self);
}

/// In-memory session store.
#[derive(Debug, Default)]
pub struct MemorySessionStore {
    inner: Mutex<Session>,
}

impl MemorySessionStore {
    /// New empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionStore for MemorySessionStore {
    fn set_ready(&self, session_id: Box<str>, resume_url: Box<str>) {
        if let Ok(mut s) = self.inner.lock() {
            s.session_id = Some(session_id);
            s.resume_url = Some(resume_url);
        }
    }
    fn set_seq(&self, seq: u64) {
        if let Ok(mut s) = self.inner.lock() {
            s.seq = Some(seq);
        }
    }
    fn snapshot(&self) -> Session {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }
    fn clear(&self) {
        if let Ok(mut s) = self.inner.lock() {
            s.session_id = None;
            s.resume_url = None;
            s.seq = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_persisted() {
        let store = MemorySessionStore::new();
        assert!(!store.can_resume());
        store.set_ready(Box::from("sess"), Box::from("wss://resume/"));
        assert!(!store.can_resume());
        store.set_seq(42);
        let snap = store.snapshot();
        assert_eq!(snap.session_id.as_deref(), Some("sess"));
        assert_eq!(snap.resume_url.as_deref(), Some("wss://resume/"));
        assert_eq!(snap.seq, Some(42));
        assert!(snap.can_resume());
        assert!(store.can_resume());
        assert_eq!(snap.gateway_url("fallback"), "wss://resume/");
        assert_eq!(Session::default().gateway_url("fallback"), "fallback");
    }

    #[test]
    fn clear_resets() {
        let store = MemorySessionStore::new();
        store.set_ready(Box::from("s"), Box::from("u"));
        store.set_seq(1);
        store.clear();
        assert!(!store.can_resume());
    }
}
