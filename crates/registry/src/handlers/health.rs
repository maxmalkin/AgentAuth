//! Health check handlers.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

/// Health check response.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Status (healthy, unhealthy).
    pub status: String,
    /// Optional details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Liveness probe handler.
///
/// Returns 200 if the process is alive and not deadlocked.
/// Does NOT check external dependencies.
pub async fn liveness() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "healthy".to_string(),
            details: None,
        }),
    )
}

/// Readiness probe handler.
///
/// Returns 200 only when all required dependencies are reachable
/// and the instance is ready to serve traffic.
pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    // Check if we're marked as ready
    if !state.health.is_ready().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "not_ready".to_string(),
                details: Some("service is not ready".to_string()),
            }),
        );
    }

    // Check database
    if let Err(e) = state.db.check_primary_health().await {
        state.health.mark_not_ready().await;
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unhealthy".to_string(),
                details: Some(format!("database check failed: {e}")),
            }),
        );
    }
    state.health.update_db_check().await;

    // Check cache
    if let Err(e) = state.cache.health_check().await {
        state.health.mark_not_ready().await;
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "unhealthy".to_string(),
                details: Some(format!("cache check failed: {e}")),
            }),
        );
    }
    state.health.update_cache_check().await;

    (
        StatusCode::OK,
        Json(HealthResponse {
            status: "healthy".to_string(),
            details: None,
        }),
    )
}

/// Startup probe handler.
///
/// Returns 200 once one-time initialization is complete.
pub async fn startup(State(state): State<AppState>) -> impl IntoResponse {
    if state.health.is_started().await {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "started".to_string(),
                details: None,
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "starting".to_string(),
                details: Some("initialization in progress".to_string()),
            }),
        )
    }
}
