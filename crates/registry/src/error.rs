//! Registry error types.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

/// Registry service errors.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Agent not found.
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    /// Grant not found.
    #[error("grant not found: {0}")]
    GrantNotFound(String),

    /// Grant not approved (cannot issue token).
    #[error("grant not approved: {0}")]
    GrantNotApproved(String),

    /// Token not found.
    #[error("token not found: {0}")]
    TokenNotFound(String),

    /// Invalid manifest.
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    /// Invalid capability.
    #[error("invalid capability: {0}")]
    InvalidCapability(String),

    /// Invalid approval assertion.
    #[error("invalid approval assertion: {0}")]
    InvalidApprovalAssertion(String),

    /// Grant already exists (idempotency).
    #[error("grant already exists")]
    GrantAlreadyExists,

    /// Grant not pending.
    #[error("grant is not in pending state")]
    GrantNotPending,

    /// Too many pending grants.
    #[error("too many pending grants for this agent")]
    TooManyPendingGrants,

    /// Grant expired.
    #[error("grant has expired")]
    GrantExpired,

    /// Token already revoked.
    #[error("token is already revoked")]
    TokenAlreadyRevoked,

    /// Agent already registered.
    #[error("agent already registered")]
    AgentAlreadyRegistered,

    /// OTP already used.
    #[error("bootstrap token already used")]
    OtpAlreadyUsed,

    /// OTP invalid.
    #[error("invalid bootstrap token")]
    OtpInvalid,

    /// Signature verification failed.
    #[error("signature verification failed: {0}")]
    SignatureVerificationFailed(String),

    /// Database error.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Redis error.
    #[error("cache error: {0}")]
    Cache(String),

    /// Crypto error.
    #[error("crypto error: {0}")]
    Crypto(#[from] auth_core::CoreError),

    /// Rate limited.
    #[error("rate limited")]
    RateLimited,

    /// Cooldown active.
    #[error("cooldown active, retry after {0} seconds")]
    CooldownActive(u64),

    /// Service unavailable.
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Error response body.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Error code.
    pub error: String,
    /// Human-readable message.
    pub message: String,
    /// Optional retry-after seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

impl IntoResponse for RegistryError {
    fn into_response(self) -> Response {
        let (status, error_code, retry_after) = match &self {
            Self::AgentNotFound(_) => (StatusCode::NOT_FOUND, "agent_not_found", None),
            Self::GrantNotFound(_) => (StatusCode::NOT_FOUND, "grant_not_found", None),
            Self::GrantNotApproved(_) => (StatusCode::CONFLICT, "grant_not_approved", None),
            Self::TokenNotFound(_) => (StatusCode::NOT_FOUND, "token_not_found", None),
            Self::InvalidManifest(_) => (StatusCode::BAD_REQUEST, "invalid_manifest", None),
            Self::InvalidCapability(_) => (StatusCode::BAD_REQUEST, "invalid_capability", None),
            Self::InvalidApprovalAssertion(_) => {
                (StatusCode::BAD_REQUEST, "invalid_approval_assertion", None)
            }
            Self::GrantAlreadyExists => (StatusCode::CONFLICT, "grant_already_exists", None),
            Self::GrantNotPending => (StatusCode::CONFLICT, "grant_not_pending", None),
            Self::TooManyPendingGrants => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_pending_grants",
                None,
            ),
            Self::GrantExpired => (StatusCode::GONE, "grant_expired", None),
            Self::TokenAlreadyRevoked => (StatusCode::CONFLICT, "token_already_revoked", None),
            Self::AgentAlreadyRegistered => {
                (StatusCode::CONFLICT, "agent_already_registered", None)
            }
            Self::OtpAlreadyUsed => (StatusCode::CONFLICT, "otp_already_used", None),
            Self::OtpInvalid => (StatusCode::UNAUTHORIZED, "otp_invalid", None),
            Self::SignatureVerificationFailed(_) => (
                StatusCode::UNAUTHORIZED,
                "signature_verification_failed",
                None,
            ),
            Self::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "database_error", None),
            Self::Cache(_) => (StatusCode::INTERNAL_SERVER_ERROR, "cache_error", None),
            Self::Crypto(_) => (StatusCode::INTERNAL_SERVER_ERROR, "crypto_error", None),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", Some(60)),
            Self::CooldownActive(secs) => (
                StatusCode::TOO_MANY_REQUESTS,
                "cooldown_active",
                Some(*secs),
            ),
            Self::ServiceUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "service_unavailable", None)
            }
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", None),
        };

        let body = ErrorResponse {
            error: error_code.to_string(),
            message: self.to_string(),
            retry_after,
        };

        (status, Json(body)).into_response()
    }
}

/// Result type for registry operations.
pub type Result<T> = std::result::Result<T, RegistryError>;
