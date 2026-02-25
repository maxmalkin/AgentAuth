//! AgentAuth Registry Service
//!
//! Full CRUD operations, KMS access, and token issuance.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

mod signing;

use agentauth_registry::{
    config::RegistryConfig,
    db::DbPool,
    middleware::{compression_layer, cors_layer, logging_middleware, request_id_middleware},
    routes::{create_metrics_router, create_router},
    services::{AuditService, CacheService, GrantService, TokenService},
    state::{AppState, HealthState},
};
use axum::middleware;
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
    let config = RegistryConfig::from_env().map_err(|e| {
        eprintln!("Failed to load configuration: {e}");
        e
    })?;

    // Initialize tracing
    init_tracing(&config.observability.log_level);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        service = "agentauth-registry",
        "Starting AgentAuth Registry Service"
    );

    // Create database pool
    let db = DbPool::new(&config.database).await.map_err(|e| {
        error!(error = %e, "Failed to create database pool");
        e
    })?;

    info!("Database pool created successfully");

    // Create cache service (Redis)
    let cache = Arc::new(CacheService::new(&config.redis).await.map_err(|e| {
        error!(error = %e, "Failed to create cache service");
        e
    })?);

    info!("Cache service (Redis) created successfully");

    // Create signing backend
    // TODO: In production, use KmsSigningBackend based on config.kms
    let signer = signing::create_signing_backend(&config.kms)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create signing backend");
            anyhow::anyhow!("Failed to create signing backend: {e}")
        })?;

    // Create services
    let audit = Arc::new(AuditService::new(db.clone(), signer.clone()));
    let grants = Arc::new(GrantService::new(db.clone(), config.grants.clone()));
    let tokens = Arc::new(TokenService::new(
        db.clone(),
        cache.clone(),
        signer.clone(),
        config.tokens.clone(),
    ));

    // Initialize audit service
    audit.initialize().await.map_err(|e| {
        error!(error = %e, "Failed to initialize audit service");
        e
    })?;

    info!("Services initialized successfully");

    // Create health state
    let health = Arc::new(HealthState::new());

    // Create app state
    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        cache,
        signer,
        tokens,
        grants,
        audit,
        health: health.clone(),
    };

    // Create routers
    let app = create_router(state)
        .layer(middleware::from_fn(logging_middleware))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(compression_layer())
        .layer(cors_layer());

    let metrics_app = create_metrics_router();

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
    health.mark_ready().await;

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
