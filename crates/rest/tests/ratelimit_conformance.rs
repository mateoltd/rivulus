//! P2 acceptance: mock server bucket exhaust -> pre-emptive wait + 429 honored; 401/403 never retried.
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn spawn_script(script: Vec<String>, hits: Arc<AtomicUsize>) -> SocketAddr {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let a = l.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                break;
            };
            let i = hits.fetch_add(1, Ordering::SeqCst);
            let raw = script.get(i).cloned().unwrap_or_else(|| {
                String::from("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            });
            tokio::spawn(async move {
                // Read request headers (and body) before responding.
                let mut buf: Vec<u8> = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    let Ok(n) = s.read(&mut tmp).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    if buf.len() > 65536 {
                        break;
                    }
                }
                let _ = s.write_all(raw.as_bytes()).await;
            });
        }
    });
    a
}

fn resp(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut h = format!("{status}\r\n");
    for (k, v) in headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    ));
    h
}

fn test_client(addr: &SocketAddr) -> rest::Client {
    let base = format!("http://{addr}");
    rest::Client::builder(secrecy::SecretString::from(String::from("test-token")))
        .base_url(base)
        .build()
        .unwrap()
}

#[tokio::test]
async fn bucket_key_major_isolation() {
    let a = rest::Route::CreateMessage { channel_id: 1 };
    let b = rest::Route::CreateMessage { channel_id: 2 };
    assert_ne!(
        rest::Ratelimiter::bucket_key(&a),
        rest::Ratelimiter::bucket_key(&b)
    );
    assert_eq!(a.path_template(), b.path_template());
    // `url_with_base` points at mocks without touching discord.com.
    let addr: SocketAddr = "127.0.0.1:9".parse().unwrap();
    let _ = addr;
    let mocked = a.url_with_base("http://127.0.0.1:1234");
    assert!(mocked.starts_with("http://127.0.0.1:1234/channels/"));
    assert!(a.url().starts_with("https://discord.com"));
}

#[tokio::test]
async fn preemptive_wait_honored() {
    // remaining=0 with reset_after=0.2 must delay second call >=0.15s and never 429.
    let ok = resp(
        "HTTP/1.1 200 OK",
        &[
            ("X-RateLimit-Limit", "1"),
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset-After", "0.2"),
            ("Content-Type", "application/json"),
        ],
        "{}",
    );
    let ok2 = resp(
        "HTTP/1.1 200 OK",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![ok, ok2], hits.clone()).await;
    let client = test_client(&addr);
    let route = rest::Route::GetChannel { channel_id: 1 };
    client.execute(&route, None, None).await.unwrap();
    let start = Instant::now();
    client.execute(&route, None, None).await.unwrap();
    assert!(
        start.elapsed() >= Duration::from_millis(150),
        "pre-emptive wait not honored: {:?}",
        start.elapsed()
    );
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn ratelimit_429_then_200() {
    let r429 = resp(
        "HTTP/1.1 429 Too Many Requests",
        &[
            ("X-RateLimit-Limit", "5"),
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset-After", "0.1"),
            ("X-RateLimit-Scope", "user"),
            ("Content-Type", "application/json"),
        ],
        r#"{"retry_after":0.1,"global":false}"#,
    );
    let ok = resp(
        "HTTP/1.1 200 OK",
        &[
            ("X-RateLimit-Limit", "5"),
            ("X-RateLimit-Remaining", "4"),
            ("X-RateLimit-Reset-After", "0.0"),
            ("Content-Type", "application/json"),
        ],
        "{}",
    );
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![r429, ok], hits.clone()).await;
    let client = test_client(&addr);
    let route = rest::Route::GetChannel { channel_id: 44 };
    let start = Instant::now();
    let out = client.execute(&route, None, None).await.unwrap();
    assert_eq!(out, b"{}");
    assert!(
        start.elapsed() >= Duration::from_millis(80),
        "429 retry_after not honored: {:?}",
        start.elapsed()
    );
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn unauthorized_is_fail_fast() {
    let u401 = resp(
        "HTTP/1.1 401 Unauthorized",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![u401], hits.clone()).await;
    let client = test_client(&addr);
    let route = rest::Route::GetChannel { channel_id: 7 };
    let err = client.execute(&route, None, None).await.unwrap_err();
    assert!(matches!(err, common::Error::Unauthorized));
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let f403 = resp(
        "HTTP/1.1 403 Forbidden",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let hits2 = Arc::new(AtomicUsize::new(0));
    let addr2 = spawn_script(vec![f403], hits2.clone()).await;
    let client2 = test_client(&addr2);
    let err2 = client2.execute(&route, None, None).await.unwrap_err();
    assert!(matches!(err2, common::Error::Forbidden));
    assert_eq!(hits2.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn shared_scope_excluded_from_ban_meter() {
    // Pure meter: 9k shared-scope 429s never count.
    let mut meter = rest::BanMeter::new();
    for _ in 0..9000 {
        meter.record(429, "shared");
    }
    let (warn, err) = meter.record(429, "user");
    assert!(!warn && !err);

    // Over HTTP: a 429 with `Scope: shared` body still retries once then 200s.
    let r429 = resp(
        "HTTP/1.1 429 Too Many Requests",
        &[
            ("X-RateLimit-Scope", "shared"),
            ("Content-Type", "application/json"),
        ],
        r#"{"retry_after":0.01,"global":false}"#,
    );
    let ok = resp(
        "HTTP/1.1 200 OK",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![r429, ok], hits.clone()).await;
    let client = test_client(&addr);
    let route = rest::Route::GetChannel { channel_id: 99 };
    client.execute(&route, None, None).await.unwrap();
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn webhook_and_multipart_hit_mock() {
    let ok = resp(
        "HTTP/1.1 200 OK",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let ok2 = ok.clone();
    let hits = Arc::new(AtomicUsize::new(0));
    let addr = spawn_script(vec![ok, ok2], hits.clone()).await;
    let client = test_client(&addr);

    // Webhook exec with secret URL never logs the token: 200 OK.
    let secret = secrecy::SecretString::from(format!("http://{addr}/webhooks/1/abc"));
    let out = client
        .execute_webhook(&secret, None, Some(b"{}".to_vec()), None)
        .await
        .unwrap();
    assert_eq!(out, b"{}");

    // Multipart message create against the same mock.
    let route = rest::Route::CreateMessage { channel_id: 5 };
    let file = rest::multipart::MultipartFile::new(
        Box::from("a.txt"),
        String::from("text/plain"),
        b"hi".to_vec(),
        None,
    );
    let out2 = client
        .execute_multipart(&route, b"{}", &[file], None)
        .await
        .unwrap();
    assert_eq!(out2, b"{}");
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}
