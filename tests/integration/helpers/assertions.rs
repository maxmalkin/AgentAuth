//! Custom assertion helpers for integration tests.

use axum::body::Body;
use http_body_util::BodyExt;
use hyper::StatusCode;

/// Assert the response has the expected status code.
///
/// # Panics
///
/// Panics if the status code does not match.
pub fn assert_status(response: &axum::response::Response<Body>, expected: StatusCode) {
    assert_eq!(
        response.status(),
        expected,
        "expected status {expected}, got {}",
        response.status()
    );
}

/// Parse the response body as JSON.
///
/// # Panics
///
/// Panics if the body cannot be read or parsed as JSON.
pub async fn parse_json(response: axum::response::Response<Body>) -> serde_json::Value {
    let status = response.status();
    let body = response.into_body();
    let bytes = body
        .collect()
        .await
        .expect("failed to read response body")
        .to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("failed to parse response JSON (status={status}): {e}\nbody: {text}"))
}
