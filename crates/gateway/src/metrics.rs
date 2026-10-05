//! Gateway metrics (plain `AtomicU64` counters, no `metrics` crate dep).
//!
//! Tracks dispatch volume plus the `gateway_unknown_events_total` counter
//! required by `plans/06` §1/§5: unknown dispatches are never silent
//! (`tracing::warn!` + count).

use std::sync::atomic::{AtomicU64, Ordering};

/// Counter for unknown dispatch events (`Event::Unknown`).
///
/// Cheap process-local `AtomicU64`; `record_unknown(kind)` bumps the count
/// and emits a `tracing::warn!` (never silent per `plans/06` §1).
#[derive(Debug, Default)]
pub struct UnknownCounter {
    inner: AtomicU64,
}

impl UnknownCounter {
    /// New zeroed counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one unknown dispatch of `kind` (`warn!` + count).
    pub fn record_unknown(&self, kind: &str) {
        self.inner.fetch_add(1, Ordering::Relaxed);
        tracing::warn!(kind, "unknown gateway dispatch counted");
    }

    /// Current count.
    #[must_use]
    pub fn get(&self) -> u64 {
        self.inner.load(Ordering::Relaxed)
    }
}

/// Aggregate gateway counters.
///
/// `events_total` counts dispatches seen, `resumes_total` counts successful
/// resumes, `unknown_total` counts `Event::Unknown` (mirrors
/// `gateway_unknown_events_total` from `plans/06` §5).
#[derive(Debug, Default)]
pub struct Metrics {
    /// Total dispatch events seen.
    pub events_total: AtomicU64,
    /// Total successful resumes.
    pub resumes_total: AtomicU64,
    /// Total unknown dispatches.
    pub unknown_total: AtomicU64,
}

impl Metrics {
    /// New zeroed metrics.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bump `events_total`.
    pub fn inc_event(&self) {
        self.events_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Bump `resumes_total`.
    pub fn inc_resume(&self) {
        self.resumes_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Bump `unknown_total` (count only, no log).
    pub fn inc_unknown(&self) {
        self.unknown_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Report one unknown dispatch: count + `warn!` (never silent).
    pub fn report_unknown(&self, kind: &str) {
        self.inc_unknown();
        tracing::warn!(
            kind,
            unknown_total = self.unknown_total.load(Ordering::Relaxed),
            "unknown gateway dispatch counted"
        );
    }

    /// Current `(events, resumes, unknown)` snapshot.
    #[must_use]
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.events_total.load(Ordering::Relaxed),
            self.resumes_total.load(Ordering::Relaxed),
            self.unknown_total.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_counter_warns_and_counts() {
        let c = UnknownCounter::new();
        assert_eq!(c.get(), 0);
        c.record_unknown("FUTURE_KIND");
        c.record_unknown("FUTURE_KIND");
        assert_eq!(c.get(), 2);
    }

    #[test]
    fn metrics_snapshot() {
        let m = Metrics::new();
        m.inc_event();
        m.inc_event();
        m.inc_resume();
        m.report_unknown("FUTURE_KIND");
        let (events, resumes, unknown) = m.snapshot();
        assert_eq!(events, 2);
        assert_eq!(resumes, 1);
        assert_eq!(unknown, 1);
    }
}
