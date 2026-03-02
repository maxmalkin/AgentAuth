//! AgentAuth Verifier Service
//!
//! Read-only token verification, designed for high replica count.
//! This service handles POST /v1/tokens/verify with strict check ordering:
//! 1. Check nonce (Redis) — reject replay immediately
//! 2. Check revocation (Redis) — reject revoked tokens
//! 3. Verify `cnf` token binding if present
//! 4. Verify DPoP proof signature
//! 5. Verify token signature using `key_id`
//! 6. Check expiry last

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

mod config;
mod handlers;
mod state;

use crate::config::VerifierConfig;
use crate::handlers::{get_keys, liveness, metrics_handler, readiness, startup, verify_token};
use crate::state::{HealthState, VerifierState};
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use registry::services::CacheService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Load configuration
    let config = VerifierConfig::from_env().map_err(|e| {
        eprintln!("Failed to load configuration: {e}");
        e
    })?;

    // Initialize tracing
    init_tracing(&config.observability.log_level);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        service = "agentauth-verifier",
        "Starting AgentAuth Verifier Service"
    );

    // Create cache service (Redis) - this is the primary data source for verifier
    let cache = Arc::new(CacheService::new(&config.redis).await.map_err(|e| {
        error!(error = %e, "Failed to create cache service");
        e
    })?);

    info!("Cache service (Redis) created successfully");

    // Create database pool for read replica fallback
    let db = registry::db::DbPool::new(&config.database)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create database pool");
            e
        })?;

    info!("Database pool (read replica) created successfully");

    // Create health state
    let health = Arc::new(HealthState::new());

    // Create verifier state
    let state = VerifierState {
        config: Arc::new(config.clone()),
        cache,
        db,
        health: health.clone(),
    };

    // Warm up cache by checking Redis connectivity
    if let Err(e) = state.cache.health_check().await {
        error!(error = %e, "Redis health check failed during startup");
        // Don't mark as ready until Redis is available
    } else {
        health.mark_cache_ready().await;
        info!("Redis cache is ready");
    }

    // Create API router
    let app = Router::new()
        // Token verification endpoint
        .route("/v1/tokens/verify", post(verify_token))
        // Keys endpoint (read-only)
        .route("/.well-known/agentauth/keys", get(get_keys))
        // Health endpoints
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .route("/health/startup", get(startup))
        .with_state(state.clone())
        .layer(middleware::from_fn(
            registry::middleware::logging_middleware,
        ))
        .layer(middleware::from_fn(
            registry::middleware::request_id_middleware,
        ))
        .layer(registry::middleware::compression_layer())
        .layer(registry::middleware::cors_layer());

    // Create metrics router (separate port)
    let metrics_app = Router::new().route("/metrics", get(metrics_handler));

    // Bind listeners
    let api_addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid API address: {e}"))?;

    let metrics_addr: SocketAddr = format!("{}:{}", config.server.host, config.server.metrics_port)
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid metrics address: {e}"))?;

    let api_listener = TcpListener::bind(api_addr).await?;
    let metrics_listener = TcpListener::bind(metrics_addr).await?;

    info!(
        api_addr = %api_addr,
        metrics_addr = %metrics_addr,
        "Server listening"
    );

    // Mark service as started
    health.mark_started().await;

    // Only mark ready if cache is ready
    if health.is_cache_ready().await {
        health.mark_ready().await;
    }

    // Create shutdown signal
    let shutdown = shutdown_signal();

    // Run servers
    tokio::select! {
        result = axum::serve(api_listener, app).with_graceful_shutdown(shutdown) => {
            if let Err(e) = result {
                error!(error = %e, "API server error");
            }
        }
        result = axum::serve(metrics_listener, metrics_app) => {
            if let Err(e) = result {
                error!(error = %e, "Metrics server error");
            }
        }
    }

    info!("Server shutdown complete");
    Ok(())
}

/// Initialize tracing subscriber.
fn init_tracing(log_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .init();
}

/// Create shutdown signal handler.
#[allow(clippy::expect_used)] // Signal handler installation failing is a fatal error
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("Received Ctrl+C, starting graceful shutdown");
        }
        () = terminate => {
            info!("Received SIGTERM, starting graceful shutdown");
        }
    }
}
