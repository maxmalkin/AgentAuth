//! Application state.

use crate::config::RegistryConfig;
use crate::db::DbPool;
use crate::services::{AuditService, CacheService, GrantService, TokenService};
use agentauth_core::crypto::SigningBackend;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Configuration.
    pub config: Arc<RegistryConfig>,
    /// Database pool.
    pub db: DbPool,
    /// Cache service (Redis).
    pub cache: Arc<CacheService>,
    /// Signing backend.
    pub signer: Arc<dyn SigningBackend>,
    /// Token service.
    pub tokens: Arc<TokenService>,
    /// Grant service.
    pub grants: Arc<GrantService>,
    /// Audit service.
    pub audit: Arc<AuditService>,
    /// Health state.
    pub health: Arc<HealthState>,
}

/// Health state tracking.
pub struct HealthState {
    /// Whether the service is ready to accept traffic.
    pub ready: RwLock<bool>,
    /// Whether startup has completed.
    pub started: RwLock<bool>,
    /// Last successful database check.
    pub last_db_check: RwLock<Option<std::time::Instant>>,
    /// Last successful cache check.
    pub last_cache_check: RwLock<Option<std::time::Instant>>,
}

impl HealthState {
    /// Create new health state.
    pub fn new() -> Self {
        Self {
            ready: RwLock::new(false),
            started: RwLock::new(false),
            last_db_check: RwLock::new(None),
            last_cache_check: RwLock::new(None),
        }
    }

    /// Mark the service as started.
    pub async fn mark_started(&self) {
        *self.started.write().await = true;
    }

    /// Mark the service as ready.
    pub async fn mark_ready(&self) {
        *self.ready.write().await = true;
    }

    /// Mark the service as not ready.
    pub async fn mark_not_ready(&self) {
        *self.ready.write().await = false;
    }

    /// Check if started.
    pub async fn is_started(&self) -> bool {
        *self.started.read().await
    }

    /// Check if ready.
    pub async fn is_ready(&self) -> bool {
        *self.ready.read().await
    }

    /// Update database check timestamp.
    pub async fn update_db_check(&self) {
        *self.last_db_check.write().await = Some(std::time::Instant::now());
    }

    /// Update cache check timestamp.
    pub async fn update_cache_check(&self) {
        *self.last_cache_check.write().await = Some(std::time::Instant::now());
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}
