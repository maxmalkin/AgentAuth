//! Route definitions for the registry service.

use crate::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

/// Create the main API router.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health endpoints
        .route("/health/live", get(handlers::liveness))
        .route("/health/ready", get(handlers::readiness))
        .route("/health/startup", get(handlers::startup))
        // Discovery endpoints
        .route(
            "/.well-known/agentauth",
            get(handlers::get_discovery_document),
        )
        .route("/.well-known/agentauth/keys", get(handlers::get_keys))
        // Agent endpoints
        .route("/v1/agents", get(handlers::list_agents))
        .route("/v1/agents/register", post(handlers::register_agent))
        .route("/v1/agents/bootstrap", post(handlers::bootstrap_agent))
        .route("/v1/agents/:agent_id", get(handlers::get_agent))
        .route("/v1/agents/:agent_id", delete(handlers::delete_agent))
        // Grant endpoints
        .route("/v1/grants/request", post(handlers::request_grant))
        .route("/v1/grants/:grant_id", get(handlers::get_grant))
        .route(
            "/v1/grants/:grant_id/approve",
            post(handlers::approve_grant),
        )
        .route("/v1/grants/:grant_id/deny", post(handlers::deny_grant))
        .route("/v1/grants/:grant_id/revoke", post(handlers::revoke_grant))
        // Token endpoints
        .route("/v1/tokens/issue", post(handlers::issue_token))
        .route("/v1/tokens/revoke", post(handlers::revoke_token))
        // Audit endpoints
        .route("/v1/audit/:agent_id", get(handlers::get_agent_audit_log))
        .route(
            "/v1/audit/:agent_id/verify",
            get(handlers::verify_agent_audit_chain),
        )
        .route("/v1/audit/record", post(handlers::record_audit_event))
        // Attach state
        .with_state(state)
}

/// Create the metrics router (separate port).
pub fn create_metrics_router() -> Router {
    Router::new().route("/metrics", get(metrics_handler))
}

/// Prometheus metrics handler.
async fn metrics_handler() -> String {
    // In a full implementation, we'd use the metrics-exporter-prometheus crate
    // to export metrics in Prometheus format
    "# Metrics endpoint\n".to_string()
}
