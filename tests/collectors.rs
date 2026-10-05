//! Collector end-reasons incl delete-ends (+ filter/max/idle/timeout/stream).
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use standby::{Collected, Collector, CollectorConfig, CollectorStream, EndReason, Standby, WaiterKind};

fn resumed() -> Arc<model::Event> {
    Arc::new(model::Event::Resumed)
}

fn msg_event(id: &str, channel: &str) -> Arc<model::Event> {
    let v = format!(
        r#"{{"id":"{id}","channel_id":"{channel}","author_id":"2","content":"hi","timestamp":"2030-01-01T00:00:00Z"}}"#
    );
    let m: model::Message = common::json::from_slice(v.as_bytes()).unwrap();
    Arc::new(model::Event::MessageCreate(Arc::new(m)))
}

#[test]
fn filter_and_max() {
    let mut c = Collector::with_filter(
        CollectorConfig {
            max: Some(2),
            time: Duration::from_secs(60),
            idle: None,
        },
        |ev| matches!(ev, model::Event::MessageCreate(_)),
    );
    assert_eq!(c.push(resumed()), None);
    assert!(c.items().is_empty());
    assert_eq!(c.push(msg_event("1", "3")), None);
    assert_eq!(c.items().len(), 1);
    assert_eq!(c.push(msg_event("2", "3")), Some(EndReason::Limit));
    assert_eq!(c.items().len(), 2);
    assert_eq!(c.end_reason(), Some(EndReason::Limit));
}

#[test]
fn idle_and_timeout() {
    let mut c = Collector::new(CollectorConfig {
        max: None,
        time: Duration::from_secs(60),
        idle: Some(Duration::from_millis(1)),
    });
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(c.push(resumed()), Some(EndReason::Idle));

    let mut c = Collector::new(CollectorConfig {
        max: None,
        time: Duration::from_millis(1),
        idle: None,
    });
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(c.push(resumed()), Some(EndReason::Time));
}

#[test]
fn delete_ends() {
    let ch = model::ChannelId::new(3).unwrap();
    let msg = model::MessageId::new(4).unwrap();
    let gid = model::GuildId::new(5).unwrap();

    let mut c = Collector::new(CollectorConfig::default());
    assert_eq!(
        c.push(Arc::new(model::Event::MessageDelete {
            channel_id: ch,
            message_id: msg,
        })),
        Some(EndReason::MessageDelete)
    );

    let mut c = Collector::new(CollectorConfig::default());
    let ch_obj = model::Channel {
        id: ch,
        kind: model::ChannelType::GuildText,
        guild_id: None,
        name: None,
        topic: None,
        position: None,
        permission_overwrites: Vec::new(),
        parent_id: None,
    };
    assert_eq!(
        c.push(Arc::new(model::Event::ChannelDelete(Arc::new(ch_obj)))),
        Some(EndReason::ChannelDelete)
    );

    let mut c = Collector::new(CollectorConfig::default());
    assert_eq!(
        c.push(Arc::new(model::Event::GuildDelete {
            guild_id: gid,
            unavailable: false,
        })),
        Some(EndReason::GuildDelete)
    );

    let mut c = Collector::new(CollectorConfig::default());
    let thread = model::Thread {
        id: ch,
        parent_id: None,
        guild_id: None,
        name: None,
    };
    assert_eq!(
        c.push(Arc::new(model::Event::ThreadDelete(Arc::new(thread)))),
        Some(EndReason::ThreadDelete)
    );
}

#[test]
fn user_stop() {
    let mut c = Collector::new(CollectorConfig::default());
    c.stop(EndReason::User);
    assert_eq!(c.end_reason(), Some(EndReason::User));
    assert_eq!(c.push(resumed()), Some(EndReason::User));
}

#[tokio::test]
async fn stream_yields_then_limit_ends() {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let coll = Collector::new(CollectorConfig {
        max: Some(2),
        time: Duration::from_secs(60),
        idle: None,
    });
    let mut s = CollectorStream::new(coll, rx);
    tx.unbounded_send(msg_event("10", "3")).unwrap();
    tx.unbounded_send(msg_event("11", "3")).unwrap();
    drop(tx);
    let a: Collected = s.next().await.expect("first");
    let b: Collected = s.next().await.expect("second");
    assert!(matches!(a.event.as_ref(), model::Event::MessageCreate(_)));
    assert!(matches!(b.event.as_ref(), model::Event::MessageCreate(_)));
    assert!(s.next().await.is_none());
    assert_eq!(s.end_reason(), Some(EndReason::Limit));
}

#[tokio::test]
async fn stream_delete_ends() {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let coll = Collector::new(CollectorConfig::default());
    let mut s = CollectorStream::new(coll, rx);
    let ch = model::ChannelId::new(3).unwrap();
    let msg = model::MessageId::new(4).unwrap();
    tx.unbounded_send(Arc::new(model::Event::MessageDelete {
        channel_id: ch,
        message_id: msg,
    }))
    .unwrap();
    drop(tx);
    assert!(s.next().await.is_none());
    assert_eq!(s.end_reason(), Some(EndReason::MessageDelete));
}

#[tokio::test]
async fn stream_idle_resets_per_item() {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let coll = Collector::new(CollectorConfig {
        max: None,
        time: Duration::from_secs(60),
        idle: Some(Duration::from_millis(80)),
    });
    let mut s = CollectorStream::new(coll, rx);
    tx.unbounded_send(msg_event("20", "3")).unwrap();
    let first = tokio::time::timeout(Duration::from_millis(50), s.next())
        .await
        .expect("on time")
        .expect("item");
    assert!(matches!(first.event.as_ref(), model::Event::MessageCreate(_)));
    tokio::time::sleep(Duration::from_millis(30)).await;
    tx.unbounded_send(msg_event("21", "3")).unwrap();
    let second = tokio::time::timeout(Duration::from_millis(50), s.next())
        .await
        .expect("idle reset")
        .expect("item");
    assert!(matches!(second.event.as_ref(), model::Event::MessageCreate(_)));
    drop(tx);
}

#[tokio::test]
async fn standby_wrappers() {
    let s = Arc::new(Standby::new());
    let ch = model::ChannelId::new(3).unwrap();
    let h = tokio::spawn({
        let s = s.clone();
        async move { s.wait_for_message_in(ch, Duration::from_millis(300)).await }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    s.feed(msg_event("30", "9"));
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(!h.is_finished());
    let want = msg_event("31", "3");
    s.feed(want.clone());
    let got = h.await.unwrap().unwrap();
    assert!(Arc::ptr_eq(&got, &want));

    let mid = model::MessageId::new(40).unwrap();
    let h = tokio::spawn({
        let s = s.clone();
        async move { s.wait_for_reaction_on(mid, Duration::from_millis(300)).await }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let add = Arc::new(model::Event::ReactionAdd {
        channel_id: ch,
        message_id: mid,
        user_id: model::UserId::new(2).unwrap(),
        emoji: Box::from("x"),
    });
    s.feed(add.clone());
    let got = h.await.unwrap().unwrap();
    assert!(Arc::ptr_eq(&got, &add));

    let h = tokio::spawn({
        let s = s.clone();
        async move { s.await_modal_submit("form-1", Duration::from_millis(300)).await }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    let inter = Arc::new(model::Event::InteractionCreate(Arc::new(model::Interaction {
        id: model::InteractionId::new(7).unwrap(),
        application_id: model::ApplicationId::new(8).unwrap(),
        kind: model::InteractionType::ModalSubmit,
        token: Box::from("t"),
        guild_id: None,
        channel_id: None,
        custom_id: Some(Box::from("form-1")),
    })));
    s.feed(inter.clone());
    let got = h.await.unwrap().unwrap();
    assert!(Arc::ptr_eq(&got, &inter));

    let _ = WaiterKind::Any;
}
