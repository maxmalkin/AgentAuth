//! Database connection pool.

use crate::config::DatabaseConfig;
use crate::error::{RegistryError, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::sync::Arc;

/// Database pool wrapper with primary and replica support.
#[derive(Clone)]
pub struct DbPool {
    /// Primary pool (for writes).
    primary: PgPool,
    /// Replica pools (for reads).
    replicas: Vec<PgPool>,
    /// Round-robin counter for replica selection.
    replica_counter: Arc<std::sync::atomic::AtomicUsize>,
}

impl DbPool {
    /// Create a new database pool from configuration.
    pub async fn new(config: &DatabaseConfig) -> Result<Self> {
        let primary = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.connect_timeout())
            .connect(&config.primary_url)
            .await?;

        let mut replicas = Vec::new();
        for url in &config.replica_urls {
            let pool = PgPoolOptions::new()
                .max_connections(config.max_connections)
                .acquire_timeout(config.connect_timeout())
                .connect(url)
                .await?;
            replicas.push(pool);
        }

        Ok(Self {
            primary,
            replicas,
            replica_counter: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    /// Get the primary pool for write operations.
    pub fn primary(&self) -> &PgPool {
        &self.primary
    }

    /// Get a read replica pool using round-robin selection.
    /// Falls back to primary if no replicas are configured.
    pub fn read_replica(&self) -> &PgPool {
        if self.replicas.is_empty() {
            return &self.primary;
        }

        let idx = self
            .replica_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % self.replicas.len();
        &self.replicas[idx]
    }

    /// Check if the primary database is healthy.
    pub async fn check_primary_health(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.primary)
            .await
            .map_err(RegistryError::Database)?;
        Ok(())
    }

    /// Check if replicas are healthy.
    pub async fn check_replica_health(&self) -> Result<()> {
        for replica in &self.replicas {
            sqlx::query("SELECT 1")
                .execute(replica)
                .await
                .map_err(RegistryError::Database)?;
        }
        Ok(())
    }

    /// Close all pools.
    pub async fn close(&self) {
        self.primary.close().await;
        for replica in &self.replicas {
            replica.close().await;
        }
    }
}
