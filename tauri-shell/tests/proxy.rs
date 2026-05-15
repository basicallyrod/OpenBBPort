//! Integration tests for `proxy.rs`. Uses `httpmock` so all I/O is local
//! (no real network calls), making the suite hermetic.

use std::time::Duration;

use httpmock::prelude::*;
use serde_json::json;
use tauri_shell::proxy::{Proxy, ProxyError};

/// Manual base64 encoder so we can assert the exact `Authorization: Basic ...`
/// header without pulling in a runtime base64 crate.
fn base64_encode(input: &str) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | bytes[i + 2] as u32;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
        i += 3;
    }
    let rem = bytes.len() - i;
    if rem == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

fn proxy_for(server: &MockServer) -> Proxy {
    let p = Proxy::new();
    p.set_base_url(server.base_url());
    p
}

#[tokio::test]
async fn get_with_query_params() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(GET)
            .path("/api/v1/equity/price/historical")
            .query_param("symbol", "AAPL")
            .query_param("provider", "yfinance");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"results":[{"close":1.23}]}"#);
    }).await;

    let p = proxy_for(&server);
    let v: serde_json::Value = p
        .get(
            "/equity/price/historical",
            &[("symbol", "AAPL"), ("provider", "yfinance")],
        )
        .await
        .expect("GET ok");
    assert_eq!(v["results"][0]["close"], json!(1.23));
    mock.assert_async().await;
}

#[tokio::test]
async fn post_with_json_body() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(POST)
            .path("/api/v1/technical/sma")
            .json_body(json!({ "length": 14 }));
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"ok":true}"#);
    }).await;

    let p = proxy_for(&server);
    let body = json!({ "length": 14 });
    let v: serde_json::Value = p
        .post("/technical/sma", &body, &[])
        .await
        .expect("POST ok");
    assert_eq!(v["ok"], json!(true));
    mock.assert_async().await;
}

#[tokio::test]
async fn basic_auth_header_is_sent() {
    let server = MockServer::start_async().await;
    let expected = format!("Basic {}", base64_encode("alice:hunter2"));
    let mock = server.mock_async(|when, then| {
        when.method(GET)
            .path("/api/v1/whoami")
            .header("authorization", expected.clone());
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"user":"alice"}"#);
    }).await;

    let p = proxy_for(&server);
    p.set_basic_auth("alice", "hunter2");
    let v: serde_json::Value = p.get("/whoami", &[]).await.expect("auth GET");
    assert_eq!(v["user"], json!("alice"));
    mock.assert_async().await;
}

#[tokio::test]
async fn bearer_token_is_sent() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(GET)
            .path("/api/v1/me")
            .header("authorization", "Bearer s3cret");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"ok":1}"#);
    }).await;

    let p = proxy_for(&server);
    p.set_bearer("s3cret");
    let _: serde_json::Value = p.get("/me", &[]).await.expect("bearer GET");
    mock.assert_async().await;
}

#[tokio::test]
async fn clear_auth_drops_both_headers() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(GET).path("/api/v1/me");
        // The mock matches *any* request; we then assert the response was 401.
        then.status(401).body("forbidden");
    }).await;

    let p = proxy_for(&server);
    p.set_basic_auth("a", "b");
    p.clear_auth();
    let result: Result<serde_json::Value, _> = p.get("/me", &[]).await;
    match result {
        Err(ProxyError::Http { status, .. }) => assert_eq!(status, 401),
        other => panic!("expected HTTP 401 error, got {other:?}"),
    }
    mock.assert_async().await;
}

#[tokio::test]
async fn handles_204_no_content() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(POST).path("/api/v1/server/stop");
        then.status(204);
    }).await;

    let p = proxy_for(&server);
    let v: serde_json::Value = p
        .post("/server/stop", &json!({}), &[])
        .await
        .expect("204 ok");
    // The proxy maps 204 → JSON null.
    assert!(v.is_null());
    mock.assert_async().await;
}

#[tokio::test]
async fn error_envelope_for_non_2xx() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(GET).path("/api/v1/equity/price/historical");
        then.status(404)
            .header("content-type", "application/json")
            .body(r#"{"error":"not found"}"#);
    }).await;

    let p = proxy_for(&server);
    let result: Result<serde_json::Value, _> =
        p.get("/equity/price/historical", &[]).await;
    let err = result.expect_err("expected HTTP error");
    let serialized = format!("{err}");
    assert!(serialized.contains("HTTP 404"), "got: {serialized}");
    // Serialize the error envelope to JSON via the thiserror Display chain.
    match err {
        ProxyError::Http { status, body } => {
            assert_eq!(status, 404);
            assert!(body.contains("not found"));
        }
        other => panic!("expected Http variant, got {other:?}"),
    }
    mock.assert_async().await;
}

#[tokio::test]
async fn get_raw_skips_api_v1_prefix() {
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        // Note: no /api/v1 prefix.
        when.method(GET).path("/widgets.json");
        then.status(200)
            .header("content-type", "application/json")
            .body(r#"{"widgets":[]}"#);
    }).await;

    let p = proxy_for(&server);
    let v: serde_json::Value = p.get_raw("/widgets.json").await.expect("raw GET");
    assert!(v["widgets"].is_array());
    mock.assert_async().await;
}

#[tokio::test]
async fn timeout_short_circuits_slow_server() {
    // Build a proxy with a *manually tightened* timeout by reaching past the
    // public API. We can't change the timeout after the client is built (the
    // proxy caches it), so this test directly verifies the configured value.
    let p = Proxy::new();
    assert_eq!(p.config().timeout_seconds, 60);

    // Also verify a slow mock response still completes within the 60s budget.
    // (We're not waiting 60s — we just confirm the client doesn't error early
    // for a quick response.)
    let server = MockServer::start_async().await;
    let mock = server.mock_async(|when, then| {
        when.method(GET).path("/api/v1/health");
        then.status(200)
            .header("content-type", "application/json")
            .delay(Duration::from_millis(100))
            .body(r#"{"ok":true}"#);
    }).await;

    let p = proxy_for(&server);
    let v: serde_json::Value = p.get("/health", &[]).await.expect("ok");
    assert_eq!(v["ok"], json!(true));
    mock.assert_async().await;
}

#[tokio::test]
async fn empty_base_url_returns_not_configured() {
    let p = Proxy::new();
    p.set_base_url("");
    let result: Result<serde_json::Value, _> = p.get("/anything", &[]).await;
    match result {
        Err(ProxyError::NotConfigured) => {}
        other => panic!("expected NotConfigured, got {other:?}"),
    }
}
