//! Gateway resume acceptance: drop->resume, 4007 fresh identify, latency.
#[path = "../../../tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gateway::SessionStore as _;

fn cfg() -> gateway::ShardConfig {
    gateway::ShardConfig::new(
        secrecy::SecretString::from(String::from("test-token")),
        model::Intents::GUILDS,
        gateway::ShardId { id: 0, total: 1 },
        50,
    )
}

fn messenger_with(latency: u64) -> gateway::ShardMessenger {
    gateway::ShardMessenger {
        shard: gateway::ShardId { id: 0, total: 1 },
        latency: Arc::new(AtomicU64::new(latency)),
    }
}

async fn recv_timeout(
    rx: &mut tokio::sync::mpsc::Receiver<model::Event>,
    ms: u64,
) -> Option<model::Event> {
    tokio::time::timeout(Duration::from_millis(ms), rx.recv())
        .await
        .ok()
        .flatten()
}

#[tokio::test]
async fn drop_resumes_with_same_session_and_seq() {
    let (addr, seen) = mock_gateway::spawn_script().await;
    let base = format!("ws://{addr}");
    let url = gateway::connect_url(&base);
    assert!(
        url.contains("compress=zlib-stream"),
        "connect URL must carry compress param: {url}"
    );
    let store = Arc::new(gateway::MemorySessionStore::new());
    let messenger = messenger_with(999_999);
    let queue = Arc::new(gateway::InMemoryQueue::new(1));
    let send_q = Arc::new(gateway::SendQueue::new());
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(64);
    let shutdown = tokio_util::sync::CancellationToken::new();

    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        gateway::Shard::run(
            cfg(),
            messenger.clone(),
            store.clone(),
            queue.clone(),
            send_q.clone(),
            None,
            Some(base.clone()),
            out_tx.clone(),
            shutdown.clone(),
        ),
    )
    .await
    .expect("first run must finish after mock drop");
    assert!(
        matches!(
            exit,
            gateway::ShardExit::Resume | gateway::ShardExit::BackoffResume
        ),
        "transport drop with session must resume, got {exit:?}"
    );
    let snap = store.snapshot();
    assert_eq!(snap.session_id.as_deref(), Some("sess-1"));
    assert_eq!(snap.seq, Some(2));

    let first = recv_timeout(&mut out_rx, 2000).await.expect("READY event");
    assert!(matches!(first, model::Event::Ready(_)), "got {first:?}");
    let second = recv_timeout(&mut out_rx, 2000)
        .await
        .expect("dispatch event");
    assert!(
        matches!(second, model::Event::Unknown { .. }),
        "future dispatch kind must surface as Unknown, got {second:?}"
    );

    {
        let s = seen.lock().unwrap_or_else(|e| e.into_inner());
        assert!(
            !s.paths.is_empty() && s.paths.iter().all(|p| p.contains("compress=zlib-stream")),
            "mock must observe compress query param: {:?}",
            s.paths
        );
    }
    let latency = messenger.latency_ms();
    assert!(
        latency < 1000,
        "Ack must update messenger latency (sentinel 999999 -> {latency})"
    );

    let shutdown2 = shutdown.clone();
    let store2 = store.clone();
    let messenger2 = messenger.clone();
    let queue2 = queue.clone();
    let send_q2 = send_q.clone();
    let out_tx2 = out_tx.clone();
    let base2 = base.clone();
    let run2 = tokio::spawn(async move {
        gateway::Shard::run(
            cfg(),
            messenger2,
            store2,
            queue2,
            send_q2,
            None,
            Some(base2),
            out_tx2,
            shutdown2,
        )
        .await
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        {
            let s = seen.lock().unwrap_or_else(|e| e.into_inner());
            if !s.resumes.is_empty() {
                break;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("client never sent Resume");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    {
        let s = seen.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(s.resumes.len(), 1, "exactly one Resume expected");
        let r = &s.resumes[0];
        assert_eq!(r.get("session_id").and_then(|v| v.as_str()), Some("sess-1"));
        assert_eq!(r.get("seq").and_then(|v| v.as_u64()), Some(2));
    }
    let resumed = recv_timeout(&mut out_rx, 5000).await.expect("RESUMED");
    assert!(
        matches!(resumed, model::Event::Resumed),
        "resume must yield Resumed, got {resumed:?}"
    );
    shutdown.cancel();
    let exit2 = tokio::time::timeout(Duration::from_secs(5), run2)
        .await
        .expect("second run must end on shutdown")
        .expect("join");
    assert_eq!(exit2, gateway::ShardExit::Shutdown);
    let _ = Ordering::Relaxed;
}

#[tokio::test]
async fn invalid_seq_fresh_identifies() {
    let (addr, seen) = mock_gateway::spawn_close_script(4007).await;
    let base = format!("ws://{addr}");
    let store = Arc::new(gateway::MemorySessionStore::new());
    let messenger = messenger_with(0);
    let queue = Arc::new(gateway::InMemoryQueue::new(1));
    let send_q = Arc::new(gateway::SendQueue::new());
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(64);
    let shutdown = tokio_util::sync::CancellationToken::new();

    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        gateway::Shard::run(
            cfg(),
            messenger.clone(),
            store.clone(),
            queue.clone(),
            send_q.clone(),
            None,
            Some(base.clone()),
            out_tx.clone(),
            shutdown.clone(),
        ),
    )
    .await
    .expect("first run must finish on 4007");
    assert_eq!(exit, gateway::ShardExit::FreshIdentify);
    assert!(
        !store.can_resume(),
        "4007 poisons seq: session must be cleared"
    );
    let _ = recv_timeout(&mut out_rx, 2000).await;

    let queue2 = Arc::new(gateway::InMemoryQueue::new(1));
    let shutdown2 = shutdown.clone();
    let run2 = tokio::spawn({
        let store = store.clone();
        let messenger = messenger.clone();
        let send_q = send_q.clone();
        let out_tx = out_tx.clone();
        let base = base.clone();
        async move {
            gateway::Shard::run(
                cfg(),
                messenger,
                store,
                queue2,
                send_q,
                None,
                Some(base),
                out_tx,
                shutdown2,
            )
            .await
        }
    });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        {
            let s = seen.lock().unwrap_or_else(|e| e.into_inner());
            if s.identifies.len() >= 2 {
                break;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("client never re-identified after 4007");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    {
        let s = seen.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(s.identifies.len(), 2, "4007 must re-Identify");
        assert!(
            s.resumes.is_empty(),
            "4007 must NEVER resume: {:?}",
            s.resumes
        );
    }
    shutdown.cancel();
    let exit2 = tokio::time::timeout(Duration::from_secs(5), run2)
        .await
        .expect("second run must end")
        .expect("join");
    assert_eq!(exit2, gateway::ShardExit::Shutdown);
}

#[test]
fn close_code_matrix() {
    for code in [4000u16, 4001, 4002, 4003, 4005, 4009] {
        assert_eq!(gateway::classify(code), gateway::CloseAction::Resume);
    }
    for code in [4006u16, 4007] {
        assert_eq!(gateway::classify(code), gateway::CloseAction::FreshIdentify);
        assert!(!gateway::classify(code).can_resume());
    }
    assert_eq!(gateway::classify(4008), gateway::CloseAction::BackoffResume);
    for code in [4004u16, 4010, 4011, 4012, 4013, 4014] {
        assert!(matches!(
            gateway::classify(code),
            gateway::CloseAction::FailFast { .. }
        ));
    }
    assert_eq!(
        gateway::map_close_to_exit(Some(4007), true),
        gateway::ShardExit::FreshIdentify
    );
    assert_eq!(
        gateway::map_close_to_exit(None, true),
        gateway::ShardExit::BackoffResume
    );
    assert_eq!(
        gateway::map_close_to_exit(None, false),
        gateway::ShardExit::FreshIdentify
    );
}

#[test]
fn latency_helper_tracks_ack() {
    let m = messenger_with(0);
    m.latency.store(12, Ordering::Relaxed);
    assert_eq!(m.latency_ms(), 12);
}
