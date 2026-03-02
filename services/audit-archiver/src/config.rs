//! Configuration for the audit archiver service.

use serde::Deserialize;

/// Top-level archiver configuration, loaded from environment variables
/// with the `ARCHIVER__` prefix.
#[derive(Debug, Clone, Deserialize)]
pub struct ArchiverConfig {
    /// Database connection settings.
    pub database: DatabaseConfig,
    /// Cold storage settings.
    pub storage: StorageConfig,
    /// Partition retention policy.
    #[serde(default)]
    pub retention: RetentionConfig,
    /// Observability settings.
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

/// Database configuration for the archiver.
/// The archiver only needs primary access (for DDL and reads before drop).
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// PostgreSQL primary connection URL.
    pub url: String,
    /// Maximum number of connections in the pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Connection timeout in seconds.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
}

/// Cold storage configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// Storage backend type.
    pub backend: StorageBackend,
    /// S3 bucket name (required when backend = s3).
    pub s3_bucket: Option<String>,
    /// S3 key prefix for archived partitions.
    #[serde(default = "default_s3_prefix")]
    pub s3_prefix: String,
    /// S3 endpoint URL override (for MinIO in local dev).
    pub s3_endpoint_url: Option<String>,
    /// S3 region.
    #[serde(default = "default_s3_region")]
    pub s3_region: String,
    /// Local filesystem path (required when backend = local_fs).
    pub local_path: Option<String>,
}

/// Which storage backend to use.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    /// AWS S3 or S3-compatible (MinIO).
    S3,
    /// Local filesystem (development only).
    LocalFs,
}

/// Partition retention policy.
#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    /// How many days to keep partitions in PostgreSQL.
    #[serde(default = "default_hot_retention")]
    pub hot_retention_days: u32,
    /// How many days in advance to create future partitions.
    #[serde(default = "default_advance_days")]
    pub advance_partition_days: u32,
}

/// Observability settings.
#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    /// Log level filter.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

impl ArchiverConfig {
    /// Loads configuration from environment variables with `ARCHIVER__` prefix.
    ///
    /// # Errors
    ///
    /// Returns an error if required variables are missing or invalid.
    pub fn from_env() -> std::result::Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(
                config::Environment::with_prefix("ARCHIVER")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()
    }
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            hot_retention_days: default_hot_retention(),
            advance_partition_days: default_advance_days(),
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
        }
    }
}

fn default_max_connections() -> u32 {
    4
}
fn default_connect_timeout() -> u64 {
    5
}
fn default_s3_prefix() -> String {
    "audit-archive/".to_string()
}
fn default_s3_region() -> String {
    "us-east-1".to_string()
}
fn default_hot_retention() -> u32 {
    90
}
fn default_advance_days() -> u32 {
    7
}
fn default_log_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_defaults() {
        let retention = RetentionConfig::default();
        assert_eq!(retention.hot_retention_days, 90);
        assert_eq!(retention.advance_partition_days, 7);
    }

    #[test]
    fn test_observability_defaults() {
        let obs = ObservabilityConfig::default();
        assert_eq!(obs.log_level, "info");
    }
}
