//! HTTP middleware components.

use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::{info_span, Instrument};
use uuid::Uuid;

/// Request ID header name.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Add request ID to requests.
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    // Get or generate request ID
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map_or_else(|| Uuid::now_v7().to_string(), ToString::to_string);

    // Store request ID in extensions
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let mut response = next.run(request).await;

    // Add request ID to response
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }

    response
}

/// Request ID extension.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Request timing and logging middleware.
pub async fn logging_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();

    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_default();

    let span = info_span!(
        "http_request",
        method = %method,
        path = %path,
        request_id = %request_id,
        status = tracing::field::Empty,
        latency_ms = tracing::field::Empty,
    );

    let start = Instant::now();

    let response = next.run(request).instrument(span.clone()).await;

    let latency = start.elapsed();
    let status = response.status().as_u16();

    span.record("status", status);
    #[allow(clippy::cast_possible_truncation)]
    span.record("latency_ms", latency.as_millis() as u64);

    tracing::info!(
        target: "http",
        method = %method,
        path = %path,
        status = status,
        latency_ms = latency.as_millis(),
        request_id = %request_id,
        "request completed"
    );

    response
}

/// CORS middleware configuration.
///
/// In local dev the approval UI runs on a different port, so we must allow
/// its origin explicitly and enable credentials for CSRF cookie handling.
pub fn cors_layer() -> tower_http::cors::CorsLayer {
    tower_http::cors::CorsLayer::new()
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::HeaderName::from_static("x-request-id"),
            axum::http::header::HeaderName::from_static("x-csrf-token"),
            axum::http::header::HeaderName::from_static("agentdpop"),
        ])
        .allow_origin([
            "http://localhost:3001".parse().expect("valid origin"),
            "http://localhost:3000".parse().expect("valid origin"),
        ])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(3600))
}

/// Timeout layer for request handling.
pub fn timeout_layer(timeout_secs: u64) -> tower::timeout::TimeoutLayer {
    tower::timeout::TimeoutLayer::new(std::time::Duration::from_secs(timeout_secs))
}

/// Compression layer for responses.
pub fn compression_layer() -> tower_http::compression::CompressionLayer {
    tower_http::compression::CompressionLayer::new()
}

/// In-flight request limiter.
///
/// Returns 503 when too many requests are being processed concurrently.
#[derive(Clone)]
pub struct InFlightLimiter {
    max_requests: usize,
    current: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl InFlightLimiter {
    /// Create a new in-flight limiter.
    pub fn new(max_requests: usize) -> Self {
        Self {
            max_requests,
            current: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Check if we can accept a new request.
    pub fn try_acquire(&self) -> Option<InFlightGuard> {
        let current = self
            .current
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if current >= self.max_requests {
            self.current
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            None
        } else {
            Some(InFlightGuard {
                counter: self.current.clone(),
            })
        }
    }
}

/// Guard that decrements the counter on drop.
pub struct InFlightGuard {
    counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// In-flight limiting middleware.
pub async fn in_flight_limit_middleware(
    axum::extract::State(limiter): axum::extract::State<InFlightLimiter>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, &'static str)> {
    let _guard = limiter.try_acquire().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "too many concurrent requests",
    ))?;

    Ok(next.run(request).await)
}
