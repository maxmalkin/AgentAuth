//! Verifier application state.

use crate::config::VerifierConfig;
use agentauth_registry::db::DbPool;
use agentauth_registry::services::CacheService;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared verifier application state.
#[derive(Clone)]
pub struct VerifierState {
    /// Configuration.
    pub config: Arc<VerifierConfig>,
    /// Cache service (Redis) - primary data source.
    pub cache: Arc<CacheService>,
    /// Database pool (read replica) - fallback only.
    pub db: DbPool,
    /// Health state.
    pub health: Arc<HealthState>,
}

/// Health state tracking for verifier.
pub struct HealthState {
    /// Whether the service is ready to accept traffic.
    pub ready: RwLock<bool>,
    /// Whether startup has completed.
    pub started: RwLock<bool>,
    /// Whether the cache (Redis) is ready.
    pub cache_ready: RwLock<bool>,
}

impl HealthState {
    /// Create new health state.
    pub fn new() -> Self {
        Self {
            ready: RwLock::new(false),
            started: RwLock::new(false),
            cache_ready: RwLock::new(false),
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
    #[allow(dead_code)] // Will be used when circuit breakers are implemented
    pub async fn mark_not_ready(&self) {
        *self.ready.write().await = false;
    }

    /// Mark cache as ready.
    pub async fn mark_cache_ready(&self) {
        *self.cache_ready.write().await = true;
    }

    /// Mark cache as not ready.
    pub async fn mark_cache_not_ready(&self) {
        *self.cache_ready.write().await = false;
        // If cache is not ready, service is not ready
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

    /// Check if cache is ready.
    pub async fn is_cache_ready(&self) -> bool {
        *self.cache_ready.read().await
    }
}

impl Default for HealthState {
    fn default() -> Self {
        Self::new()
    }
}
