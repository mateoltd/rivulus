//! Scripted mock gateway (WS server for `gateway_resume` tests).
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::{SinkExt, StreamExt};

/// Observed client frames (assertion hook).
#[derive(Debug, Default)]
pub struct Seen {
    /// HTTP request paths (must contain `compress=zlib-stream`).
    pub paths: Vec<String>,
    /// Raw Identify `d` payloads (`op2`).
    pub identifies: Vec<serde_json::Value>,
    /// Raw Resume `d` payloads (`op6`).
    pub resumes: Vec<serde_json::Value>,
    /// Heartbeat count (`op1`).
    pub heartbeats: usize,
}

/// Spawn a scripted server (accept then script); keeps `spawn()` shape.
pub async fn spawn() -> SocketAddr {
    let (addr, _) = spawn_script().await;
    addr
}

/// Spawn the drop-then-resume script.
///
/// Conn 0: Hello(100ms) -> expect Identify -> Ready(sess-1) + one
/// Dispatch(s=2) -> DROP. Conn 1+: Hello -> expect Resume(sess-1, seq=2)
/// -> RESUMED, then Ack heartbeats. Returns the addr plus the shared
/// assertion hook.
pub async fn spawn_script() -> (SocketAddr, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = l.local_addr().expect("addr");
    let resume_url = format!("ws://{addr}/");
    let hits = Arc::new(AtomicUsize::new(0));
    let seen_task = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = l.accept().await else {
                break;
            };
            let seen_c = seen_task.clone();
            let hits_c = hits.clone();
            let resume_c = resume_url.clone();
            tokio::spawn(async move {
                serve_script(stream, seen_c, hits_c, resume_c).await;
            });
        }
    });
    (addr, seen)
}

async fn serve_script(
    stream: tokio::net::TcpStream,
    seen: Arc<Mutex<Seen>>,
    hits: Arc<AtomicUsize>,
    resume_url: String,
) {
    let idx = hits.fetch_add(1, Ordering::SeqCst);
    let seen_h = seen.clone();
    let ws = match tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         res: tokio_tungstenite::tungstenite::handshake::server::Response| {
            if let Ok(mut s) = seen_h.lock() {
                s.paths.push(req.uri().to_string());
            }
            Ok(res)
        },
    )
    .await
    {
        Ok(w) => w,
        Err(_) => return,
    };
    let (mut sink, mut stream) = ws.split();
    let hello = serde_json::json!({"op": 10, "d": {"heartbeat_interval": 100}});
    let hello_txt = serde_json::to_string(&hello).unwrap_or_default();
    if sink
        .send(tokio_tungstenite::tungstenite::Message::Text(hello_txt))
        .await
        .is_err()
    {
        return;
    }
    let mut drop_at: Option<tokio::time::Instant> = None;
    loop {
        let sleep_fut = async {
            match drop_at {
                Some(t) => tokio::time::sleep_until(t).await,
                None => futures::future::pending::<()>().await,
            }
        };
        tokio::select! {
            _ = sleep_fut => {
                return;
            }
            msg = stream.next() => {
                let Some(msg) = msg else { return; };
                let Ok(msg) = msg else { return; };
                match msg {
                    tokio_tungstenite::tungstenite::Message::Text(s) => {
                        let v: serde_json::Value = match serde_json::from_str(&s) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(99);
                        if op == 1 {
                            if let Ok(mut sl) = seen.lock() {
                                sl.heartbeats += 1;
                            }
                            let ack = serde_json::json!({"op": 11, "d": null});
                            let txt = serde_json::to_string(&ack).unwrap_or_default();
                            let _ = sink.send(tokio_tungstenite::tungstenite::Message::Text(txt)).await;
                        } else if op == 2 {
                            if let Ok(mut sl) = seen.lock() {
                                sl.identifies.push(v.get("d").cloned().unwrap_or(serde_json::Value::Null));
                            }
                            if idx == 0 {
                                send_ready(&mut sink, &resume_url).await;
                                send_dispatch(&mut sink).await;
                                drop_at = Some(tokio::time::Instant::now() + std::time::Duration::from_millis(400));
                            } else {
                                send_ready(&mut sink, &resume_url).await;
                            }
                        } else if op == 6 {
                            if let Ok(mut sl) = seen.lock() {
                                sl.resumes.push(v.get("d").cloned().unwrap_or(serde_json::Value::Null));
                            }
                            let resumed = serde_json::json!({"op": 0, "t": "RESUMED", "s": 2, "d": {}});
                            let txt = serde_json::to_string(&resumed).unwrap_or_default();
                            let _ = sink.send(tokio_tungstenite::tungstenite::Message::Text(txt)).await;
                        }
                    }
                    tokio_tungstenite::tungstenite::Message::Ping(d) => {
                        let _ = sink.send(tokio_tungstenite::tungstenite::Message::Pong(d)).await;
                    }
                    tokio_tungstenite::tungstenite::Message::Close(_) => return,
                    _ => {}
                }
            }
        }
    }
}

async fn send_ready(
    sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        tokio_tungstenite::tungstenite::Message,
    >,
    resume_url: &str,
) {
    let ready = serde_json::json!({
        "op": 0, "t": "READY", "s": 1,
        "d": {
            "v": 10,
            "session_id": "sess-1",
            "resume_gateway_url": resume_url,
            "user": {"id": "1", "username": "bot", "discriminator": "0"}
        }
    });
    let txt = serde_json::to_string(&ready).unwrap_or_default();
    let _ = sink
        .send(tokio_tungstenite::tungstenite::Message::Text(txt))
        .await;
}

async fn send_dispatch(
    sink: &mut futures::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
        tokio_tungstenite::tungstenite::Message,
    >,
) {
    let ev = serde_json::json!({
        "op": 0, "t": "SOME_FUTURE_KIND", "s": 2, "d": {"note": "hello"}
    });
    let txt = serde_json::to_string(&ev).unwrap_or_default();
    let _ = sink
        .send(tokio_tungstenite::tungstenite::Message::Text(txt))
        .await;
}

/// Spawn a close-code script (e.g. 4007).
///
/// Conn 0: Hello -> Identify -> Ready -> close(`code`). Conn 1+:
/// Hello -> record Identify/Resume -> Ready, then Ack. Used to prove
/// `4007` reconnects with fresh Identify (never Resume).
pub async fn spawn_close_script(code: u16) -> (SocketAddr, Arc<Mutex<Seen>>) {
    let seen = Arc::new(Mutex::new(Seen::default()));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = l.local_addr().expect("addr");
    let resume_url = format!("ws://{addr}/");
    let seen_task = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = l.accept().await else {
                break;
            };
            let seen_c = seen_task.clone();
            let resume_c = resume_url.clone();
            tokio::spawn(async move {
                serve_close(stream, seen_c, resume_c, code).await;
            });
        }
    });
    (addr, seen)
}

async fn serve_close(
    stream: tokio::net::TcpStream,
    seen: Arc<Mutex<Seen>>,
    resume_url: String,
    code: u16,
) {
    let seen_h = seen.clone();
    let ws = match tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         res: tokio_tungstenite::tungstenite::handshake::server::Response| {
            if let Ok(mut s) = seen_h.lock() {
                s.paths.push(req.uri().to_string());
            }
            Ok(res)
        },
    )
    .await
    {
        Ok(w) => w,
        Err(_) => return,
    };
    let (mut sink, mut stream) = ws.split();
    let hello = serde_json::json!({"op": 10, "d": {"heartbeat_interval": 100}});
    let txt = serde_json::to_string(&hello).unwrap_or_default();
    if sink
        .send(tokio_tungstenite::tungstenite::Message::Text(txt))
        .await
        .is_err()
    {
        return;
    }
    let mut conns: usize = 0;
    if let Ok(s) = seen.lock() {
        conns = s.identifies.len() + s.resumes.len();
    }
    let first = conns == 0;
    loop {
        let Some(msg) = stream.next().await else {
            return;
        };
        let Ok(msg) = msg else {
            return;
        };
        match msg {
            tokio_tungstenite::tungstenite::Message::Text(s) => {
                let v: serde_json::Value = match serde_json::from_str(&s) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let op = v.get("op").and_then(|o| o.as_u64()).unwrap_or(99);
                if op == 1 {
                    if let Ok(mut sl) = seen.lock() {
                        sl.heartbeats += 1;
                    }
                    let ack = serde_json::json!({"op": 11, "d": null});
                    let txt = serde_json::to_string(&ack).unwrap_or_default();
                    let _ = sink
                        .send(tokio_tungstenite::tungstenite::Message::Text(txt))
                        .await;
                } else if op == 2 {
                    if let Ok(mut sl) = seen.lock() {
                        sl.identifies
                            .push(v.get("d").cloned().unwrap_or(serde_json::Value::Null));
                    }
                    send_ready(&mut sink, &resume_url).await;
                    if first {
                        use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
                        use tokio_tungstenite::tungstenite::protocol::CloseFrame;
                        let frame = CloseFrame {
                            code: CloseCode::from(code),
                            reason: std::borrow::Cow::Borrowed("test close"),
                        };
                        let _ = sink
                            .send(tokio_tungstenite::tungstenite::Message::Close(Some(frame)))
                            .await;
                        return;
                    }
                } else if op == 6 {
                    if let Ok(mut sl) = seen.lock() {
                        sl.resumes
                            .push(v.get("d").cloned().unwrap_or(serde_json::Value::Null));
                    }
                    send_ready(&mut sink, &resume_url).await;
                }
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => return,
            _ => {}
        }
    }
}
