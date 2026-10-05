//! Boundary tests against scripted mocks (M3).
//!
//! Drives the napi surface fns directly (plain Rust fns under the
//! attributes) against the workspace scripted gateway plus queued REST
//! mocks on 127.0.0.1. Mock token only; never production; never a secret.
//! `subscribe` needs a real JS env, so listener delivery waits for the M4
//! Node-runner suite; everything else is proven here.

#[allow(dead_code, clippy::all)]
#[path = "../../tests/support/mock_gateway.rs"]
mod mock_gateway;
#[allow(dead_code, clippy::all)]
#[path = "../../tests/support/mock_rest.rs"]
mod mock_rest;

use rivulus_node::{
    close, create_client, create_test_client, fetch_page, get_cache_stats, get_health,
    get_latencies, get_uptime_ms, login, stream_pages, verify_webhook, CreateOptions,
};

fn options() -> CreateOptions {
    CreateOptions {
        token: String::from("mock-token-for-m3-mocks"),
        intents: 513,
        cache: String::from("balanced"),
        sharding: String::from("auto"),
    }
}

fn message_json(id: &str) -> String {
    let body = format!(
        r#"[{{"id":"{id}","channel_id":"3","author":{{"id":"2","username":"u"}},"content":"hi","timestamp":"2024-01-01T00:00:00Z"}}]"#
    );
    mock_rest::resp(
        "HTTP/1.1 200 OK",
        &[("Content-Type", "application/json")],
        &body,
    )
}

/// Test client pointed at mocks (REST script plus scripted gateway).
async fn mock_client(rest_script: Vec<String>) -> f64 {
    let (rest_addr, _hits, _seen) = mock_rest::spawn_queued(rest_script).await;
    let (gw_addr, _gw_seen) = mock_gateway::spawn_script().await;
    create_test_client(
        options(),
        format!("http://{rest_addr}"),
        format!("ws://{gw_addr}/"),
    )
    .expect("test client builds")
}

#[tokio::test]
async fn login_reaches_ready_against_scripted_gateway() {
    let handle = mock_client(vec![]).await;
    login(handle).await.expect("READY via mock gateway");
    let health = get_health(handle).expect("health reads");
    assert!(health.ready);
    close(handle).await.expect("close ok");
}

#[tokio::test]
async fn fetch_page_returns_snapshots_with_string_ids() {
    let handle = mock_client(vec![message_json("4")]).await;
    login(handle).await.expect("READY");
    let page = fetch_page(handle, String::from("3"), 50.0)
        .await
        .expect("page parses");
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, "4");
    assert_eq!(page[0].channel_id, "3");
    assert_eq!(page[0].content, "hi");
    assert!(!page[0].timestamp.is_empty(), "rfc3339 timestamp");
    close(handle).await.expect("close ok");
}

#[tokio::test]
async fn clamp_zero_maps_to_one() {
    let handle = mock_client(vec![message_json("9")]).await;
    login(handle).await.expect("READY");
    let page = fetch_page(handle, String::from("3"), 0.0)
        .await
        .expect("clamped fetch ok");
    assert_eq!(page.len(), 1);
    close(handle).await.expect("close ok");
}

#[tokio::test]
async fn stream_pages_advances_by_cursor_then_stops() {
    let handle = mock_client(vec![
        message_json("4"),
        mock_rest::resp(
            "HTTP/1.1 200 OK",
            &[("Content-Type", "application/json")],
            "[]",
        ),
    ])
    .await;
    login(handle).await.expect("READY");
    let pages = stream_pages(handle, String::from("3"), 1.0)
        .await
        .expect("stream ok");
    assert_eq!(pages.len(), 2, "full page advances, empty page stops");
    assert_eq!(pages[0].len(), 1);
    assert!(pages[1].is_empty());
    close(handle).await.expect("close ok");
}

#[tokio::test]
async fn unknown_handle_rejects_everywhere() {
    let bad = 999_999.0;
    assert!(login(bad).await.is_err());
    assert!(fetch_page(bad, String::from("3"), 5.0).await.is_err());
    assert!(get_health(bad).is_err());
    assert!(close(bad).await.is_err());
}

#[tokio::test]
async fn bad_ids_reject_with_typed_strings() {
    let handle = create_client(options()).expect("builds offline");
    assert!(fetch_page(handle, String::from("abc"), 5.0).await.is_err());
    assert!(fetch_page(handle, String::from("3"), f64::NAN)
        .await
        .is_err());
    assert!(fetch_page(handle, String::from("0"), 5.0).await.is_err());
    close(handle).await.expect("close ok");
}

#[tokio::test]
async fn getters_read_without_network() {
    let handle = create_client(options()).expect("builds offline");
    let latencies = get_latencies(handle).expect("latencies read");
    assert_eq!(latencies.len(), 1);
    let uptime = get_uptime_ms(handle).expect("uptime reads");
    assert!(uptime < 600_000.0);
    let stats = get_cache_stats(handle).expect("cache stats read");
    assert_eq!(stats.hit_ratio, 1.0);
    close(handle).await.expect("close ok");
}

#[tokio::test]
async fn verify_webhook_failure_is_typed() {
    let body = napi::bindgen_prelude::Buffer::from(b"{\"type\":1}".to_vec());
    let err = verify_webhook(
        String::from("00").repeat(32),
        String::from("1699000000"),
        body,
        String::from("ff").repeat(64),
    )
    .expect_err("bogus signature must fail closed");
    let text = format!("{err}");
    assert!(
        text.contains("signature invalid"),
        "fail-closed string, got: {text}"
    );
}
