//! Pre-emptive bucket ratelimiter + global 50rps + ban-meter.
use std::collections::HashMap;
use std::time::Instant;

/// Per-bucket state from headers.
#[derive(Debug, Clone, Copy)]
pub struct BucketState {
    /// Limit.
    pub limit: u64,
    /// Remaining.
    pub remaining: u64,
    /// Reset-after seconds (relative, preferred over absolute Reset).
    pub reset_after_secs: f64,
}

/// Ratelimiter (guarded by `std::sync::Mutex`, short sections).
#[derive(Debug, Default)]
pub struct Ratelimiter {
    buckets: HashMap<String, BucketState>,
    global_until: Option<Instant>,
}

impl Ratelimiter {
    /// New empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Bucket key = method + template + major (never ad-hoc concat elsewhere).
    #[must_use]
    pub fn bucket_key(route: &crate::Route) -> String {
        match route.major_id() {
            Some(m) => format!(
                "{}:{}:{}",
                route.method().as_str(),
                route.path_template(),
                m
            ),
            None => format!("{}:{}", route.method().as_str(), route.path_template()),
        }
    }
    /// Record headers for a bucket.
    pub fn register_headers(&mut self, bucket: &str, limit: u64, remaining: u64, reset_after: f64) {
        self.buckets.insert(
            bucket.to_owned(),
            BucketState {
                limit,
                remaining,
                reset_after_secs: reset_after,
            },
        );
    }
    /// Pre-emptive wait: when `remaining == 0`, sleep `reset_after`.
    #[must_use]
    pub fn should_wait(&self, bucket: &str) -> Option<f64> {
        if let Some(until) = self.global_until {
            if Instant::now() < until {
                return Some(1.0);
            }
        }
        self.buckets.get(bucket).and_then(|b| {
            if b.remaining == 0 {
                Some(b.reset_after_secs)
            } else {
                None
            }
        })
    }
    /// Compute sleep for a 429. Sets global lock when `global == true`.
    pub fn retry_after_for_429(&mut self, retry_after: f64, global: bool) -> f64 {
        if global {
            self.global_until =
                Some(Instant::now() + std::time::Duration::from_secs_f64(retry_after.max(0.0)));
        }
        retry_after
    }
}

/// Invalid-request (Cloudflare ban) meter: 10k/10min. Counts 401/403/429 except shared scope.
#[derive(Debug)]
pub struct BanMeter {
    invalid: u64,
    window_start: Instant,
}

impl BanMeter {
    /// New meter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            invalid: 0,
            window_start: Instant::now(),
        }
    }
    /// Record a status; returns `(warn_50, error_80)`.
    pub fn record(&mut self, status: u16, scope: &str) -> (bool, bool) {
        if self.window_start.elapsed() > std::time::Duration::from_secs(600) {
            self.invalid = 0;
            self.window_start = Instant::now();
        }
        if scope == "shared" {
            return (false, false);
        }
        if status == 401 || status == 403 || status == 429 {
            self.invalid += 1;
        }
        (self.invalid >= 5000, self.invalid >= 8000)
    }
}

impl Default for BanMeter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preemptive_wait() {
        let mut r = Ratelimiter::new();
        r.register_headers("GET:/x:1", 5, 0, 1.5);
        assert_eq!(r.should_wait("GET:/x:1"), Some(1.5));
        r.register_headers("GET:/x:1", 5, 3, 0.0);
        assert_eq!(r.should_wait("GET:/x:1"), None);
    }

    #[test]
    fn shared_not_counted() {
        let mut b = BanMeter::new();
        for _ in 0..9000 {
            b.record(429, "shared");
        }
        assert_eq!(b.invalid, 0);
        let (w, _) = b.record(429, "user");
        assert!(!w);
    }
}
