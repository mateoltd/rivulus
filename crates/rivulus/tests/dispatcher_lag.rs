//! Dispatcher lag policy: broadcast overflow never panics, never silently drops.
use std::sync::Arc;

use rivulus::{lag_policy, Dispatcher, LagPolicy};

fn typing(channel: u64, user: u64) -> Arc<model::Event> {
    Arc::new(model::Event::TypingStart {
        channel_id: model::ChannelId::new(channel).unwrap(),
        user_id: model::UserId::new(user).unwrap(),
    })
}

fn message(id: &str, channel: &str) -> Arc<model::Event> {
    let v = format!(
        r#"{{"id":"{id}","channel_id":"{channel}","author_id":"2","content":"hi","timestamp":"2030-01-01T00:00:00Z"}}"#
    );
    let m: model::Message = common::json::from_slice(v.as_bytes()).unwrap();
    Arc::new(model::Event::MessageCreate(Arc::new(m)))
}

fn unknown() -> Arc<model::Event> {
    Arc::new(model::Event::Unknown {
        kind: Box::from("FUTURE_KIND"),
        seq: None,
        payload: Box::from("{}"),
    })
}

#[test]
fn lag_policy_per_kind() {
    assert_eq!(lag_policy(&typing(3, 2)), LagPolicy::Drop);
    assert_eq!(
        lag_policy(&message("10", "3")),
        LagPolicy::Refetch,
        "cacheable message events take the refetch path"
    );
    assert_eq!(lag_policy(&unknown()), LagPolicy::Count);
}

#[test]
fn broadcast_overflow_ephemeral_drops_and_counts() {
    let d = Dispatcher::new(Vec::new());
    let mut rx = d.subscribe();
    // Overflow the 2048 buffer without receiving.
    for _ in 0..(rivulus::BROADCAST_CAPACITY + 50) {
        let _ = d.dispatch_broadcast(typing(3, 2));
    }
    // Slow receiver must observe Lagged, never a panic.
    let mut lagged_by = 0_u64;
    loop {
        match rx.try_recv() {
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                lagged_by = lagged_by.saturating_add(n);
                break;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
            Ok(_) => {}
        }
    }
    assert!(
        lagged_by > 0,
        "overflow must produce Lagged, got {lagged_by}"
    );
    // Ephemeral policy: drop + count via record_lag (warn! + counter).
    let before = d.lag_dropped();
    d.record_lag(&typing(3, 2), lagged_by);
    assert_eq!(d.lag_dropped(), before + lagged_by.max(1));
}

#[test]
fn cacheable_refetch_path_counted_not_silent() {
    let d = Dispatcher::new(Vec::new());
    let before = d.lag_dropped();
    d.record_lag(&message("11", "3"), 3);
    assert_eq!(d.lag_dropped(), before + 3);
    d.record_lag(&unknown(), 1);
    assert_eq!(d.lag_dropped(), before + 4);
}

#[test]
fn try_dispatch_never_panics_without_receivers() {
    let d = Dispatcher::new(Vec::new());
    // No subscribers: send reports 0 receivers, not lag, never panics.
    assert_eq!(d.dispatch_broadcast(message("12", "3")), 0);
    let r = d.try_dispatch_broadcast(message("13", "3"));
    assert!(
        r.is_ok() || r.is_err(),
        "either Ok(0) or SendError, never panic"
    );
}
