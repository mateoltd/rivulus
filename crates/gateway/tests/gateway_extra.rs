//! Extra gateway coverage: cluster spawn, shard run branches, pure helpers.
#[path = "../../../tests/support/mock_gateway.rs"]
mod mock_gateway;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use gateway::Queue as _;
use gateway::SessionStore as _;
use serde::Deserialize as _;

type Ws = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

fn cfg() -> gateway::ShardConfig {
    gateway::ShardConfig::new(
        secrecy::SecretString::from(String::from("test-token")),
        model::Intents::GUILDS,
        gateway::ShardId { id: 0, total: 1 },
        50,
    )
}

fn messenger() -> gateway::ShardMessenger {
    gateway::ShardMessenger {
        shard: gateway::ShardId { id: 0, total: 1 },
        latency: Arc::new(AtomicU64::new(0)),
    }
}

fn hello_txt(ms: u64) -> String {
    format!(r#"{{"op":10,"d":{{"heartbeat_interval":{ms}}}}}"#)
}

fn ready_txt(seq: u64, base: &str) -> String {
    format!(
        r#"{{"op":0,"t":"READY","s":{seq},"d":{{"v":10,"session_id":"sess-1","resume_gateway_url":"{base}","user":{{"id":"1","username":"bot","discriminator":"0"}}}}}}"#
    )
}

async fn accept(l: tokio::net::TcpListener) -> Ws {
    let (s, _) = l.accept().await.expect("accept");
    tokio_tungstenite::accept_async(s).await.expect("ws")
}

async fn next_msg(ws: &mut Ws) -> tokio_tungstenite::tungstenite::Message {
    tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("frame in time")
        .expect("stream open")
        .expect("ws ok")
}

fn next_text(ws: &mut Ws) -> impl std::future::Future<Output = String> + '_ {
    async move {
        loop {
            match next_msg(ws).await {
                tokio_tungstenite::tungstenite::Message::Text(s) => return s,
                tokio_tungstenite::tungstenite::Message::Ping(d) => {
                    ws.send(tokio_tungstenite::tungstenite::Message::Pong(d))
                        .await
                        .expect("pong");
                }
                _ => {}
            }
        }
    }
}

async fn run_once(
    base: String,
    st: Arc<gateway::MemorySessionStore>,
    msg: gateway::ShardMessenger,
    limit: Option<model::SessionStartLimit>,
    out: tokio::sync::mpsc::Sender<model::Event>,
    sd: tokio_util::sync::CancellationToken,
) -> gateway::ShardExit {
    tokio::time::timeout(
        Duration::from_secs(10),
        gateway::Shard::run(
            cfg(),
            msg,
            st,
            Arc::new(gateway::InMemoryQueue::new(1)),
            Arc::new(gateway::SendQueue::new()),
            limit,
            Some(base),
            out,
            sd,
        ),
    )
    .await
    .expect("run finishes")
}

async fn recv_ev(rx: &mut tokio::sync::mpsc::Receiver<model::Event>) -> model::Event {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("event in time")
        .expect("channel open")
}

fn compress(payloads: &[&[u8]]) -> Vec<u8> {
    use flate2::{Compress, Compression, FlushCompress};
    let mut c = Compress::new(Compression::default(), true);
    let mut out = Vec::new();
    let mut buf = [0u8; 32768];
    for payload in payloads {
        let mut input: &[u8] = payload;
        // Feed with No-flush until consumed (safety break, never spins:
        // free buffer space always makes progress or we stop loudly).
        while !input.is_empty() {
            let before_in = c.total_in();
            let before_out = c.total_out();
            c.compress(input, &mut buf, FlushCompress::None)
                .expect("compress");
            let consumed = (c.total_in() - before_in) as usize;
            let produced = (c.total_out() - before_out) as usize;
            input = &input[consumed.min(input.len())..];
            out.extend_from_slice(&buf[..produced.min(buf.len())]);
            if consumed == 0 && produced == 0 {
                break;
            }
        }
        // One sync point per payload (like Discord): drain with Sync
        // until a short write. Repeated Sync calls each emit a fresh
        // marker, so "until empty" would never terminate.
        loop {
            let before_out = c.total_out();
            c.compress(&[], &mut buf, FlushCompress::Sync)
                .expect("sync");
            let produced = (c.total_out() - before_out) as usize;
            out.extend_from_slice(&buf[..produced.min(buf.len())]);
            if produced < buf.len() {
                break;
            }
        }
    }
    out
}

// ---- cluster unit ----

#[test]
fn cluster_helpers_and_bookkeeping() {
    assert_eq!(gateway::bucket(20, 16), 4);
    assert_eq!(gateway::bucket(0, 1), 0);
    let ids = gateway::shard_ids(&gateway::ClusterConfig::new(4, 2));
    assert_eq!(ids.len(), 4);
    assert_eq!(ids[0].total, 4);
    let clamped = gateway::ClusterConfig::new(0, 0);
    assert_eq!((clamped.total, clamped.max_concurrency), (1, 1));

    let resp = model::GetGatewayBotResponse {
        url: Box::from("wss://gateway.discord.gg"),
        shards: 3,
        session_start_limit: model::SessionStartLimit {
            total: 1000,
            remaining: 999,
            reset_after: 1,
            max_concurrency: 16,
        },
    };
    assert_eq!(gateway::recommended_shards(&resp), 3);

    let mut c = gateway::Cluster::from_config(&gateway::ClusterConfig::new(2, 1));
    assert_eq!(c.health().len(), 2);
    c.mark_connected(0, 12);
    assert!(c.health().iter().any(|h| h.shard.id == 0 && h.connected));
    c.update(1, 7, Some(9), true);
    let h1 = c
        .health()
        .iter()
        .find(|h| h.shard.id == 1)
        .expect("h1")
        .clone();
    assert_eq!((h1.latency_ms, h1.seq), (7, Some(9)));
    c.restart(0);
    assert!(c.health().iter().any(|h| h.shard.id == 0 && !h.connected));
    // Unknown ids are ignored, never panic.
    c.mark_connected(99, 1);
    c.restart(99);
    c.update(99, 1, None, false);
    let shared = gateway::Cluster::shared(vec![gateway::ShardId { id: 0, total: 1 }]);
    assert_eq!(shared.lock().expect("lock").health().len(), 1);
}

// ---- cluster spawn ----

struct Stub {
    url: String,
    fail: bool,
}

impl gateway::Bootstrap for Stub {
    fn get_gateway_bot(
        &self,
    ) -> futures::future::BoxFuture<'_, Result<model::GetGatewayBotResponse, common::Error>> {
        let url = self.url.clone();
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                return Err(common::Error::Config(Box::from("no gateway")));
            }
            Ok(model::GetGatewayBotResponse {
                url: url.into(),
                shards: 1,
                session_start_limit: model::SessionStartLimit {
                    total: 1000,
                    remaining: 1000,
                    reset_after: 0,
                    max_concurrency: 1,
                },
            })
        })
    }
}

#[tokio::test]
async fn cluster_spawn_identifies_and_tracks_health() {
    let (addr, seen) = mock_gateway::spawn_script().await;
    let base = format!("ws://{addr}");
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (out_tx, _out_rx) = tokio::sync::mpsc::channel(64);
    let auth = gateway::ClusterAuth {
        token: secrecy::SecretString::from(String::from("test-token")),
        intents: model::Intents::GUILDS,
        base_url: Some(base.clone()),
        shutdown: shutdown.clone(),
    };
    let (cluster, handles) = gateway::Cluster::spawn(
        gateway::ClusterConfig::new(1, 1),
        auth,
        Stub {
            url: base,
            fail: false,
        },
        out_tx,
    )
    .await
    .expect("spawn");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        {
            let s = seen.lock().unwrap_or_else(|e| e.into_inner());
            if !s.identifies.is_empty() {
                break;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "cluster never identified"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(cluster.lock().expect("lock").health().len(), 1);
    shutdown.cancel();
    for h in handles {
        tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("join in time")
            .expect("task ok");
    }
}

#[tokio::test]
async fn cluster_spawn_bootstrap_error() {
    let shutdown = tokio_util::sync::CancellationToken::new();
    let (out_tx, _out_rx) = tokio::sync::mpsc::channel(8);
    let auth = gateway::ClusterAuth {
        token: secrecy::SecretString::from(String::from("test-token")),
        intents: model::Intents::GUILDS,
        base_url: None,
        shutdown,
    };
    let dbg = format!("{auth:?}");
    assert!(!dbg.contains("test-token"));
    let err = gateway::Cluster::spawn(
        gateway::ClusterConfig::new(1, 1),
        auth,
        Stub {
            url: String::new(),
            fail: true,
        },
        out_tx,
    )
    .await
    .expect_err("bootstrap fails");
    assert!(matches!(err, common::Error::Config(_)));
}

// ---- shard run branches ----

#[tokio::test]
async fn invalid_session_true_resumes() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let id = next_text(&mut ws).await;
        assert!(id.contains("\"op\":2"), "expected identify: {id}");
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":9,"d":true}"#,
        )))
        .await
        .expect("inv");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        base,
        st.clone(),
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::Resume);
}

#[tokio::test]
async fn invalid_session_false_fresh_identifies_and_clears() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":9,"d":false}"#,
        )))
        .await
        .expect("inv");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    st.set_ready(Box::from("old"), base.clone().into());
    st.set_seq(3);
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        base,
        st.clone(),
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::FreshIdentify);
    assert!(!st.can_resume());
}

#[tokio::test]
async fn reconnect_resumes_with_session_and_identifies_without() {
    // With session: Resume.
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let r = next_text(&mut ws).await;
        assert!(r.contains("\"op\":6"), "expected resume: {r}");
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":7,"d":null}"#,
        )))
        .await
        .expect("reconnect");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    st.set_ready(Box::from("sess-1"), base_srv.clone().into());
    st.set_seq(2);
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        base,
        st,
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::Resume);

    // Without session: FreshIdentify.
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base2 = format!("ws://{}", l2.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l2).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let r = next_text(&mut ws).await;
        assert!(r.contains("\"op\":2"), "expected identify: {r}");
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":7,"d":null}"#,
        )))
        .await
        .expect("reconnect");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st2 = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx2, _rx2) = tokio::sync::mpsc::channel(8);
    let exit2 = run_once(
        base2,
        st2,
        messenger(),
        None,
        out_tx2,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert_eq!(exit2, gateway::ShardExit::FreshIdentify);
}

#[tokio::test]
async fn failfast_close_code_stops() {
    let (addr, _seen) = mock_gateway::spawn_close_script(4014).await;
    let base = format!("ws://{addr}");
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        base,
        st,
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(matches!(exit, gateway::ShardExit::FailFast { .. }));
}

#[tokio::test]
async fn precancelled_shutdown_returns_immediately() {
    let sd = tokio_util::sync::CancellationToken::new();
    sd.cancel();
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        String::from("ws://127.0.0.1:9"),
        st,
        messenger(),
        None,
        out_tx,
        sd,
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::Shutdown);
}

#[tokio::test]
async fn connect_failure_maps_to_fresh_identify() {
    // Guaranteed-closed port: bind then drop.
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = l.local_addr().expect("addr");
    drop(l);
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        format!("ws://{addr}"),
        st,
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::FreshIdentify);
}

#[tokio::test]
async fn close_before_hello_maps_code() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Close(Some(
            tokio_tungstenite::tungstenite::protocol::CloseFrame {
                code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(
                    4007u16,
                ),
                reason: std::borrow::Cow::Borrowed("bad seq"),
            },
        )))
        .await
        .expect("close");
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let exit = run_once(
        base,
        st.clone(),
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::FreshIdentify);
    assert!(!st.can_resume());
}

#[tokio::test]
async fn reset_after_sleeps_then_connects() {
    let (addr, _seen) = mock_gateway::spawn_script().await;
    let base = format!("ws://{addr}");
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(64);
    let limit = model::SessionStartLimit {
        total: 1000,
        remaining: 0,
        reset_after: 50,
        max_concurrency: 1,
    };
    let start = tokio::time::Instant::now();
    let exit = run_once(
        base,
        st,
        messenger(),
        Some(limit),
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(
        start.elapsed() >= Duration::from_millis(40),
        "reset_after not slept"
    );
    assert!(
        matches!(
            exit,
            gateway::ShardExit::Resume | gateway::ShardExit::BackoffResume
        ),
        "got {exit:?}"
    );
}

#[tokio::test]
async fn reset_after_cancel_returns_shutdown() {
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let sd = tokio_util::sync::CancellationToken::new();
    let sd_c = sd.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        sd_c.cancel();
    });
    let limit = model::SessionStartLimit {
        total: 1000,
        remaining: 0,
        reset_after: 500,
        max_concurrency: 1,
    };
    let exit = run_once(
        String::from("ws://127.0.0.1:9"),
        st,
        messenger(),
        Some(limit),
        out_tx,
        sd,
    )
    .await;
    assert_eq!(exit, gateway::ShardExit::Shutdown);
}

#[tokio::test]
async fn missed_acks_close_and_resume() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(50)))
            .await
            .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ready_txt(
            1, &base_srv,
        )))
        .await
        .expect("ready");
        // Never ack: read heartbeats and ignore them.
        let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        loop {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            let _ = tokio::time::timeout(Duration::from_millis(100), ws.next()).await;
        }
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(64);
    let exit = run_once(
        base,
        st.clone(),
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(exit.can_resume(), "missed acks must resume, got {exit:?}");
    assert_eq!(st.snapshot().seq, Some(1));
}

#[tokio::test]
async fn heartbeat_op1_acked_updates_latency_and_seq() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ready_txt(
            1, &base_srv,
        )))
        .await
        .expect("ready");
        // Ask the client to heartbeat right now; expect an op1 back.
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":1,"d":null}"#,
        )))
        .await
        .expect("hb req");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        let mut seen = 0;
        while tokio::time::Instant::now() < deadline && seen < 2 {
            let m = next_msg(&mut ws).await;
            if let tokio_tungstenite::tungstenite::Message::Text(s) = m {
                if s.contains("\"op\":1") {
                    seen += 1;
                    // Ack every heartbeat. The first ack may land before any
                    // periodic heartbeat armed `last_sent`; the second ack
                    // always follows a periodic heartbeat, so latency stores.
                    ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
                        r#"{"op":11,"d":null}"#,
                    )))
                    .await
                    .expect("ack");
                }
            }
        }
        assert!(seen >= 2, "client must heartbeat (request + periodic)");
        // Bump seq via dispatch.
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":0,"t":"SOME_FUTURE_KIND","s":5,"d":{}}"#,
        )))
        .await
        .expect("dispatch");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let msg = messenger();
    msg.latency.store(999_999, Ordering::Relaxed);
    let (out_tx, mut rx) = tokio::sync::mpsc::channel(64);
    let st_c = st.clone();
    let msg_c = msg.clone();
    let run = tokio::spawn(async move {
        run_once(
            base,
            st_c,
            msg_c,
            None,
            out_tx,
            tokio_util::sync::CancellationToken::new(),
        )
        .await
    });
    let first = recv_ev(&mut rx).await;
    assert!(matches!(first, model::Event::Ready(_)), "got {first:?}");
    let second = recv_ev(&mut rx).await;
    assert!(
        matches!(second, model::Event::Unknown { .. }),
        "got {second:?}"
    );
    // Seq persisted before fan-out: visible even while the run continues.
    assert_eq!(st.snapshot().seq, Some(5));
    let exit = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("join in time")
        .expect("task ok");
    assert!(
        exit.can_resume(),
        "drop with session must resume, got {exit:?}"
    );
    assert!(msg.latency_ms() < 1000, "ack must store latency");
}

#[tokio::test]
async fn unknown_dispatch_capped() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    let big = "a".repeat(9000);
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ready_txt(
            1, &base_srv,
        )))
        .await
        .expect("ready");
        let ev = format!(r#"{{"op":0,"t":"FUTURE_X","s":2,"d":{{"blob":"{big}"}}}}"#);
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ev))
            .await
            .expect("dispatch");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, mut rx) = tokio::sync::mpsc::channel(64);
    let run = tokio::spawn({
        let st = st.clone();
        async move {
            run_once(
                base,
                st,
                messenger(),
                None,
                out_tx,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
        }
    });
    assert!(matches!(recv_ev(&mut rx).await, model::Event::Ready(_)));
    let ev = recv_ev(&mut rx).await;
    if let model::Event::Unknown { kind, seq, payload } = ev {
        assert_eq!(&*kind, "FUTURE_X");
        assert_eq!(seq, Some(2));
        assert!(payload.len() <= 8192, "payload must cap: {}", payload.len());
    } else {
        panic!("expected Unknown, got different event");
    }
    let exit = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("join in time")
        .expect("task ok");
    assert!(exit.can_resume());
}

#[tokio::test]
async fn ready_parse_failure_surfaces_unknown() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":0,"t":"READY","s":1,"d":{"bogus":true}}"#,
        )))
        .await
        .expect("bad ready");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, mut rx) = tokio::sync::mpsc::channel(64);
    let run = tokio::spawn({
        let st = st.clone();
        async move {
            run_once(
                base,
                st,
                messenger(),
                None,
                out_tx,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
        }
    });
    let ev = recv_ev(&mut rx).await;
    assert!(
        matches!(ev, model::Event::Unknown { .. }),
        "bad READY must surface as Unknown, got {ev:?}"
    );
    let _ = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("join in time");
}

#[tokio::test]
async fn voice_branches_forward_or_unknown() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ready_txt(
            1, &base_srv,
        )))
        .await
        .expect("ready");
        for frame in [
            r#"{"op":0,"t":"VOICE_STATE_UPDATE","s":2,"d":{"user_id":"5"}}"#,
            r#"{"op":0,"t":"VOICE_STATE_UPDATE","s":3,"d":{"nope":true}}"#,
            r#"{"op":0,"t":"VOICE_SERVER_UPDATE","s":4,"d":{"token":"t","guild_id":"6"}}"#,
            r#"{"op":0,"t":"VOICE_SERVER_UPDATE","s":5,"d":{"nope":true}}"#,
        ] {
            ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
                frame,
            )))
            .await
            .expect("frame");
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, mut rx) = tokio::sync::mpsc::channel(64);
    let run = tokio::spawn({
        let st = st.clone();
        async move {
            run_once(
                base,
                st,
                messenger(),
                None,
                out_tx,
                tokio_util::sync::CancellationToken::new(),
            )
            .await
        }
    });
    assert!(matches!(recv_ev(&mut rx).await, model::Event::Ready(_)));
    assert!(matches!(
        recv_ev(&mut rx).await,
        model::Event::VoiceStateUpdate(_)
    ));
    assert!(matches!(
        recv_ev(&mut rx).await,
        model::Event::Unknown { .. }
    ));
    assert!(matches!(
        recv_ev(&mut rx).await,
        model::Event::VoiceServerUpdate(_)
    ));
    assert!(matches!(
        recv_ev(&mut rx).await,
        model::Event::Unknown { .. }
    ));
    let exit = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("join in time")
        .expect("task ok");
    assert!(exit.can_resume());
    assert_eq!(st.snapshot().seq, Some(5));
}

#[tokio::test]
async fn mid_hello_unknown_op_and_garbage_ignored() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":11,"d":null}"#,
        )))
        .await
        .expect("early ack-ish");
        ws.send(tokio_tungstenite::tungstenite::Message::Ping(vec![9u8]))
            .await
            .expect("ping");
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ready_txt(
            1, &base_srv,
        )))
        .await
        .expect("ready");
        // Mid-connection Hello (ignored), unknown opcode (ignored), garbage (ignored).
        for frame in [
            r#"{"op":10,"d":{"heartbeat_interval":100}}"#,
            r#"{"op":5,"d":{}}"#,
            r#"{{{not json"#,
        ] {
            ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
                frame,
            )))
            .await
            .expect("frame");
        }
        // Early op11 with no heartbeat sent yet: latency untouched path.
        ws.send(tokio_tungstenite::tungstenite::Message::Text(String::from(
            r#"{"op":11,"d":null}"#,
        )))
        .await
        .expect("ack");
        ws.send(tokio_tungstenite::tungstenite::Message::Pong(vec![1u8]))
            .await
            .expect("pong");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, mut rx) = tokio::sync::mpsc::channel(64);
    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        run_once(
            base,
            st.clone(),
            messenger(),
            None,
            out_tx,
            tokio_util::sync::CancellationToken::new(),
        ),
    )
    .await
    .expect("run finishes");
    let first = recv_ev(&mut rx).await;
    assert!(matches!(first, model::Event::Ready(_)), "got {first:?}");
    assert!(exit.can_resume(), "got {exit:?}");
    assert_eq!(st.snapshot().seq, Some(1));
}

#[tokio::test]
async fn oversize_text_disconnects() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(ready_txt(
            1, &base_srv,
        )))
        .await
        .expect("ready");
        let big = "x".repeat(gateway::MESSAGE_CAP + 1);
        let _ = ws
            .send(tokio_tungstenite::tungstenite::Message::Text(big))
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(64);
    let exit = run_once(
        base,
        st,
        messenger(),
        None,
        out_tx,
        tokio_util::sync::CancellationToken::new(),
    )
    .await;
    assert!(!matches!(exit, gateway::ShardExit::Shutdown));
}

#[tokio::test]
async fn zlib_binary_frames_decode() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    let base_srv = base.clone();
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        // Hello + Ready as ONE zlib stream (two sync flushes), like Discord.
        let hello = hello_txt(100);
        let ready = ready_txt(1, &base_srv);
        let combined = compress(&[hello.as_bytes(), ready.as_bytes()]);
        let cut = combined
            .windows(4)
            .position(|w| w == gateway::ZSYNC_FLUSH)
            .map(|p| p + 4)
            .expect("flush suffix");
        let (hello_part, ready_part) = combined.split_at(cut);
        // Hello split across two frames (suffix straddles the boundary).
        let split = hello_part.len() / 2;
        ws.send(tokio_tungstenite::tungstenite::Message::Binary(
            hello_part[..split].to_vec(),
        ))
        .await
        .expect("hello-a");
        ws.send(tokio_tungstenite::tungstenite::Message::Binary(
            hello_part[split..].to_vec(),
        ))
        .await
        .expect("hello-b");
        let _ = next_text(&mut ws).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Binary(
            ready_part.to_vec(),
        ))
        .await
        .expect("ready");
        tokio::time::sleep(Duration::from_millis(300)).await;
    });
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, mut rx) = tokio::sync::mpsc::channel(64);
    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        run_once(
            base,
            st.clone(),
            messenger(),
            None,
            out_tx,
            tokio_util::sync::CancellationToken::new(),
        ),
    )
    .await
    .expect("run finishes");
    let first = recv_ev(&mut rx).await;
    assert!(matches!(first, model::Event::Ready(_)), "got {first:?}");
    assert_eq!(st.snapshot().session_id.as_deref(), Some("sess-1"));
    assert!(exit.can_resume(), "got {exit:?}");
}

#[tokio::test]
async fn shutdown_during_identify_queue_returns_shutdown() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        tokio::time::sleep(Duration::from_millis(800)).await;
    });
    // Occupy the bucket so the run blocks in `enqueue` (5s pacing).
    let queue = Arc::new(gateway::InMemoryQueue::new(1));
    queue.enqueue(0).await;
    let st = Arc::new(gateway::MemorySessionStore::new());
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let sd = tokio_util::sync::CancellationToken::new();
    let sd_c = sd.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        sd_c.cancel();
    });
    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        gateway::Shard::run(
            cfg(),
            messenger(),
            st,
            queue,
            Arc::new(gateway::SendQueue::new()),
            None,
            Some(base),
            out_tx,
            sd,
        ),
    )
    .await
    .expect("run finishes");
    assert_eq!(exit, gateway::ShardExit::Shutdown);
}

#[tokio::test]
async fn shutdown_during_resume_acquire_returns_shutdown() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let base = format!("ws://{}", l.local_addr().expect("addr"));
    tokio::spawn(async move {
        let mut ws = accept(l).await;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(hello_txt(
            100,
        )))
        .await
        .expect("hello");
        tokio::time::sleep(Duration::from_millis(800)).await;
    });
    // Fill the send queue so `acquire` blocks; cancel mid-wait.
    let send_q = Arc::new(gateway::SendQueue::new());
    for _ in 0..120 {
        send_q.wait_for_command();
    }
    let st = Arc::new(gateway::MemorySessionStore::new());
    st.set_ready(Box::from("sess-1"), base.clone().into());
    st.set_seq(2);
    let (out_tx, _rx) = tokio::sync::mpsc::channel(8);
    let sd = tokio_util::sync::CancellationToken::new();
    let sd_c = sd.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        sd_c.cancel();
    });
    let exit = tokio::time::timeout(
        Duration::from_secs(10),
        gateway::Shard::run(
            cfg(),
            messenger(),
            st,
            Arc::new(gateway::InMemoryQueue::new(1)),
            send_q,
            None,
            Some(base),
            out_tx,
            sd,
        ),
    )
    .await
    .expect("run finishes");
    assert_eq!(exit, gateway::ShardExit::Shutdown);
}

// ---- pure unit extras ----

#[test]
fn payload_getters_and_shapes() {
    let c = gateway::ShardConfig::new(
        secrecy::SecretString::from(String::from("tok")),
        model::Intents::GUILDS,
        gateway::ShardId { id: 1, total: 4 },
        0,
    );
    assert_eq!(c.large_threshold, 50);
    let c2 = gateway::ShardConfig::new(
        secrecy::SecretString::from(String::from("tok")),
        model::Intents::GUILDS,
        gateway::ShardId { id: 1, total: 4 },
        255,
    );
    assert_eq!(c2.large_threshold, 250);
    let p = gateway::IdentifyPayload::build(&c);
    assert_eq!(p.shard(), [1, 4]);
    assert_eq!(p.intents(), model::Intents::GUILDS);
    let r = gateway::ResumePayload::new(
        secrecy::SecretString::from(String::from("t")),
        Box::from("sess"),
        9,
    );
    assert_eq!(r.session_id(), "sess");
    assert_eq!(r.seq(), 9);
    assert_eq!(gateway::shard_for_guild(1 << 22, 4), 1);
    assert_eq!(gateway::shard_for_guild(0, 1), 0);
}

#[test]
fn lifecycle_and_exit_helpers() {
    let id = gateway::ShardId { id: 2, total: 4 };
    let ready = gateway::ShardEvent::Ready(id);
    let resumed = gateway::ShardEvent::Resumed(id);
    let disc = gateway::ShardEvent::Disconnected {
        shard: id,
        code: Some(4000),
    };
    assert!(matches!(ready, gateway::ShardEvent::Ready(_)));
    assert!(matches!(resumed, gateway::ShardEvent::Resumed(_)));
    assert!(matches!(disc, gateway::ShardEvent::Disconnected { .. }));
    assert!(gateway::ShardExit::Resume.can_resume());
    assert!(gateway::ShardExit::BackoffResume.can_resume());
    assert!(!gateway::ShardExit::FreshIdentify.can_resume());
    assert!(!gateway::ShardExit::Shutdown.can_resume());
    assert!(matches!(
        gateway::ShardExit::from_action(gateway::CloseAction::Resume),
        gateway::ShardExit::Resume
    ));
    assert!(matches!(
        gateway::ShardExit::from_action(gateway::CloseAction::FreshIdentify),
        gateway::ShardExit::FreshIdentify
    ));
    assert!(matches!(
        gateway::ShardExit::from_action(gateway::CloseAction::BackoffResume),
        gateway::ShardExit::BackoffResume
    ));
    assert!(matches!(
        gateway::ShardExit::from_action(gateway::CloseAction::FailFast { help: "x" }),
        gateway::ShardExit::FailFast { .. }
    ));
    assert_eq!(
        gateway::invalid_session_exit(true),
        gateway::ShardExit::Resume
    );
    let _ = gateway::ShardStrategy::Auto;
    let _ = gateway::ShardStrategy::Single;
    let _ = gateway::ShardStrategy::Manual;
    assert!((0.0..1.0).contains(&gateway::random_fract()));
    let raw = r#"{"nope":1}"#;
    let boxed: Box<serde_json::value::RawValue> = serde_json::from_str(raw).expect("raw");
    assert_eq!(gateway::parse_hello_interval(&boxed), None);
}

#[test]
fn chunk_assembler_len_and_empty_nonce_expiry() {
    use std::time::Instant;
    let mut asm = gateway::ChunkAssembler::new();
    assert!(asm.is_empty());
    assert_eq!(asm.len(), 0);
    let guild = model::GuildId::new(5).expect("guild");
    let c0 = model::GuildMembersChunk {
        guild_id: guild,
        members: Vec::new(),
        chunk_index: 0,
        chunk_count: 2,
        nonce: None,
        not_found: Vec::new(),
    };
    let now = Instant::now();
    assert!(asm.insert(c0, now).is_none());
    assert_eq!(asm.len(), 1);
    let expired = asm.expire(now + Duration::from_secs(11));
    assert_eq!(expired.len(), 1);
    assert!(expired[0].nonce.is_none());
    assert!(asm.is_empty());
}

#[test]
fn close_help_arms() {
    assert!(!gateway::classify(4000).help().is_empty());
    assert!(!gateway::classify(4007).help().is_empty());
    assert!(!gateway::classify(4008).help().is_empty());
    assert!(!gateway::classify(4014).help().is_empty());
    assert!(gateway::classify(4000).can_resume());
    assert!(!gateway::classify(4007).can_resume());
    assert!(gateway::classify(4008).can_resume());
    assert!(!gateway::classify(4014).can_resume());
}

#[test]
fn opcode_full_tolerant_surface() {
    let op: gateway::Opcode = 7u8.into();
    assert_eq!(op, gateway::Opcode::Reconnect);
    let n: u8 = gateway::Opcode::Hello.into();
    assert_eq!(n, 10);
    let via_u8 = gateway::Opcode::deserialize(serde::de::value::U8Deserializer::<
        serde::de::value::Error,
    >::new(2))
    .expect("u8 de");
    assert_eq!(via_u8, gateway::Opcode::Identify);
    let via_u16 = gateway::Opcode::deserialize(serde::de::value::U16Deserializer::<
        serde::de::value::Error,
    >::new(11))
    .expect("u16 de");
    assert_eq!(via_u16, gateway::Opcode::HeartbeatAck);
    let via_u32 = gateway::Opcode::deserialize(serde::de::value::U32Deserializer::<
        serde::de::value::Error,
    >::new(9))
    .expect("u32 de");
    assert_eq!(via_u32, gateway::Opcode::InvalidSession);
    let err =
        serde_json::from_value::<gateway::Opcode>(serde_json::Value::String(String::from("x")))
            .expect_err("string opcode fails");
    assert!(err.to_string().contains("gateway opcode"), "got {err}");
    let neg = serde_json::from_value::<gateway::Opcode>(serde_json::Value::from(-1))
        .expect_err("negative opcode fails");
    assert!(!neg.to_string().is_empty());
    let big = serde_json::from_value::<gateway::Opcode>(serde_json::Value::from(300))
        .expect_err("out-of-range opcode fails");
    assert!(!big.to_string().is_empty());
}

#[test]
fn compression_extra_caps() {
    // Suffix-only garbage: invalid zlib header must error and reset.
    let mut z = gateway::ZlibStream::new();
    assert!(z.feed(&[0x00, 0x00, 0xFF, 0xFF]).is_err());
    // Inflated output beyond MESSAGE_CAP errors.
    let big = vec![0u8; gateway::MESSAGE_CAP + 1024];
    let compressed = compress(&[big.as_slice()]);
    let mut z2 = gateway::ZlibStream::new();
    assert!(z2.feed(&compressed).is_err());
    z2.reset();
    // Trailing bytes without a suffix buffer harmlessly.
    let mut z3 = gateway::ZlibStream::new();
    assert!(z3.feed(&[1u8, 2u8, 3u8]).expect("buffer").is_empty());
    z3.reset();
}

#[tokio::test]
async fn queue_acquire_paths() {
    let q = gateway::SendQueue::new();
    q.acquire(false).await;
    q.acquire(true).await;
    assert_eq!(gateway::InMemoryQueue::new(0).key(3), 0);
    let iq = gateway::InMemoryQueue::new(4);
    iq.enqueue(2).await;
    assert_eq!(iq.key(6), 2);
    assert_eq!(q.wait_for_command(), 0.0);
}
