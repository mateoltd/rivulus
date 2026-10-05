//! Extra REST coverage over the scripted mock: retries, fail-fast,
//! audit headers, multipart shape, webhook shape, builders, routes.
#[path = "../../../tests/support/mock_rest.rs"]
mod mock_rest;

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

fn client_for(addr: &SocketAddr) -> rest::Client {
    rest::Client::builder(secrecy::SecretString::from(String::from("test-token")))
        .base_url(format!("http://{addr}"))
        .build()
        .expect("client")
}

fn ok(body: &str) -> String {
    mock_rest::resp(
        "HTTP/1.1 200 OK",
        &[("Content-Type", "application/json")],
        body,
    )
}

fn seen_all(
    seen: &std::sync::Arc<std::sync::Mutex<Vec<mock_rest::Observed>>>,
) -> Vec<mock_rest::Observed> {
    seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tokio::test]
async fn retry_429_then_200() {
    let r429 = mock_rest::resp(
        "HTTP/1.1 429 Too Many Requests",
        &[("Content-Type", "application/json")],
        r#"{"retry_after":0.05,"global":false}"#,
    );
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![r429, ok("{}")]).await;
    let client = client_for(&addr);
    let route = rest::Route::GetChannel { channel_id: 44 };
    let start = Instant::now();
    let out = client.execute(&route, None, None).await.expect("ok");
    assert_eq!(out, b"{}");
    assert!(
        start.elapsed() >= Duration::from_millis(40),
        "retry_after not honored"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn global_429_reports_global_and_locks() {
    let g = || {
        mock_rest::resp(
            "HTTP/1.1 429 Too Many Requests",
            &[
                ("X-RateLimit-Global", "true"),
                ("Content-Type", "application/json"),
            ],
            r#"{"retry_after":0.05,"global":true}"#,
        )
    };
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![g(), g(), g()]).await;
    let client = client_for(&addr);
    let route = rest::Route::GetChannel { channel_id: 1 };
    let err = client
        .execute(&route, None, None)
        .await
        .expect_err("limited");
    assert!(
        matches!(err, common::Error::RateLimited { global: true, .. }),
        "got {err:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 3);
    // The global lock is real: any bucket waits while it is held.
    let mut limiter = rest::Ratelimiter::new();
    limiter.retry_after_for_429(5.0, true);
    assert!(limiter.should_wait("GET:/x:1").is_some());
    let mut plain = rest::Ratelimiter::new();
    assert_eq!(plain.retry_after_for_429(0.5, false), 0.5);
    assert!(plain.should_wait("GET:/x:1").is_none());
    plain.register_headers("GET:/x:1", 5, 3, 0.0);
    assert!(plain.should_wait("GET:/x:1").is_none());
}

#[tokio::test]
async fn preemptive_wait_remaining_zero() {
    let first = mock_rest::resp(
        "HTTP/1.1 200 OK",
        &[
            ("X-RateLimit-Limit", "1"),
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset-After", "0.15"),
            ("Content-Type", "application/json"),
        ],
        "{}",
    );
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![first, ok("{}")]).await;
    let client = client_for(&addr);
    let route = rest::Route::GetChannel { channel_id: 2 };
    client.execute(&route, None, None).await.expect("first");
    let start = Instant::now();
    client.execute(&route, None, None).await.expect("second");
    assert!(
        start.elapsed() >= Duration::from_millis(120),
        "pre-emptive wait skipped"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn fail_fast_404_single_hit() {
    let n404 = mock_rest::resp(
        "HTTP/1.1 404 Not Found",
        &[("Content-Type", "application/json")],
        "{}",
    );
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![n404]).await;
    let client = client_for(&addr);
    let route = rest::Route::GetChannel { channel_id: 7 };
    let err = client.execute(&route, None, None).await.expect_err("404");
    assert!(matches!(err, common::Error::NotFound(_)), "got {err:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn get_5xx_retries_delete_too_post_does_not() {
    let e500 = mock_rest::resp("HTTP/1.1 500 Internal Server Error", &[], "{}");
    // GET retries then succeeds.
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![e500.clone(), ok("{}")]).await;
    let client = client_for(&addr);
    let out = client
        .execute(&rest::Route::GetChannel { channel_id: 3 }, None, None)
        .await
        .expect("get retries");
    assert_eq!(out, b"{}");
    assert_eq!(hits.load(Ordering::SeqCst), 2);
    // DELETE retries then succeeds.
    let (addr2, hits2, _seen) = mock_rest::spawn_queued(vec![e500.clone(), ok("{}")]).await;
    let client2 = client_for(&addr2);
    let out = client2
        .execute(
            &rest::Route::DeleteMessage {
                channel_id: 3,
                message_id: 4,
            },
            None,
            None,
        )
        .await
        .expect("delete retries");
    assert_eq!(out, b"{}");
    assert_eq!(hits2.load(Ordering::SeqCst), 2);
    // POST never retries a 500 (duplicate risk): single hit, typed error.
    let (addr3, hits3, _seen) = mock_rest::spawn_queued(vec![e500]).await;
    let client3 = client_for(&addr3);
    let err = client3
        .execute(
            &rest::Route::CreateMessage { channel_id: 3 },
            Some(b"{}".to_vec()),
            None,
        )
        .await
        .expect_err("post fails fast");
    assert!(
        matches!(err, common::Error::Rest { status: 500, .. }),
        "got {err:?}"
    );
    assert_eq!(hits3.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn error_body_maps_code_and_message_with_defaults() {
    let bad = mock_rest::resp(
        "HTTP/1.1 400 Bad Request",
        &[("Content-Type", "application/json")],
        r#"{"code":50013,"message":"missing permissions"}"#,
    );
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![bad]).await;
    let client = client_for(&addr);
    let err = client
        .execute(&rest::Route::GetChannel { channel_id: 1 }, None, None)
        .await
        .expect_err("400");
    assert!(
        matches!(
            err,
            common::Error::Rest {
                status: 400,
                code: 50013,
                ..
            }
        ),
        "got {err:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // Empty body falls back to generic message/code.
    let bare = mock_rest::resp("HTTP/1.1 400 Bad Request", &[], "");
    let (addr2, _hits2, _seen) = mock_rest::spawn_queued(vec![bare]).await;
    let client2 = client_for(&addr2);
    let err2 = client2
        .execute(&rest::Route::GetChannel { channel_id: 1 }, None, None)
        .await
        .expect_err("400 bare");
    assert!(
        matches!(
            err2,
            common::Error::Rest {
                status: 400,
                code: 0,
                ..
            }
        ),
        "got {err2:?}"
    );
}

#[tokio::test]
async fn malformed_429_body_uses_defaults_then_succeeds() {
    let weird = mock_rest::resp("HTTP/1.1 429 Too Many Requests", &[], "not json");
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![weird, ok("{}")]).await;
    let client = client_for(&addr);
    let out = client
        .execute(&rest::Route::GetChannel { channel_id: 5 }, None, None)
        .await
        .expect("default retry_after then ok");
    assert_eq!(out, b"{}");
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn audit_reason_header_observed_and_validated() {
    let (addr, hits, seen) = mock_rest::spawn_queued(vec![ok("{}")]).await;
    let client = client_for(&addr);
    let route = rest::Route::DeleteMessage {
        channel_id: 3,
        message_id: 4,
    };
    client
        .execute(&route, None, Some("hello world"))
        .await
        .expect("audit ok");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let reqs = seen_all(&seen);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].audit.as_deref(), Some("hello%20world"));
    assert_eq!(reqs[0].auth.as_deref(), Some("Bot test-token"));
    assert_eq!(reqs[0].method, "DELETE");
    // Invalid reasons never hit the network, on any entrypoint.
    for bad in ["", "has\nnewline", &"x".repeat(513)] {
        let e = client
            .execute(&route, None, Some(bad))
            .await
            .expect_err("bad audit");
        assert!(matches!(e, common::Error::Validation(_)), "got {e:?}");
        let f = rest::multipart::MultipartFile::new(
            Box::from("a.txt"),
            String::from("text/plain"),
            b"hi".to_vec(),
            None,
        );
        let e2 = client
            .execute_multipart(
                &rest::Route::CreateMessage { channel_id: 3 },
                b"{}",
                &[f],
                Some(bad),
            )
            .await
            .expect_err("bad audit multipart");
        assert!(matches!(e2, common::Error::Validation(_)));
        let secret = secrecy::SecretString::from(format!("http://{addr}/webhooks/1/abc"));
        let e3 = client
            .execute_webhook(&secret, None, Some(b"{}".to_vec()), Some(bad))
            .await
            .expect_err("bad audit webhook");
        assert!(matches!(e3, common::Error::Validation(_)));
    }
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn multipart_payload_json_first() {
    let (addr, hits, seen) = mock_rest::spawn_queued(vec![ok("{}")]).await;
    let client = client_for(&addr);
    let route = rest::Route::CreateMessage { channel_id: 5 };
    let file = rest::multipart::MultipartFile::new(
        Box::from("a.txt"),
        String::from("text/plain"),
        b"FILEBYTES123".to_vec(),
        None,
    );
    let payload = br#"{"content":"hi"}"#;
    let out = client
        .execute_multipart(&route, payload, &[file], None)
        .await
        .expect("multipart ok");
    assert_eq!(out, b"{}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let reqs = seen_all(&seen);
    assert_eq!(reqs.len(), 1);
    let ct = reqs[0].content_type.clone().unwrap_or_default();
    assert!(ct.starts_with("multipart/form-data;"), "got {ct}");
    let body = &reqs[0].body;
    let find = |needle: &[u8]| {
        body.windows(needle.len())
            .position(|w| w == needle)
            .expect("part present")
    };
    let pj = find(b"name=\"payload_json\"");
    let f0 = find(b"name=\"files[0]\"");
    assert!(pj < f0, "payload_json must be the first part");
    assert!(find(br#"{"content":"hi"}"#) > pj);
    assert!(find(b"FILEBYTES123") > f0);
}

#[tokio::test]
async fn multipart_error_arms() {
    // 404 fail-fast.
    let n404 = mock_rest::resp("HTTP/1.1 404 Not Found", &[], "{}");
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![n404]).await;
    let client = client_for(&addr);
    let route = rest::Route::CreateMessage { channel_id: 5 };
    let file = || {
        rest::multipart::MultipartFile::new(
            Box::from("a.txt"),
            String::from("text/plain"),
            b"hi".to_vec(),
            None,
        )
    };
    let err = client
        .execute_multipart(&route, b"{}", &[file()], None)
        .await
        .expect_err("404");
    assert!(matches!(err, common::Error::NotFound(_)));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // 429 then 200.
    let r429 = mock_rest::resp(
        "HTTP/1.1 429 Too Many Requests",
        &[("Content-Type", "application/json")],
        r#"{"retry_after":0.05,"global":false}"#,
    );
    let (addr2, hits2, _seen) = mock_rest::spawn_queued(vec![r429, ok("{}")]).await;
    let client2 = client_for(&addr2);
    let out = client2
        .execute_multipart(&route, b"{}", &[file()], None)
        .await
        .expect("retry ok");
    assert_eq!(out, b"{}");
    assert_eq!(hits2.load(Ordering::SeqCst), 2);
    // 500 POST: no retry.
    let e500 = mock_rest::resp("HTTP/1.1 500 Internal Server Error", &[], "{}");
    let (addr3, hits3, _seen) = mock_rest::spawn_queued(vec![e500]).await;
    let client3 = client_for(&addr3);
    let err3 = client3
        .execute_multipart(&route, b"{}", &[file()], None)
        .await
        .expect_err("500");
    assert!(matches!(err3, common::Error::Rest { status: 500, .. }));
    assert_eq!(hits3.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn webhook_query_shape_404_and_500() {
    let (addr, hits, seen) = mock_rest::spawn_queued(vec![ok(r#"{"id":"9"}"#)]).await;
    let client = client_for(&addr);
    let secret = secrecy::SecretString::from(format!("http://{addr}/webhooks/1/abc"));
    let q = rest::ExecuteWebhook::new().wait(true).thread_id(7).query();
    assert_eq!(q, "?wait=true&thread_id=7");
    let out = client
        .execute_webhook(&secret, Some(&q), Some(b"{}".to_vec()), Some("reason"))
        .await
        .expect("webhook ok");
    assert_eq!(out, br#"{"id":"9"}"#);
    let reqs = seen_all(&seen);
    assert_eq!(reqs.len(), 1);
    assert!(
        reqs[0].path.contains("?wait=true&thread_id=7"),
        "got {}",
        reqs[0].path
    );
    assert_eq!(reqs[0].audit.as_deref(), Some("reason"));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    // 404 marks the webhook dead: single hit.
    let n404 = mock_rest::resp("HTTP/1.1 404 Not Found", &[], "{}");
    let (addr2, hits2, _seen) = mock_rest::spawn_queued(vec![n404]).await;
    let client2 = client_for(&addr2);
    let secret2 = secrecy::SecretString::from(format!("http://{addr2}/webhooks/1/abc"));
    let err = client2
        .execute_webhook(&secret2, None, None, None)
        .await
        .expect_err("dead webhook");
    assert!(matches!(err, common::Error::NotFound(_)));
    assert_eq!(hits2.load(Ordering::SeqCst), 1);
    // 500 POST webhook: no retry.
    let e500 = mock_rest::resp("HTTP/1.1 500 Internal Server Error", &[], "{}");
    let (addr3, hits3, _seen) = mock_rest::spawn_queued(vec![e500]).await;
    let client3 = client_for(&addr3);
    let secret3 = secrecy::SecretString::from(format!("http://{addr3}/webhooks/1/abc"));
    let err3 = client3
        .execute_webhook(&secret3, None, Some(b"{}".to_vec()), None)
        .await
        .expect_err("500");
    assert!(matches!(err3, common::Error::Rest { status: 500, .. }));
    assert_eq!(hits3.load(Ordering::SeqCst), 1);
    // Webhook 400 maps code/message; webhook 429 retries once.
    let bad = mock_rest::resp(
        "HTTP/1.1 400 Bad Request",
        &[("Content-Type", "application/json")],
        r#"{"code":1,"message":"bad"}"#,
    );
    let (addr4, _h4, _s) = mock_rest::spawn_queued(vec![bad]).await;
    let client4 = client_for(&addr4);
    let secret4 = secrecy::SecretString::from(format!("http://{addr4}/webhooks/1/abc"));
    let err4 = client4
        .execute_webhook(&secret4, None, None, None)
        .await
        .expect_err("400");
    assert!(matches!(err4, common::Error::Rest { status: 400, .. }));
    let r429 = mock_rest::resp(
        "HTTP/1.1 429 Too Many Requests",
        &[("Content-Type", "application/json")],
        r#"{"retry_after":0.05,"global":false}"#,
    );
    let (addr5, hits5, _s) = mock_rest::spawn_queued(vec![r429, ok("{}")]).await;
    let client5 = client_for(&addr5);
    let secret5 = secrecy::SecretString::from(format!("http://{addr5}/webhooks/1/abc"));
    client5
        .execute_webhook(&secret5, None, None, None)
        .await
        .expect("webhook retry");
    assert_eq!(hits5.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn json_array_body_roundtrip() {
    let (addr, hits, _seen) = mock_rest::spawn_queued(vec![ok(r#"[{"n":1},{"n":2}]"#)]).await;
    let client = client_for(&addr);
    let route = rest::Route::ListMessages {
        channel_id: 3,
        limit: 2,
        after: None,
    };
    let out = client.execute(&route, None, None).await.expect("array");
    assert_eq!(out, br#"[{"n":1},{"n":2}]"#);
    let v: serde_json::Value = serde_json::from_slice(&out).expect("json");
    assert_eq!(v.as_array().map(|a| a.len()), Some(2));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn network_and_timeout_errors() {
    // Refused connection maps to Network.
    let l = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let dead = l.local_addr().expect("addr");
    drop(l);
    let refused = rest::Client::builder(secrecy::SecretString::from(String::from("t")))
        .base_url(format!("http://{dead}"))
        .build()
        .expect("client");
    let err = refused
        .execute(&rest::Route::GetChannel { channel_id: 1 }, None, None)
        .await
        .expect_err("refused");
    assert!(matches!(err, common::Error::Network(_)), "got {err:?}");
    // Silent server + short timeout maps to Timeout.
    let l2 = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr2 = l2.local_addr().expect("addr");
    tokio::spawn(async move {
        let Ok((mut s, _)) = l2.accept().await else {
            return;
        };
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 1024];
        let _ = s.read(&mut buf).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    let slow = rest::Client::builder(secrecy::SecretString::from(String::from("t")))
        .base_url(format!("http://{addr2}"))
        .timeout(Duration::from_millis(100))
        .build()
        .expect("client");
    let err2 = slow
        .execute(&rest::Route::GetChannel { channel_id: 1 }, None, None)
        .await
        .expect_err("timeout");
    assert!(matches!(err2, common::Error::Timeout(_)), "got {err2:?}");
    // Builder surface: route_url respects the base, inner exists.
    let (addr3, _h, _s) = mock_rest::spawn_queued(vec![ok("{}")]).await;
    let c3 = client_for(&addr3);
    assert!(c3
        .route_url(&rest::Route::GetChannel { channel_id: 9 })
        .contains(&addr3.to_string()));
    let _ = c3.inner();
}

#[test]
fn method_names() {
    assert_eq!(rest::Method::Get.as_str(), "GET");
    assert_eq!(rest::Method::Post.as_str(), "POST");
    assert_eq!(rest::Method::Put.as_str(), "PUT");
    assert_eq!(rest::Method::Patch.as_str(), "PATCH");
    assert_eq!(rest::Method::Delete.as_str(), "DELETE");
}

#[test]
fn builders_validate_branches() {
    // Thread names: empty and over-long both fail.
    assert!(rest::ExecuteWebhook::new()
        .thread_name("")
        .validate()
        .is_err());
    assert!(rest::ExecuteWebhook::new()
        .thread_name("x".repeat(101))
        .validate()
        .is_err());
    assert!(rest::ExecuteWebhook::new()
        .thread_name("ok-name_~.")
        .validate()
        .is_ok());
    let q = rest::ExecuteWebhook::new().thread_name("a&b=c").query();
    assert_eq!(q, "?thread_name=a%26b%3Dc");
    assert!(rest::ExecuteWebhook::new()
        .content("x".repeat(2001))
        .validate()
        .is_err());
    assert!(!rest::ExecuteWebhook::new()
        .content("hi")
        .build()
        .expect("build")
        .is_empty());
    // Interaction: ephemeral flag lands in JSON; dataless ack has no data.
    let eph = rest::InteractionCallback::new(4)
        .content("hi")
        .ephemeral(true);
    assert!(eph.validate().is_ok());
    let bytes = eph.build().expect("build");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["data"]["flags"], 64);
    let ack = rest::InteractionCallback::new(1).build().expect("ack");
    let av: serde_json::Value = serde_json::from_slice(&ack).expect("json");
    assert!(av.get("data").is_none());
    let v2 = rest::InteractionCallback::new(4)
        .content("hi")
        .components_v2(true)
        .ephemeral(true);
    let vv: serde_json::Value = serde_json::from_slice(&v2.build().expect("v2")).expect("json");
    assert_eq!(vv["data"]["flags"], 64 + 32768);
    assert!(rest::InteractionCallback::new(4)
        .content("x".repeat(2001))
        .validate()
        .is_err());
    // Message: embed errors propagate; v2 flag serializes; embeds build.
    let bad_embed = rest::CreateEmbed::new().description("x".repeat(6001));
    assert!(rest::CreateMessage::new()
        .embed(bad_embed)
        .validate()
        .is_err());
    let with_embed = rest::CreateMessage::new().content("hi").embed(
        rest::CreateEmbed::new()
            .title("t")
            .description("d")
            .color(3),
    );
    let bytes = with_embed.build().expect("build");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["embeds"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(v["embeds"][0]["color"], 3);
    let v2m = rest::CreateMessage::new().content("hi").components_v2(true);
    let bytes = v2m.build().expect("v2 build");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(v["flags"], 32768);
    assert!(rest::CreateMessage::new().build().expect("empty") == b"{}");
    // Multipart validation: bad mime and bad filename both fail.
    let bad_mime = rest::multipart::MultipartFile::new(
        Box::from("a.txt"),
        String::from("not a mime!!"),
        b"hi".to_vec(),
        None,
    );
    assert!(rest::multipart::message_form(b"{}", &[bad_mime]).is_err());
    let bad_name = rest::multipart::MultipartFile::new(
        Box::from("a/b.txt"),
        String::from("text/plain"),
        b"hi".to_vec(),
        None,
    );
    assert!(rest::multipart::message_form(b"{}", &[bad_name]).is_err());
    let sticker_bad = rest::multipart::MultipartFile::new(
        Box::from("s.png"),
        String::from("not a mime!!"),
        vec![1],
        None,
    );
    assert!(rest::multipart::sticker_form("n", "t", &sticker_bad).is_err());
    let sticker_name = rest::multipart::MultipartFile::new(
        Box::from("a/b.png"),
        String::from("image/png"),
        vec![1],
        None,
    );
    assert!(rest::multipart::sticker_form("n", "t", &sticker_name).is_err());
    // Ban meter thresholds and default.
    let mut meter = rest::BanMeter::default();
    for _ in 0..5000 {
        meter.record(429, "user");
    }
    let (warn, err) = meter.record(200, "user");
    assert!(warn && !err);
    for _ in 0..3000 {
        meter.record(401, "user");
    }
    let (warn, err) = meter.record(403, "user");
    assert!(warn && err);
}

#[test]
fn routes_full_matrix() {
    use rest::Route;
    let base = "http://127.0.0.1:9/";
    let routes: Vec<Route> = vec![
        Route::GetChannel { channel_id: 1 },
        Route::CreateMessage { channel_id: 1 },
        Route::GetMessage {
            channel_id: 1,
            message_id: 2,
        },
        Route::EditMessage {
            channel_id: 1,
            message_id: 2,
        },
        Route::DeleteMessage {
            channel_id: 1,
            message_id: 2,
        },
        Route::BulkDelete { channel_id: 1 },
        Route::GetGuild { guild_id: 1 },
        Route::CreateGuildChannel { guild_id: 1 },
        Route::ListMembers { guild_id: 1 },
        Route::ListGuildChannels { guild_id: 1 },
        Route::ListMessages {
            channel_id: 1,
            limit: 10,
            after: Some(2),
        },
        Route::ListCurrentUserGuilds { limit: 10 },
        Route::GetMember {
            guild_id: 1,
            user_id: 2,
        },
        Route::CreateRole { guild_id: 1 },
        Route::GetAuditLog { guild_id: 1 },
        Route::CreateWebhook { channel_id: 1 },
        Route::ExecuteWebhook {
            webhook_id: 1,
            token_is_present: true,
        },
        Route::GetGatewayBot,
        Route::CreateApplicationCommand { application_id: 1 },
        Route::CreateGuildCommand {
            application_id: 1,
            guild_id: 2,
        },
        Route::InteractionCallback {
            interaction_id: 1,
            token_is_present: true,
        },
    ];
    for r in &routes {
        // Trailing-slash base is trimmed; every route renders a URL.
        let u = r.url_with_base(base);
        assert!(u.starts_with("http://127.0.0.1:9/"), "got {u}");
        assert!(!r.path_template().is_empty());
        assert!(!rest::Ratelimiter::bucket_key(r).is_empty());
        let _ = r.url();
    }
    assert_eq!(
        Route::EditMessage {
            channel_id: 1,
            message_id: 2
        }
        .method(),
        rest::Method::Patch
    );
    assert_eq!(
        Route::DeleteMessage {
            channel_id: 1,
            message_id: 2
        }
        .method(),
        rest::Method::Delete
    );
    assert_eq!(
        Route::BulkDelete { channel_id: 1 }.method(),
        rest::Method::Post
    );
    assert_eq!(
        Route::GetMessage {
            channel_id: 1,
            message_id: 2
        }
        .path(),
        "/channels/1/messages/2"
    );
    assert_eq!(
        Route::BulkDelete { channel_id: 1 }.path(),
        "/channels/1/messages/bulk-delete"
    );
    assert_eq!(Route::GetGuild { guild_id: 7 }.path(), "/guilds/7");
    assert_eq!(
        Route::CreateGuildChannel { guild_id: 7 }.path(),
        "/guilds/7/channels"
    );
    assert_eq!(
        Route::ListGuildChannels { guild_id: 7 }.path(),
        "/guilds/7/channels"
    );
    assert_eq!(
        Route::ListMembers { guild_id: 7 }.path(),
        "/guilds/7/members"
    );
    assert_eq!(
        Route::GetMember {
            guild_id: 7,
            user_id: 8
        }
        .path(),
        "/guilds/7/members/8"
    );
    assert_eq!(Route::CreateRole { guild_id: 7 }.path(), "/guilds/7/roles");
    assert_eq!(
        Route::GetAuditLog { guild_id: 7 }.path(),
        "/guilds/7/audit-logs"
    );
    assert_eq!(
        Route::CreateWebhook { channel_id: 7 }.path(),
        "/channels/7/webhooks"
    );
    assert_eq!(
        Route::ListCurrentUserGuilds { limit: 5 }.path(),
        "/users/@me/guilds"
    );
    assert_eq!(Route::GetGatewayBot.path(), "/gateway/bot");
    assert_eq!(
        Route::CreateApplicationCommand { application_id: 3 }.path(),
        "/applications/3/commands"
    );
    assert_eq!(
        Route::CreateGuildCommand {
            application_id: 3,
            guild_id: 4
        }
        .path(),
        "/applications/3/guilds/4/commands"
    );
    assert_eq!(
        Route::InteractionCallback {
            interaction_id: 3,
            token_is_present: false
        }
        .path(),
        "/interactions/3/callback"
    );
    assert_eq!(
        Route::ExecuteWebhook {
            webhook_id: 3,
            token_is_present: true
        }
        .path(),
        "/webhooks/3"
    );
    assert_eq!(Route::GetGuild { guild_id: 7 }.major_id(), Some(7));
    assert_eq!(
        Route::CreateGuildCommand {
            application_id: 3,
            guild_id: 4
        }
        .major_id(),
        Some(4)
    );
    assert_eq!(
        Route::CreateApplicationCommand { application_id: 3 }.major_id(),
        Some(3)
    );
    assert_eq!(
        Route::ExecuteWebhook {
            webhook_id: 3,
            token_is_present: true
        }
        .major_id(),
        Some(3)
    );
    assert_eq!(
        Route::InteractionCallback {
            interaction_id: 3,
            token_is_present: true
        }
        .major_id(),
        Some(3)
    );
    assert_eq!(
        Route::ListMessages {
            channel_id: 1,
            limit: 10,
            after: Some(2)
        }
        .query(),
        Some(String::from("limit=10&after=2"))
    );
    assert_eq!(
        Route::ListCurrentUserGuilds { limit: 0 }.query(),
        Some(String::from("limit=1"))
    );
    assert_eq!(
        Route::GetChannel { channel_id: 1 }.path_template(),
        "/channels/{channel_id}"
    );
    assert_eq!(
        Route::GetMessage {
            channel_id: 1,
            message_id: 2
        }
        .path_template(),
        "/channels/{channel_id}/messages/{message_id}"
    );
    assert_eq!(
        Route::BulkDelete { channel_id: 1 }.path_template(),
        "/channels/{channel_id}/messages/bulk-delete"
    );
    assert_eq!(
        Route::GetGuild { guild_id: 1 }.path_template(),
        "/guilds/{guild_id}"
    );
    assert_eq!(
        Route::ListMembers { guild_id: 1 }.path_template(),
        "/guilds/{guild_id}/members"
    );
    assert_eq!(
        Route::GetMember {
            guild_id: 1,
            user_id: 2
        }
        .path_template(),
        "/guilds/{guild_id}/members/{user_id}"
    );
    assert_eq!(
        Route::CreateRole { guild_id: 1 }.path_template(),
        "/guilds/{guild_id}/roles"
    );
    assert_eq!(
        Route::GetAuditLog { guild_id: 1 }.path_template(),
        "/guilds/{guild_id}/audit-logs"
    );
    assert_eq!(
        Route::CreateWebhook { channel_id: 1 }.path_template(),
        "/channels/{channel_id}/webhooks"
    );
    assert_eq!(
        Route::ExecuteWebhook {
            webhook_id: 1,
            token_is_present: false
        }
        .path_template(),
        "/webhooks/{webhook_id}/{token}"
    );
    assert_eq!(Route::GetGatewayBot.path_template(), "/gateway/bot");
    assert_eq!(
        Route::CreateApplicationCommand { application_id: 1 }.path_template(),
        "/applications/{application_id}/commands"
    );
    assert_eq!(
        Route::CreateGuildCommand {
            application_id: 1,
            guild_id: 2
        }
        .path_template(),
        "/applications/{application_id}/guilds/{guild_id}/commands"
    );
    assert_eq!(
        Route::InteractionCallback {
            interaction_id: 1,
            token_is_present: true
        }
        .path_template(),
        "/interactions/{interaction_id}/{token}/callback"
    );
}
