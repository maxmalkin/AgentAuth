//! Verifier service configuration.

use agentauth_registry::config::{DatabaseConfig, ObservabilityConfig, RedisConfig};
use serde::Deserialize;

/// Verifier service configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct VerifierConfig {
    /// Server configuration.
    pub server: ServerConfig,
    /// Database configuration (read replica only).
    pub database: DatabaseConfig,
    /// Redis configuration.
    pub redis: RedisConfig,
    /// Observability configuration.
    pub observability: ObservabilityConfig,
    /// Verification configuration.
    #[serde(default)]
    pub verification: VerificationConfig,
}

/// Server configuration for verifier.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)] // shutdown_timeout_secs will be used when graceful shutdown is wired up
pub struct ServerConfig {
    /// Host to bind to.
    #[serde(default = "default_host")]
    pub host: String,
    /// Port to bind to.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Metrics port (separate from API port).
    #[serde(default = "default_metrics_port")]
    pub metrics_port: u16,
    /// Graceful shutdown timeout in seconds.
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
}

/// Verification configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct VerificationConfig {
    /// Nonce TTL in seconds.
    #[serde(default = "default_nonce_ttl")]
    pub nonce_ttl_secs: u64,
    /// Maximum clock skew allowed in seconds.
    #[serde(default = "default_clock_skew")]
    pub max_clock_skew_secs: i64,
    /// Whether to require DPoP proof.
    #[serde(default = "default_require_dpop")]
    pub require_dpop: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            nonce_ttl_secs: default_nonce_ttl(),
            max_clock_skew_secs: default_clock_skew(),
            require_dpop: default_require_dpop(),
        }
    }
}

// Default value functions
fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8081
}

fn default_metrics_port() -> u16 {
    9091
}

fn default_shutdown_timeout() -> u64 {
    20
}

fn default_nonce_ttl() -> u64 {
    900 // 15 minutes - same as token lifetime
}

fn default_clock_skew() -> i64 {
    30 // 30 seconds
}

fn default_require_dpop() -> bool {
    true
}

impl VerifierConfig {
    /// Load configuration from environment.
    pub fn from_env() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::Environment::with_prefix("AGENTAUTH_VERIFIER").separator("__"))
            .build()?
            .try_deserialize()
    }
}
