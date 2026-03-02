//! Agent management handlers.

use crate::db::{self, AgentRow};
use crate::error::{RegistryError, Result};
use crate::services::{AuditEvent, AuditEventType};
use crate::state::AppState;
use auth_core::{AgentId, AgentManifest, BehavioralEnvelope, Capability};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent registration request.
#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    /// Agent manifest.
    pub manifest: AgentManifest,
    /// Signature over the manifest.
    #[serde(with = "hex_serde")]
    pub signature: Vec<u8>,
}

/// Agent registration response.
#[derive(Debug, Serialize)]
pub struct RegisterAgentResponse {
    /// Agent ID.
    pub agent_id: Uuid,
    /// Registration status.
    pub status: String,
}

/// Agent details response.
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    /// Agent ID.
    pub id: Uuid,
    /// Human principal ID.
    pub human_principal_id: Uuid,
    /// Agent name.
    pub name: String,
    /// Description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Public key (hex encoded).
    pub public_key: String,
    /// Key ID.
    pub key_id: String,
    /// Requested capabilities.
    pub requested_capabilities: Vec<Capability>,
    /// Default behavioral envelope.
    pub default_behavioral_envelope: BehavioralEnvelope,
    /// Model origin.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_origin: Option<String>,
    /// Is active.
    pub is_active: bool,
}

/// Bootstrap request for OTP-based provisioning.
#[derive(Debug, Deserialize)]
pub struct BootstrapAgentRequest {
    /// One-time provisioning token.
    pub otp: String,
    /// Agent manifest (without signature - will be signed by registry).
    pub manifest: AgentManifest,
}

/// Bootstrap response.
#[derive(Debug, Serialize)]
pub struct BootstrapAgentResponse {
    /// Agent ID.
    pub agent_id: Uuid,
    /// Key reference (not the actual key).
    pub key_ref: String,
}

/// Register a new agent.
///
/// POST /v1/agents/register
pub async fn register_agent(
    State(state): State<AppState>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<impl IntoResponse> {
    let agent_id = req.manifest.id;

    // Check if agent already exists (idempotent)
    if db::agent_exists(state.db.primary(), &agent_id).await? {
        // Return success for idempotent re-registration
        return Ok((
            StatusCode::OK,
            Json(RegisterAgentResponse {
                agent_id: *agent_id.as_uuid(),
                status: "already_registered".to_string(),
            }),
        ));
    }

    // Verify the manifest signature
    auth_core::crypto::verify_manifest_bytes(&req.manifest, &req.signature)
        .map_err(|e| RegistryError::SignatureVerificationFailed(e.to_string()))?;

    // Insert the agent
    db::insert_agent(state.db.primary(), &req.manifest, &req.signature).await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::AgentRegistered)
                .agent_id(*agent_id.as_uuid())
                .human_principal_id(req.manifest.human_principal_id.0)
                .data(serde_json::json!({
                    "name": req.manifest.name,
                    "model_origin": req.manifest.model_origin,
                })),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(RegisterAgentResponse {
            agent_id: *agent_id.as_uuid(),
            status: "registered".to_string(),
        }),
    ))
}

/// Bootstrap a new agent using OTP.
///
/// POST /v1/agents/bootstrap
pub async fn bootstrap_agent(
    State(state): State<AppState>,
    Json(req): Json<BootstrapAgentRequest>,
) -> Result<impl IntoResponse> {
    // Validate and consume OTP
    let otp_valid = state.cache.validate_and_consume_otp(&req.otp).await?;
    if !otp_valid {
        return Err(RegistryError::OtpInvalid);
    }

    let agent_id = req.manifest.id;

    // Check if agent already exists
    if db::agent_exists(state.db.primary(), &agent_id).await? {
        return Err(RegistryError::AgentAlreadyRegistered);
    }

    // Sign the manifest using the registry's signing backend
    let signature = state
        .signer
        .sign(&auth_core::crypto::manifest_signing_bytes(&req.manifest))
        .await
        .map_err(|e| RegistryError::Internal(format!("failed to sign manifest: {e}")))?;

    // Insert the agent
    db::insert_agent(state.db.primary(), &req.manifest, signature.as_bytes()).await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::AgentRegistered)
                .agent_id(*agent_id.as_uuid())
                .human_principal_id(req.manifest.human_principal_id.0)
                .data(serde_json::json!({
                    "name": req.manifest.name,
                    "bootstrap": true,
                })),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(BootstrapAgentResponse {
            agent_id: *agent_id.as_uuid(),
            key_ref: state.signer.key_id().to_string(),
        }),
    ))
}

/// Get agent details.
///
/// GET /v1/agents/:agent_id
pub async fn get_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let agent_id = AgentId::from_uuid(agent_id);

    let row = db::get_agent(state.db.read_replica(), &agent_id)
        .await?
        .ok_or_else(|| RegistryError::AgentNotFound(agent_id.to_string()))?;

    let response = row_to_response(&row)?;

    Ok(Json(response))
}

/// Delete (deactivate) an agent.
///
/// DELETE /v1/agents/:agent_id
pub async fn delete_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let agent_id = AgentId::from_uuid(agent_id);

    let deactivated = db::deactivate_agent(state.db.primary(), &agent_id).await?;

    if !deactivated {
        // Check if agent exists
        let exists = db::agent_exists(state.db.read_replica(), &agent_id).await?;
        if !exists {
            return Err(RegistryError::AgentNotFound(agent_id.to_string()));
        }
        // Agent exists but wasn't deactivated - already inactive
    }

    // Record audit event
    let _ = state
        .audit
        .record(AuditEvent::new(AuditEventType::AgentRevoked).agent_id(*agent_id.as_uuid()))
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Convert database row to response.
fn row_to_response(row: &AgentRow) -> Result<AgentResponse> {
    let capabilities: Vec<Capability> = serde_json::from_value(row.requested_capabilities.clone())
        .map_err(|e| RegistryError::Internal(format!("failed to parse capabilities: {e}")))?;

    let envelope: BehavioralEnvelope =
        serde_json::from_value(row.default_behavioral_envelope.clone())
            .map_err(|e| RegistryError::Internal(format!("failed to parse envelope: {e}")))?;

    Ok(AgentResponse {
        id: row.id,
        human_principal_id: row.human_principal_id,
        name: row.name.clone(),
        description: row.description.clone(),
        public_key: hex::encode(&row.public_key),
        key_id: row.key_id.clone(),
        requested_capabilities: capabilities,
        default_behavioral_envelope: envelope,
        model_origin: row.model_origin.clone(),
        is_active: row.is_active,
    })
}

/// Hex serialization helper.
mod hex_serde {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}
