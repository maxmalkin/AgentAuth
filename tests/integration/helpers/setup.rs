//! Test infrastructure for integration tests.
//!
//! Provides `TestApp` which creates in-process Axum routers backed by
//! real PostgreSQL and Redis from docker-compose.

use auth_core::crypto::{Ed25519PublicKey, Signature, SigningBackend};
use auth_core::error::CryptoError;
use axum::Router;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use registry::config::{
    DatabaseConfig, GrantConfig, KmsBackend, KmsConfig, ObservabilityConfig, RedisConfig,
    RegistryConfig, ServerConfig, TokenConfig,
};
use registry::db::DbPool;
use registry::routes::create_router;
use registry::services::{AuditService, CacheService, GrantService, TokenService};
use registry::state::{AppState, HealthState};
use sqlx::PgPool;
use std::sync::Arc;

// Re-export for convenience in tests.
pub use axum::body::Body;
pub use http_body_util::BodyExt;
pub use hyper::Request;
pub use tower::ServiceExt;

/// Test signing backend using an in-memory Ed25519 key.
/// Only used in integration tests — never in production.
pub struct TestSigningBackend {
    signing_key: SigningKey,
    key_id: String,
}

impl TestSigningBackend {
    /// Create a new test signing backend with a random key.
    pub fn new() -> Self {
        Self {
            signing_key: SigningKey::generate(&mut OsRng),
            key_id: format!("test-key-{}", uuid::Uuid::now_v7()),
        }
    }

    /// Sign raw bytes with this backend's key (sync convenience for test factories).
    pub fn sign_bytes(&self, message: &[u8]) -> [u8; 64] {
        let sig = self.signing_key.sign(message);
        sig.to_bytes()
    }

    /// Get the public key bytes.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
}

#[async_trait::async_trait]
impl SigningBackend for TestSigningBackend {
    async fn sign(&self, message: &[u8]) -> Result<Signature, CryptoError> {
        let sig = self.signing_key.sign(message);
        Signature::from_bytes(&sig.to_bytes())
    }

    async fn public_key(&self) -> Result<Ed25519PublicKey, CryptoError> {
        Ed25519PublicKey::from_bytes(&self.signing_key.verifying_key().to_bytes())
    }

    fn key_id(&self) -> &str {
        &self.key_id
    }
}

/// Integration test application with in-process routers.
pub struct TestApp {
    /// Registry router for in-process requests.
    pub registry_router: Router,
    /// Verifier router for in-process requests.
    pub verifier_router: Router,
    /// Direct database pool for test setup/assertions.
    pub db_pool: PgPool,
    /// The signing backend (for creating signed test data).
    pub signer: Arc<TestSigningBackend>,
}

impl TestApp {
    /// Create a new test app connected to docker-compose services.
    ///
    /// # Panics
    ///
    /// Panics if database or Redis connection fails (test infrastructure issue).
    pub async fn new() -> Self {
        // Initialize tracing (only once, ignore errors on subsequent calls)
        let _ = tracing_subscriber::fmt()
            .with_env_filter("warn")
            .with_test_writer()
            .try_init();

        let db_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://agentauth:agentauth@localhost:5434/agentauth".into());
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6399".into());

        // Connect to PostgreSQL and run migrations
        let db_pool = PgPool::connect(&db_url)
            .await
            .expect("failed to connect to test database");

        sqlx::migrate!("../../migrations")
            .run(&db_pool)
            .await
            .expect("failed to run migrations");

        // Ensure audit_events partition exists for the current month.
        // The base migration only creates 2025-01 and 2025-02 partitions.
        // Multiple test processes may race to create the same partition, so
        // we ignore the 42P07 (duplicate_table) error.
        let now = chrono::Utc::now();
        let partition_name = format!("audit_events_{}_{:02}", now.format("%Y"), now.format("%m"));
        let next_month = now + chrono::Duration::days(32);
        let start = format!("{}-{:02}-01", now.format("%Y"), now.format("%m"));
        let end = format!(
            "{}-{:02}-01",
            next_month.format("%Y"),
            next_month.format("%m")
        );
        let create_partition = format!(
            "CREATE TABLE {partition_name} PARTITION OF audit_events \
             FOR VALUES FROM ('{start}') TO ('{end}')"
        );
        match sqlx::query(&create_partition).execute(&db_pool).await {
            Ok(_) => {}
            Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("42P07") => {
                // Partition already exists (concurrent test or previous run) — safe to ignore.
            }
            Err(e) => panic!("failed to create audit partition for current month: {e}"),
        }

        // Build registry config with test defaults
        let config = RegistryConfig {
            server: ServerConfig {
                host: "127.0.0.1".into(),
                port: 0,
                metrics_port: 0,
                tls_cert_path: None,
                tls_key_path: None,
                shutdown_timeout_secs: 5,
                external_url: "http://localhost:8080".into(),
                verifier_url: "http://localhost:8081".into(),
                approval_ui_url: "http://localhost:3000".into(),
            },
            database: DatabaseConfig {
                primary_url: db_url.clone(),
                replica_urls: vec![],
                max_connections: 5,
                connect_timeout_secs: 5,
                query_timeout_secs: 5,
            },
            redis: RedisConfig {
                urls: vec![redis_url.clone()],
                timeout_secs: 2,
                token_cache_prefix: format!("test_{}:token:", uuid::Uuid::now_v7()),
                nonce_store_prefix: format!("test_{}:nonce:", uuid::Uuid::now_v7()),
                rate_limit_prefix: format!("test_{}:rl:", uuid::Uuid::now_v7()),
            },
            kms: KmsConfig {
                backend: KmsBackend::EncryptedKeyfile {
                    path: "/dev/null".into(),
                },
                signing_key_id: "test-key".into(),
                timeout_secs: 5,
            },
            grants: GrantConfig {
                max_pending_per_agent: 5,
                expiry_secs: 3600,
                cooldown_multiplier: 4.0,
                initial_cooldown_secs: 3600,
                max_cooldown_secs: 86400,
                max_requests_per_minute: 60,
                max_burst: 10,
            },
            tokens: TokenConfig {
                lifetime_secs: 900,
                idempotency_window_secs: 900,
                revocation_propagation_ms: 100,
            },
            observability: ObservabilityConfig {
                otlp_endpoint: None,
                service_name: "test-registry".into(),
                log_level: "warn".into(),
            },
        };

        // Build services
        let db = DbPool::new(&config.database)
            .await
            .expect("failed to create DB pool");

        let cache = Arc::new(
            CacheService::new(&config.redis)
                .await
                .expect("failed to connect to Redis"),
        );

        let signer = Arc::new(TestSigningBackend::new());
        let signer_backend: Arc<dyn SigningBackend> = signer.clone();

        let tokens = Arc::new(TokenService::new(
            db.clone(),
            cache.clone(),
            signer_backend.clone(),
            config.tokens.clone(),
        ));

        let grants = Arc::new(GrantService::new(db.clone(), config.grants.clone()));

        let audit = Arc::new(AuditService::new(db.clone(), signer_backend.clone()));

        let health = Arc::new(HealthState::new());
        health.mark_started().await;
        health.mark_ready().await;

        let state = AppState {
            config: Arc::new(config.clone()),
            db: db.clone(),
            cache: cache.clone(),
            signer: signer_backend,
            tokens,
            grants,
            audit,
            health,
        };

        let registry_router = create_router(state);

        // Build verifier router (replicated from services/verifier since
        // VerifierState is in the binary crate and not importable).
        let verifier_router = build_verifier_router(db, cache, config);

        Self {
            registry_router,
            verifier_router,
            db_pool,
            signer,
        }
    }

    /// Send a request to the registry router and return the response.
    pub async fn registry_request(&self, request: Request<Body>) -> axum::response::Response<Body> {
        self.registry_router
            .clone()
            .oneshot(request)
            .await
            .expect("registry request failed")
    }

    /// Send a request to the verifier router and return the response.
    pub async fn verifier_request(&self, request: Request<Body>) -> axum::response::Response<Body> {
        self.verifier_router
            .clone()
            .oneshot(request)
            .await
            .expect("verifier request failed")
    }
}

/// Build a verifier-like router for testing.
///
/// We replicate the verifier router here because the verifier's `VerifierState`
/// type is defined in the binary crate (`services/verifier/`) which cannot be
/// depended on from a library test crate.
fn build_verifier_router(db: DbPool, cache: Arc<CacheService>, _config: RegistryConfig) -> Router {
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum::Json;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    /// Minimal verifier state for testing.
    #[derive(Clone)]
    struct TestVerifierState {
        cache: Arc<CacheService>,
        db: DbPool,
        nonce_ttl_secs: u64,
        max_clock_skew_secs: i64,
    }

    #[derive(Debug, Deserialize)]
    struct VerifyRequest {
        jti: uuid::Uuid,
        service_provider_id: uuid::Uuid,
        nonce: String,
        #[allow(dead_code)]
        dpop_proof: Option<String>,
        #[allow(dead_code)]
        dpop_thumbprint: Option<String>,
    }

    #[derive(Debug, Serialize)]
    struct VerifyResponse {
        valid: bool,
        outcome: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        agent_id: Option<uuid::Uuid>,
        #[serde(skip_serializing_if = "Option::is_none")]
        granted_capabilities: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        behavioral_envelope: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        remaining_lifetime_secs: Option<i64>,
    }

    /// Token verification handler (mirrors services/verifier logic).
    async fn verify_token(
        State(state): State<TestVerifierState>,
        Json(req): Json<VerifyRequest>,
    ) -> impl IntoResponse {
        let token_id = auth_core::TokenId::from_uuid(req.jti);

        // Step 1: Nonce replay check
        let nonce_ttl = Duration::from_secs(state.nonce_ttl_secs);
        match state.cache.check_and_set_nonce(&req.nonce, nonce_ttl).await {
            Ok(true) => {
                return (
                    StatusCode::OK,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "nonce_replay".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            Ok(false) => {}
            Err(_) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "internal_error".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
        }

        // Step 2: Check cache for revocation + SP binding
        let cached = state.cache.get_cached_token(&token_id).await.ok().flatten();

        if let Some(ref c) = cached {
            if c.is_revoked {
                return (
                    StatusCode::OK,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "revoked".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            if c.service_provider_id != req.service_provider_id.to_string() {
                return (
                    StatusCode::OK,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "service_provider_mismatch".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
        }

        // Fall back to DB
        let token_row = match registry::db::get_token(state.db.read_replica(), &token_id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                return (
                    StatusCode::OK,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "not_found".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            Err(_) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "internal_error".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
        };

        if cached.is_none() {
            // Check revocation from DB
            if token_row.is_revoked {
                return (
                    StatusCode::OK,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "revoked".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            if token_row.service_provider_id != req.service_provider_id {
                return (
                    StatusCode::OK,
                    Json(VerifyResponse {
                        valid: false,
                        outcome: "service_provider_mismatch".into(),
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            // Cache for future requests
            let _ = state
                .cache
                .cache_token(
                    &token_id,
                    &token_row.service_provider_id.to_string(),
                    token_row.expires_at.timestamp(),
                    token_row.is_revoked,
                )
                .await;
        }

        // Step 6: Expiry check
        let now = chrono::Utc::now();
        let clock_skew = chrono::Duration::seconds(state.max_clock_skew_secs);
        if token_row.expires_at + clock_skew < now {
            return (
                StatusCode::OK,
                Json(VerifyResponse {
                    valid: false,
                    outcome: "expired".into(),
                    agent_id: None,
                    granted_capabilities: None,
                    behavioral_envelope: None,
                    remaining_lifetime_secs: None,
                }),
            );
        }

        let remaining = (token_row.expires_at - now).num_seconds();
        (
            StatusCode::OK,
            Json(VerifyResponse {
                valid: true,
                outcome: "allowed".into(),
                agent_id: Some(token_row.agent_id),
                granted_capabilities: Some(token_row.granted_capabilities),
                behavioral_envelope: Some(token_row.behavioral_envelope),
                remaining_lifetime_secs: Some(remaining),
            }),
        )
    }

    async fn live() -> StatusCode {
        StatusCode::OK
    }

    let verifier_state = TestVerifierState {
        cache,
        db,
        nonce_ttl_secs: 900,
        max_clock_skew_secs: 30,
    };

    Router::new()
        .route("/v1/tokens/verify", post(verify_token))
        .route("/health/live", get(live))
        .with_state(verifier_state)
}

/// Seed a human principal into the database for test use.
pub async fn seed_human_principal(pool: &PgPool, id: uuid::Uuid) {
    sqlx::query(
        "INSERT INTO human_principals (id, email) \
         VALUES ($1, $2) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(format!("test-{}@example.com", id))
    .execute(pool)
    .await
    .expect("failed to seed human principal");
}

/// Seed a service provider into the database for test use.
pub async fn seed_service_provider(pool: &PgPool, id: uuid::Uuid) {
    sqlx::query(
        "INSERT INTO service_providers (id, name, verification_endpoint, public_key) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(id)
    .bind(format!("Test SP {}", id))
    .bind(format!("https://sp-{}.example.com/verify", id))
    .bind(vec![0u8; 32]) // Placeholder public key
    .execute(pool)
    .await
    .expect("failed to seed service provider");
}
