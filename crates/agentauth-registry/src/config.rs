//! Registry configuration.

use serde::Deserialize;
use std::time::Duration;

/// Registry service configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryConfig {
    /// Server configuration.
    pub server: ServerConfig,
    /// Database configuration.
    pub database: DatabaseConfig,
    /// Redis configuration.
    pub redis: RedisConfig,
    /// KMS configuration.
    pub kms: KmsConfig,
    /// Grant configuration.
    pub grants: GrantConfig,
    /// Token configuration.
    pub tokens: TokenConfig,
    /// Observability configuration.
    pub observability: ObservabilityConfig,
}

/// Server configuration.
#[derive(Debug, Clone, Deserialize)]
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
    /// TLS certificate path.
    pub tls_cert_path: Option<String>,
    /// TLS key path.
    pub tls_key_path: Option<String>,
    /// Graceful shutdown timeout in seconds.
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
    /// External URL (for discovery document).
    #[serde(default = "default_external_url")]
    pub external_url: String,
    /// Verifier service URL.
    #[serde(default = "default_verifier_url")]
    pub verifier_url: String,
    /// Approval UI URL.
    #[serde(default = "default_approval_ui_url")]
    pub approval_ui_url: String,
}

/// Database configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Primary database URL.
    pub primary_url: String,
    /// Read replica URLs.
    #[serde(default)]
    pub replica_urls: Vec<String>,
    /// Maximum connections per pool.
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    /// Connection timeout in seconds.
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    /// Query timeout in seconds.
    #[serde(default = "default_query_timeout")]
    pub query_timeout_secs: u64,
}

/// Redis configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    /// Redis cluster URLs.
    pub urls: Vec<String>,
    /// Connection timeout in seconds.
    #[serde(default = "default_redis_timeout")]
    pub timeout_secs: u64,
    /// Token cache database/prefix.
    #[serde(default = "default_token_cache_prefix")]
    pub token_cache_prefix: String,
    /// Nonce store database/prefix.
    #[serde(default = "default_nonce_store_prefix")]
    pub nonce_store_prefix: String,
    /// Rate limit database/prefix.
    #[serde(default = "default_rate_limit_prefix")]
    pub rate_limit_prefix: String,
}

/// KMS configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct KmsConfig {
    /// KMS backend type.
    pub backend: KmsBackend,
    /// Key ID for signing.
    pub signing_key_id: String,
    /// Operation timeout in seconds.
    #[serde(default = "default_kms_timeout")]
    pub timeout_secs: u64,
}

/// KMS backend type.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KmsBackend {
    /// AWS KMS.
    AwsKms {
        /// AWS region.
        region: String,
    },
    /// GCP Cloud KMS.
    GcpKms {
        /// Project ID.
        project_id: String,
        /// Location.
        location: String,
        /// Key ring.
        key_ring: String,
    },
    /// HashiCorp Vault Transit.
    VaultTransit {
        /// Vault address.
        address: String,
        /// Mount path.
        mount: String,
    },
    /// Encrypted keyfile (development only).
    EncryptedKeyfile {
        /// Path to the encrypted keyfile.
        path: String,
    },
}

/// Grant configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct GrantConfig {
    /// Maximum pending grants per agent.
    #[serde(default = "default_max_pending_grants")]
    pub max_pending_per_agent: u32,
    /// Grant request expiry in seconds.
    #[serde(default = "default_grant_expiry")]
    pub expiry_secs: u64,
    /// Cooldown multiplier after denial.
    #[serde(default = "default_cooldown_multiplier")]
    pub cooldown_multiplier: f64,
    /// Initial cooldown in seconds.
    #[serde(default = "default_initial_cooldown")]
    pub initial_cooldown_secs: u64,
    /// Maximum cooldown in seconds.
    #[serde(default = "default_max_cooldown")]
    pub max_cooldown_secs: u64,
    /// Maximum requests per minute (for behavioral limits).
    #[serde(default = "default_max_requests_per_minute")]
    pub max_requests_per_minute: u32,
    /// Maximum burst (for behavioral limits).
    #[serde(default = "default_max_burst")]
    pub max_burst: u32,
}

/// Token configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenConfig {
    /// Token lifetime in seconds.
    #[serde(default = "default_token_lifetime")]
    pub lifetime_secs: u64,
    /// Idempotency window in seconds.
    #[serde(default = "default_idempotency_window")]
    pub idempotency_window_secs: u64,
    /// Revocation propagation timeout in milliseconds.
    #[serde(default = "default_revocation_propagation")]
    pub revocation_propagation_ms: u64,
}

/// Observability configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ObservabilityConfig {
    /// OTLP endpoint.
    pub otlp_endpoint: Option<String>,
    /// Service name.
    #[serde(default = "default_service_name")]
    pub service_name: String,
    /// Log level.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

// Default value functions
fn default_host() -> String {
    "0.0.0.0".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_metrics_port() -> u16 {
    9090
}
fn default_shutdown_timeout() -> u64 {
    20
}
fn default_external_url() -> String {
    "http://localhost:8080".to_string()
}
fn default_verifier_url() -> String {
    "http://localhost:8081".to_string()
}
fn default_approval_ui_url() -> String {
    "http://localhost:3000".to_string()
}
fn default_max_connections() -> u32 {
    16
}
fn default_connect_timeout() -> u64 {
    5
}
fn default_query_timeout() -> u64 {
    5
}
fn default_redis_timeout() -> u64 {
    2
}
fn default_token_cache_prefix() -> String {
    "token:".to_string()
}
fn default_nonce_store_prefix() -> String {
    "nonce:".to_string()
}
fn default_rate_limit_prefix() -> String {
    "ratelimit:".to_string()
}
fn default_kms_timeout() -> u64 {
    10
}
fn default_max_pending_grants() -> u32 {
    5
}
fn default_grant_expiry() -> u64 {
    3600
}
fn default_cooldown_multiplier() -> f64 {
    4.0
}
fn default_initial_cooldown() -> u64 {
    3600
}
fn default_max_cooldown() -> u64 {
    86400
}
fn default_max_requests_per_minute() -> u32 {
    60
}
fn default_max_burst() -> u32 {
    10
}
fn default_token_lifetime() -> u64 {
    900
}
fn default_idempotency_window() -> u64 {
    900
}
fn default_revocation_propagation() -> u64 {
    100
}
fn default_service_name() -> String {
    "agentauth-registry".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}

impl RegistryConfig {
    /// Load configuration from environment.
    pub fn from_env() -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::Environment::with_prefix("AGENTAUTH").separator("__"))
            .build()?
            .try_deserialize()
    }
}

impl ServerConfig {
    /// Get shutdown timeout as Duration.
    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_secs)
    }
}

impl DatabaseConfig {
    /// Get connect timeout as Duration.
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.connect_timeout_secs)
    }

    /// Get query timeout as Duration.
    pub fn query_timeout(&self) -> Duration {
        Duration::from_secs(self.query_timeout_secs)
    }
}

impl RedisConfig {
    /// Get timeout as Duration.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

impl TokenConfig {
    /// Get token lifetime as Duration.
    pub fn lifetime(&self) -> Duration {
        Duration::from_secs(self.lifetime_secs)
    }

    /// Get idempotency window as Duration.
    pub fn idempotency_window(&self) -> Duration {
        Duration::from_secs(self.idempotency_window_secs)
    }
}
