//! Verifier request handlers.

use crate::state::VerifierState;
use auth_core::TokenId;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::Utc;
use registry::db;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

// ============================================================================
// Token Verification
// ============================================================================

/// Token verification request.
#[derive(Debug, Deserialize)]
pub struct VerifyTokenRequest {
    /// The token JTI to verify.
    pub jti: Uuid,
    /// The service provider ID that received the token.
    pub service_provider_id: Uuid,
    /// The nonce for replay protection.
    pub nonce: String,
    /// Optional DPoP proof.
    pub dpop_proof: Option<String>,
    /// Optional DPoP public key thumbprint (for binding verification).
    pub dpop_thumbprint: Option<String>,
}

/// Token verification response.
#[derive(Debug, Serialize)]
pub struct VerifyTokenResponse {
    /// Whether the token is valid.
    pub valid: bool,
    /// Verification outcome.
    pub outcome: VerificationOutcome,
    /// Agent ID (if valid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    /// Granted capabilities (if valid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granted_capabilities: Option<serde_json::Value>,
    /// Behavioral envelope (if valid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavioral_envelope: Option<serde_json::Value>,
    /// Remaining token lifetime in seconds (if valid).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_lifetime_secs: Option<i64>,
}

/// Verification outcome.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // InvalidSignature will be used when signature verification is implemented
pub enum VerificationOutcome {
    /// Token is valid and allowed.
    Allowed,
    /// Nonce replay detected.
    NonceReplay,
    /// Token has been revoked.
    Revoked,
    /// Token binding mismatch.
    BindingMismatch,
    /// Invalid DPoP proof.
    InvalidDpopProof,
    /// Invalid token signature.
    InvalidSignature,
    /// Token has expired.
    Expired,
    /// Token not found.
    NotFound,
    /// Service provider mismatch.
    ServiceProviderMismatch,
    /// Internal error.
    InternalError,
}

/// Verify a token.
///
/// POST /v1/tokens/verify
///
/// Strict check ordering:
/// 1. Check nonce (Redis) — reject replay immediately
/// 2. Check revocation (Redis) — reject revoked tokens
/// 3. Verify `cnf` token binding if present
/// 4. Verify DPoP proof signature
/// 5. Verify token signature using `key_id`
/// 6. Check expiry last
#[allow(clippy::too_many_lines)] // Complex verification logic requires many steps
pub async fn verify_token(
    State(state): State<VerifierState>,
    Json(req): Json<VerifyTokenRequest>,
) -> impl IntoResponse {
    let token_id = TokenId::from_uuid(req.jti);
    let start = std::time::Instant::now();

    // Step 1: Check nonce for replay (Redis)
    // This is the first check because replay attacks are the most common
    let nonce_ttl = Duration::from_secs(state.config.verification.nonce_ttl_secs);
    match state.cache.check_and_set_nonce(&req.nonce, nonce_ttl).await {
        Ok(true) => {
            // Nonce was already used - replay detected
            warn!(
                jti = %req.jti,
                nonce = %req.nonce,
                "Nonce replay detected"
            );
            return (
                StatusCode::OK,
                Json(VerifyTokenResponse {
                    valid: false,
                    outcome: VerificationOutcome::NonceReplay,
                    agent_id: None,
                    granted_capabilities: None,
                    behavioral_envelope: None,
                    remaining_lifetime_secs: None,
                }),
            );
        }
        Ok(false) => {
            // Nonce is fresh, continue
        }
        Err(e) => {
            warn!(error = %e, "Failed to check nonce in Redis");
            // On cache failure, we could fail open or closed
            // For security, we fail closed (reject the request)
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(VerifyTokenResponse {
                    valid: false,
                    outcome: VerificationOutcome::InternalError,
                    agent_id: None,
                    granted_capabilities: None,
                    behavioral_envelope: None,
                    remaining_lifetime_secs: None,
                }),
            );
        }
    }

    // Step 2: Check revocation (Redis first, then DB fallback)
    let cached_token = match state.cache.get_cached_token(&token_id).await {
        Ok(Some(cached)) => Some(cached),
        Ok(None) => None,
        Err(e) => {
            warn!(error = %e, jti = %req.jti, "Failed to get cached token");
            None
        }
    };

    // Check revocation from cache
    if let Some(ref cached) = cached_token {
        if cached.is_revoked {
            info!(jti = %req.jti, "Token is revoked (from cache)");
            return (
                StatusCode::OK,
                Json(VerifyTokenResponse {
                    valid: false,
                    outcome: VerificationOutcome::Revoked,
                    agent_id: None,
                    granted_capabilities: None,
                    behavioral_envelope: None,
                    remaining_lifetime_secs: None,
                }),
            );
        }

        // Also check service provider ID from cache
        if cached.service_provider_id != req.service_provider_id.to_string() {
            warn!(
                jti = %req.jti,
                expected = %cached.service_provider_id,
                actual = %req.service_provider_id,
                "Service provider mismatch"
            );
            return (
                StatusCode::OK,
                Json(VerifyTokenResponse {
                    valid: false,
                    outcome: VerificationOutcome::ServiceProviderMismatch,
                    agent_id: None,
                    granted_capabilities: None,
                    behavioral_envelope: None,
                    remaining_lifetime_secs: None,
                }),
            );
        }
    }

    // If not in cache, fall back to database
    let token_row = if cached_token.is_some() {
        // We have cache data, but need full token for detailed checks
        // Only hit DB if we need more details
        match db::get_token(state.db.read_replica(), &token_id).await {
            Ok(Some(row)) => row,
            Ok(None) => {
                info!(jti = %req.jti, "Token not found");
                return (
                    StatusCode::OK,
                    Json(VerifyTokenResponse {
                        valid: false,
                        outcome: VerificationOutcome::NotFound,
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            Err(e) => {
                warn!(error = %e, jti = %req.jti, "Failed to get token from database");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(VerifyTokenResponse {
                        valid: false,
                        outcome: VerificationOutcome::InternalError,
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
        }
    } else {
        // Cache miss - get from database
        match db::get_token(state.db.read_replica(), &token_id).await {
            Ok(Some(row)) => {
                // Check revocation from DB
                if row.is_revoked {
                    info!(jti = %req.jti, "Token is revoked (from DB)");
                    return (
                        StatusCode::OK,
                        Json(VerifyTokenResponse {
                            valid: false,
                            outcome: VerificationOutcome::Revoked,
                            agent_id: None,
                            granted_capabilities: None,
                            behavioral_envelope: None,
                            remaining_lifetime_secs: None,
                        }),
                    );
                }

                // Check service provider ID
                if row.service_provider_id != req.service_provider_id {
                    warn!(
                        jti = %req.jti,
                        expected = %row.service_provider_id,
                        actual = %req.service_provider_id,
                        "Service provider mismatch"
                    );
                    return (
                        StatusCode::OK,
                        Json(VerifyTokenResponse {
                            valid: false,
                            outcome: VerificationOutcome::ServiceProviderMismatch,
                            agent_id: None,
                            granted_capabilities: None,
                            behavioral_envelope: None,
                            remaining_lifetime_secs: None,
                        }),
                    );
                }

                // Cache the token for future requests
                let _ = state
                    .cache
                    .cache_token(
                        &token_id,
                        &row.service_provider_id.to_string(),
                        row.expires_at.timestamp(),
                        row.is_revoked,
                    )
                    .await;

                row
            }
            Ok(None) => {
                info!(jti = %req.jti, "Token not found");
                return (
                    StatusCode::OK,
                    Json(VerifyTokenResponse {
                        valid: false,
                        outcome: VerificationOutcome::NotFound,
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
            Err(e) => {
                warn!(error = %e, jti = %req.jti, "Failed to get token from database");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(VerifyTokenResponse {
                        valid: false,
                        outcome: VerificationOutcome::InternalError,
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
        }
    };

    // Step 3: Verify token binding (cnf claim) if present
    if let Some(ref token_binding) = token_row.token_binding {
        if let Some(ref dpop_thumbprint) = req.dpop_thumbprint {
            // Compare the stored binding with the provided thumbprint
            // Using constant-time comparison for security
            if !constant_time_eq(token_binding, dpop_thumbprint.as_bytes()) {
                warn!(jti = %req.jti, "Token binding mismatch");
                return (
                    StatusCode::OK,
                    Json(VerifyTokenResponse {
                        valid: false,
                        outcome: VerificationOutcome::BindingMismatch,
                        agent_id: None,
                        granted_capabilities: None,
                        behavioral_envelope: None,
                        remaining_lifetime_secs: None,
                    }),
                );
            }
        } else if state.config.verification.require_dpop {
            // Token has binding but no thumbprint provided
            warn!(jti = %req.jti, "DPoP thumbprint required but not provided");
            return (
                StatusCode::OK,
                Json(VerifyTokenResponse {
                    valid: false,
                    outcome: VerificationOutcome::InvalidDpopProof,
                    agent_id: None,
                    granted_capabilities: None,
                    behavioral_envelope: None,
                    remaining_lifetime_secs: None,
                }),
            );
        }
    }

    // Step 4: Verify DPoP proof signature (if required)
    if state.config.verification.require_dpop && req.dpop_proof.is_none() {
        warn!(jti = %req.jti, "DPoP proof required but not provided");
        return (
            StatusCode::OK,
            Json(VerifyTokenResponse {
                valid: false,
                outcome: VerificationOutcome::InvalidDpopProof,
                agent_id: None,
                granted_capabilities: None,
                behavioral_envelope: None,
                remaining_lifetime_secs: None,
            }),
        );
    }

    // TODO: Implement actual DPoP proof verification when require_dpop is true
    // This would involve:
    // 1. Parsing the DPoP JWT
    // 2. Verifying the signature against the public key
    // 3. Checking the 'htm' (HTTP method) claim
    // 4. Checking the 'htu' (HTTP URI) claim
    // 5. Checking the 'iat' (issued at) claim for freshness
    // 6. Checking the 'jti' claim for uniqueness

    // Step 5: Verify token signature using key_id
    // TODO: Implement actual signature verification
    // This would involve:
    // 1. Looking up the public key by key_id
    // 2. Verifying the token signature
    // For now, we trust the database (the registry verified on issuance)

    // Step 6: Check expiry (last check per spec)
    let now = Utc::now();
    let clock_skew = chrono::Duration::seconds(state.config.verification.max_clock_skew_secs);

    if token_row.expires_at + clock_skew < now {
        info!(
            jti = %req.jti,
            expires_at = %token_row.expires_at,
            now = %now,
            "Token has expired"
        );
        return (
            StatusCode::OK,
            Json(VerifyTokenResponse {
                valid: false,
                outcome: VerificationOutcome::Expired,
                agent_id: None,
                granted_capabilities: None,
                behavioral_envelope: None,
                remaining_lifetime_secs: None,
            }),
        );
    }

    // All checks passed - token is valid
    let remaining_lifetime = (token_row.expires_at - now).num_seconds();
    let latency_ms = start.elapsed().as_millis();

    info!(
        jti = %req.jti,
        agent_id = %token_row.agent_id,
        service_provider_id = %req.service_provider_id,
        remaining_lifetime_secs = remaining_lifetime,
        latency_ms = latency_ms,
        "Token verified successfully"
    );

    (
        StatusCode::OK,
        Json(VerifyTokenResponse {
            valid: true,
            outcome: VerificationOutcome::Allowed,
            agent_id: Some(token_row.agent_id),
            granted_capabilities: Some(token_row.granted_capabilities),
            behavioral_envelope: Some(token_row.behavioral_envelope),
            remaining_lifetime_secs: Some(remaining_lifetime),
        }),
    )
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

// ============================================================================
// Keys Endpoint
// ============================================================================

/// Public key response.
#[derive(Debug, Serialize)]
pub struct KeysResponse {
    /// List of public keys.
    pub keys: Vec<PublicKeyInfo>,
}

/// Public key information.
#[derive(Debug, Serialize)]
pub struct PublicKeyInfo {
    /// Key ID.
    pub kid: String,
    /// Key type (always "OKP" for Ed25519).
    pub kty: String,
    /// Curve (always "Ed25519").
    pub crv: String,
    /// Public key (base64url encoded).
    pub x: String,
    /// Key use (always "sig").
    #[serde(rename = "use")]
    pub key_use: String,
    /// Key operations.
    pub key_ops: Vec<String>,
}

/// Get public keys.
///
/// GET /.well-known/agentauth/keys
pub async fn get_keys(State(_state): State<VerifierState>) -> impl IntoResponse {
    // TODO: Implement key lookup from database or configuration
    // For now, return an empty list
    Json(KeysResponse { keys: vec![] })
}

// ============================================================================
// Health Endpoints
// ============================================================================

/// Liveness probe.
///
/// GET /health/live
///
/// Returns 200 if the process is alive and not deadlocked.
pub async fn liveness() -> impl IntoResponse {
    StatusCode::OK
}

/// Readiness probe.
///
/// GET /health/ready
///
/// Returns 200 only when Redis is reachable and cache is warm.
pub async fn readiness(State(state): State<VerifierState>) -> impl IntoResponse {
    if state.health.is_ready().await && state.health.is_cache_ready().await {
        // Double-check Redis is actually reachable
        if state.cache.health_check().await.is_ok() {
            return StatusCode::OK;
        }
        // Redis check failed, mark not ready
        state.health.mark_cache_not_ready().await;
    }
    StatusCode::SERVICE_UNAVAILABLE
}

/// Startup probe.
///
/// GET /health/startup
///
/// Returns 200 once initialization is complete.
pub async fn startup(State(state): State<VerifierState>) -> impl IntoResponse {
    if state.health.is_started().await {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

// ============================================================================
// Metrics
// ============================================================================

/// Prometheus metrics handler.
pub async fn metrics_handler() -> impl IntoResponse {
    // TODO: Implement proper Prometheus metrics export
    // For now, return placeholder
    "# Verifier metrics endpoint\n"
}
