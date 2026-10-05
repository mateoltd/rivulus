//! Deterministic mock REST server (`async fn spawn() -> SocketAddr`).
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Spawn a stub HTTP server returning `200 {}` for any request.
pub async fn spawn() -> SocketAddr {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let a = l.local_addr().expect("addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf).await;
                let body = b"{}";
                let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n", body.len());
                let _ = s.write_all(head.as_bytes()).await;
                let _ = s.write_all(body).await;
            });
        }
    });
    a
}

/// Observed request (assertion hook for scripted tests).
#[derive(Debug, Default, Clone)]
pub struct Observed {
    /// HTTP method (`GET`, `POST`, ...).
    pub method: String,
    /// Path including query string.
    pub path: String,
    /// `Authorization` header value, if present.
    pub auth: Option<String>,
    /// `X-Audit-Log-Reason` header value, if present.
    pub audit: Option<String>,
    /// `Content-Type` header value, if present.
    pub content_type: Option<String>,
    /// Raw request body bytes.
    pub body: Vec<u8>,
}

/// Build a raw scripted response (`status` like `"HTTP/1.1 200 OK"`).
#[must_use]
pub fn resp(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut h = String::from(status);
    h.push_str("\r\n");
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

/// Spawn a scripted server returning queued raw responses in order.
///
/// Each connection consumes one script entry by hit index (extra hits get
/// `200 {}`). Records every request (method, path, auth/audit/content-type
/// headers, body) for server-side assertions. Returns the address, the hit
/// counter, and the observation log.
pub async fn spawn_queued(
    script: Vec<String>,
) -> (
    SocketAddr,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    std::sync::Arc<std::sync::Mutex<Vec<Observed>>>,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let a = l.local_addr().expect("addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let seen: Arc<Mutex<Vec<Observed>>> = Arc::new(Mutex::new(Vec::new()));
    let hits_srv = hits.clone();
    let seen_srv = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((s, _)) = l.accept().await else {
                break;
            };
            let i = hits_srv.fetch_add(1, Ordering::SeqCst);
            let raw = script.get(i).cloned().unwrap_or_else(|| {
                String::from("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            });
            let seen_c = seen_srv.clone();
            tokio::spawn(async move {
                let mut obs = read_request(s).await;
                if let Ok(mut v) = seen_c.lock() {
                    v.push(obs.0);
                }
                let _ = obs.1.write_all(raw.as_bytes()).await;
            });
        }
    });
    (a, hits, seen)
}

/// Read one request: headers plus `Content-Length` body (bounded waits).
async fn read_request(mut s: tokio::net::TcpStream) -> (Observed, tokio::net::TcpStream) {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    // Headers first (short bounded wait per read).
    loop {
        let n = match tokio::time::timeout(std::time::Duration::from_millis(500), s.read(&mut tmp))
            .await
        {
            Ok(Ok(n)) => n,
            _ => break,
        };
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 65536 {
            break;
        }
    }
    let head_end = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(buf.len());
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_owned();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or_default().to_owned();
    let mut auth = None;
    let mut audit = None;
    let mut content_type = None;
    let mut content_length = 0usize;
    for line in lines {
        let lower = line.to_lowercase();
        let value = |name: &str| {
            lower
                .starts_with(name)
                .then(|| line[name.len()..].trim().to_owned())
        };
        if let Some(v) = value("authorization:") {
            auth = Some(v);
        } else if let Some(v) = value("x-audit-log-reason:") {
            audit = Some(v);
        } else if let Some(v) = value("content-type:") {
            content_type = Some(v);
        } else if let Some(v) = value("content-length:") {
            content_length = v.parse().unwrap_or(0);
        }
    }
    let mut body = if buf.len() > head_end {
        buf[head_end..].to_vec()
    } else {
        Vec::new()
    };
    while body.len() < content_length {
        let n = match tokio::time::timeout(std::time::Duration::from_millis(500), s.read(&mut tmp))
            .await
        {
            Ok(Ok(n)) => n,
            _ => break,
        };
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
        if body.len() > 16 * 1024 * 1024 {
            break;
        }
    }
    body.truncate(content_length);
    (
        Observed {
            method,
            path,
            auth,
            audit,
            content_type,
            body,
        },
        s,
    )
}
