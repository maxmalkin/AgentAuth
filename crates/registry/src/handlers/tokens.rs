//! Token management handlers.

use crate::db;
use crate::error::{RegistryError, Result};
use crate::services::{AuditEvent, AuditEventType};
use crate::state::AppState;
use auth_core::{AgentAccessToken, AgentId, BehavioralEnvelope, Capability, GrantId, TokenId};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Token issuance request.
///
/// Supports two modes:
/// - **Full**: all fields provided (legacy/direct callers)
/// - **Simplified**: only `grant_id` + optional `dpop_thumbprint` (SDK callers);
///   the handler looks up grant details from the database.
#[derive(Debug, Deserialize)]
pub struct IssueTokenRequest {
    /// Grant ID for which to issue the token.
    pub grant_id: Uuid,
    /// Agent ID (optional — looked up from grant if absent).
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    /// Service provider ID (optional — looked up from grant if absent).
    #[serde(default)]
    pub service_provider_id: Option<Uuid>,
    /// Human principal ID (optional — looked up from grant if absent).
    #[serde(default)]
    pub human_principal_id: Option<Uuid>,
    /// Capabilities (optional — looked up from grant if absent).
    #[serde(default)]
    pub capabilities: Option<Vec<Capability>>,
    /// Behavioral envelope (optional — looked up from grant if absent).
    #[serde(default)]
    pub behavioral_envelope: Option<BehavioralEnvelope>,
    /// Optional token binding (for DPoP, hex-encoded).
    #[serde(default)]
    #[serde(with = "option_hex_serde")]
    pub token_binding: Option<Vec<u8>>,
    /// Optional DPoP thumbprint (SDK sends this instead of token_binding).
    #[serde(default)]
    pub dpop_thumbprint: Option<String>,
}

/// Token response.
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    /// Token JTI (also exposed as `access_token` for SDK compatibility).
    pub jti: Uuid,
    /// Access token string (JTI as string, for SDK).
    pub access_token: String,
    /// Token type.
    pub token_type: String,
    /// Agent ID.
    pub agent_id: Uuid,
    /// Human principal ID.
    pub human_principal_id: Uuid,
    /// Service provider ID.
    pub service_provider_id: Uuid,
    /// Grant ID.
    pub grant_id: Uuid,
    /// Granted capabilities.
    pub granted_capabilities: Vec<Capability>,
    /// Behavioral envelope.
    pub behavioral_envelope: BehavioralEnvelope,
    /// Issued at.
    pub issued_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Token binding (hex encoded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_binding: Option<String>,
}

/// Token revocation request.
#[derive(Debug, Deserialize)]
pub struct RevokeTokenRequest {
    /// Token JTI to revoke.
    pub jti: Uuid,
    /// Optional revocation reason.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Issue a token for an approved grant.
///
/// POST /v1/tokens/issue
///
/// This is idempotent: calling with the same grant within the idempotency window
/// returns the same token JTI.
pub async fn issue_token(
    State(state): State<AppState>,
    Json(req): Json<IssueTokenRequest>,
) -> Result<impl IntoResponse> {
    let grant_id = GrantId::from_uuid(req.grant_id);

    // If simplified request (no agent_id), look up grant details from DB
    let (agent_id_uuid, sp_id, hp_id, capabilities, envelope) =
        if let (Some(a), Some(s), Some(h), Some(c), Some(e)) = (
            req.agent_id,
            req.service_provider_id,
            req.human_principal_id,
            req.capabilities,
            req.behavioral_envelope,
        ) {
            (a, s, h, c, e)
        } else {
            // Look up from grant
            let grant_row = db::get_grant(state.db.read_replica(), &grant_id)
                .await?
                .ok_or_else(|| RegistryError::GrantNotFound(grant_id.to_string()))?;

            if grant_row.status != "approved" {
                return Err(RegistryError::GrantNotApproved(grant_id.to_string()));
            }

            let caps: Vec<Capability> =
                serde_json::from_value(grant_row.granted_capabilities).map_err(|e| {
                    RegistryError::Internal(format!("failed to parse grant capabilities: {e}"))
                })?;
            let env: BehavioralEnvelope =
                serde_json::from_value(grant_row.behavioral_envelope).map_err(|e| {
                    RegistryError::Internal(format!("failed to parse grant envelope: {e}"))
                })?;

            (
                grant_row.agent_id,
                grant_row.service_provider_id,
                grant_row.human_principal_id,
                caps,
                env,
            )
        };

    let agent_id = AgentId::from_uuid(agent_id_uuid);

    // Resolve token binding: prefer explicit token_binding, fall back to dpop_thumbprint as bytes
    let token_binding = req
        .token_binding
        .or_else(|| req.dpop_thumbprint.map(String::into_bytes));

    // Issue the token (idempotent)
    let token = state
        .tokens
        .issue_token(
            &grant_id,
            &agent_id,
            sp_id,
            hp_id,
            capabilities,
            envelope,
            token_binding,
        )
        .await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::TokenIssued)
                .agent_id(agent_id_uuid)
                .service_provider_id(sp_id)
                .human_principal_id(hp_id)
                .grant_id(req.grant_id)
                .token_jti(*token.jti.as_uuid()),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(token_to_response(&token, req.grant_id)),
    ))
}

/// Revoke a token.
///
/// POST /v1/tokens/revoke
pub async fn revoke_token(
    State(state): State<AppState>,
    Json(req): Json<RevokeTokenRequest>,
) -> Result<impl IntoResponse> {
    let jti = TokenId::from_uuid(req.jti);

    // Get token first for audit logging
    let token = state
        .tokens
        .get_token(&jti)
        .await?
        .ok_or_else(|| RegistryError::TokenNotFound(jti.to_string()))?;

    // Revoke the token
    state
        .tokens
        .revoke_token(&jti, req.reason.as_deref())
        .await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::TokenRevoked)
                .agent_id(*token.agent_id.as_uuid())
                .service_provider_id(token.service_provider_id.0)
                .token_jti(req.jti)
                .data(serde_json::json!({
                    "reason": req.reason,
                })),
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Convert token to response.
fn token_to_response(token: &AgentAccessToken, grant_id: Uuid) -> TokenResponse {
    let jti = *token.jti.as_uuid();
    TokenResponse {
        jti,
        access_token: jti.to_string(),
        token_type: "AgentBearer".to_string(),
        agent_id: *token.agent_id.as_uuid(),
        human_principal_id: token.human_principal_id.0,
        service_provider_id: token.service_provider_id.0,
        grant_id,
        granted_capabilities: token.granted_capabilities.clone(),
        behavioral_envelope: token.behavioral_envelope.clone(),
        issued_at: token.issued_at,
        expires_at: token.expires_at,
        token_binding: None,
    }
}

/// Optional hex serialization helper.
mod option_hex_serde {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) if !s.is_empty() => hex::decode(&s).map(Some).map_err(serde::de::Error::custom),
            _ => Ok(None),
        }
    }
}
