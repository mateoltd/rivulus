//! Central `SendQueue` (120 cmds/min per shard, presence 5 per 20s).

use std::collections::VecDeque;
use std::time::Duration;
use std::time::Instant;

const CMD_MAX: usize = 120;
const CMD_WINDOW: Duration = Duration::from_secs(60);
const PRESENCE_MAX: usize = 5;
const PRESENCE_WINDOW: Duration = Duration::from_secs(20);

#[derive(Debug, Default)]
struct Inner {
    cmd_times: VecDeque<Instant>,
    presence_times: VecDeque<Instant>,
}

fn prune(q: &mut VecDeque<Instant>, window: Duration, now: Instant) {
    while q.front().is_some_and(|t| now.duration_since(*t) > window) {
        q.pop_front();
    }
}

fn wait_needed(
    q: &VecDeque<Instant>,
    max: usize,
    window: Duration,
    now: Instant,
) -> Option<Duration> {
    if q.len() < max {
        return None;
    }
    q.front().map(|oldest| {
        window
            .checked_sub(now.duration_since(*oldest))
            .unwrap_or_default()
    })
}

/// Token-bucket send queue (one per shard; no direct send bypass).
#[derive(Debug, Default)]
pub struct SendQueue {
    inner: std::sync::Mutex<Inner>,
}

impl SendQueue {
    /// New empty.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(Inner::default()),
        }
    }
    /// Seconds to wait before a normal command (0 = send now).
    pub fn wait_for_command(&self) -> f64 {
        self.wait_secs(false)
    }
    /// Seconds to wait before a presence update (5 per 20s).
    pub fn wait_for_presence(&self) -> f64 {
        self.wait_secs(true)
    }

    fn wait_secs(&self, is_presence: bool) -> f64 {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poison) => poison.into_inner(),
        };
        let now = Instant::now();
        prune(&mut guard.cmd_times, CMD_WINDOW, now);
        prune(&mut guard.presence_times, PRESENCE_WINDOW, now);
        let cmd_wait = wait_needed(&guard.cmd_times, CMD_MAX, CMD_WINDOW, now);
        let presence_wait = if is_presence {
            wait_needed(&guard.presence_times, PRESENCE_MAX, PRESENCE_WINDOW, now)
        } else {
            None
        };
        let wait = match (cmd_wait, presence_wait) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        match wait {
            Some(d) => d.as_secs_f64(),
            None => {
                guard.cmd_times.push_back(now);
                if is_presence {
                    guard.presence_times.push_back(now);
                }
                0.0
            }
        }
    }

    /// Async acquire: waits until a send is allowed, then records it.
    ///
    /// `is_presence` also consumes the `5/20s` sub-bucket on top of the
    /// `120/min` global bucket. Uses short `Mutex` sections plus
    /// `tokio::time::sleep`; no send bypass.
    pub async fn acquire(&self, is_presence: bool) {
        loop {
            let wait = {
                let mut guard = match self.inner.lock() {
                    Ok(g) => g,
                    Err(poison) => poison.into_inner(),
                };
                let now = Instant::now();
                prune(&mut guard.cmd_times, CMD_WINDOW, now);
                prune(&mut guard.presence_times, PRESENCE_WINDOW, now);
                let cmd_wait = wait_needed(&guard.cmd_times, CMD_MAX, CMD_WINDOW, now);
                let presence_wait = if is_presence {
                    wait_needed(&guard.presence_times, PRESENCE_MAX, PRESENCE_WINDOW, now)
                } else {
                    None
                };
                let next = match (cmd_wait, presence_wait) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                match next {
                    Some(d) if !d.is_zero() => d,
                    _ => {
                        guard.cmd_times.push_back(now);
                        if is_presence {
                            guard.presence_times.push_back(now);
                        }
                        Duration::ZERO
                    }
                }
            };
            if wait.is_zero() {
                return;
            }
            tokio::time::sleep(wait).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_cap() {
        let q = SendQueue::new();
        for _ in 0..5 {
            assert_eq!(q.wait_for_presence(), 0.0);
        }
        assert!(q.wait_for_presence() > 0.0);
    }

    #[test]
    fn command_cap() {
        let q = SendQueue::new();
        for _ in 0..120 {
            assert_eq!(q.wait_for_command(), 0.0);
        }
        assert!(q.wait_for_command() > 0.0);
    }

    #[test]
    fn presence_consumes_global() {
        let q = SendQueue::new();
        for _ in 0..5 {
            assert_eq!(q.wait_for_presence(), 0.0);
        }
        assert_eq!(q.wait_for_command(), 0.0);
        for _ in 0..114 {
            assert_eq!(q.wait_for_command(), 0.0);
        }
        assert!(q.wait_for_command() > 0.0);
    }

    #[tokio::test]
    async fn acquire_presence_throttles() {
        let q = SendQueue::new();
        for _ in 0..5 {
            q.acquire(true).await;
        }
        assert!(q.wait_for_presence() > 0.0);
    }
}
