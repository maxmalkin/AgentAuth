//! Grant management handlers.

use crate::db::GrantRow;
use crate::error::{RegistryError, Result};
use crate::services::{AuditEvent, AuditEventType};
use crate::state::AppState;
use auth_core::{AgentId, BehavioralEnvelope, Capability, CapabilityGrant, GrantId, GrantStatus};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Grant request body.
#[derive(Debug, Deserialize)]
pub struct RequestGrantRequest {
    /// Agent ID requesting the grant.
    pub agent_id: Uuid,
    /// Service provider ID.
    pub service_provider_id: Uuid,
    /// Requested capabilities (also accepted as `requested_capabilities` from SDK).
    #[serde(alias = "requested_capabilities")]
    pub capabilities: Vec<Capability>,
    /// Behavioral envelope (also accepted as `requested_envelope` from SDK).
    #[serde(alias = "requested_envelope")]
    pub behavioral_envelope: BehavioralEnvelope,
}

/// Grant response.
#[derive(Debug, Serialize)]
pub struct GrantResponse {
    /// Grant ID (also emitted as `grant_id` for UI compatibility).
    pub id: Uuid,
    /// Alias for `id` — used by the approval UI.
    pub grant_id: Uuid,
    /// Agent ID.
    pub agent_id: Uuid,
    /// Agent display name.
    pub agent_name: String,
    /// Service provider ID.
    pub service_provider_id: Uuid,
    /// Service provider display name.
    pub service_provider_name: String,
    /// Granted capabilities (also emitted as `requested_capabilities`).
    pub granted_capabilities: Vec<Capability>,
    /// Alias for `granted_capabilities` — used by the approval UI.
    pub requested_capabilities: Vec<Capability>,
    /// Behavioral envelope (also emitted as `requested_envelope`).
    pub behavioral_envelope: BehavioralEnvelope,
    /// Alias for `behavioral_envelope` — used by the approval UI.
    pub requested_envelope: BehavioralEnvelope,
    /// Grant status.
    pub status: String,
    /// When the grant was requested.
    pub created_at: DateTime<Utc>,
    /// Approved by (human principal ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_by: Option<Uuid>,
    /// Approved at timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,
    /// Expires at timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Approve grant request body.
#[derive(Debug, Deserialize)]
pub struct ApproveGrantRequest {
    /// Human principal ID approving the grant.
    pub approved_by: Uuid,
    /// Approval nonce (WebAuthn challenge).
    #[serde(with = "hex_serde")]
    pub approval_nonce: Vec<u8>,
    /// Approval signature (WebAuthn signature).
    #[serde(with = "hex_serde")]
    pub approval_signature: Vec<u8>,
}

/// Request a new grant.
///
/// POST /v1/grants/request
pub async fn request_grant(
    State(state): State<AppState>,
    Json(req): Json<RequestGrantRequest>,
) -> Result<impl IntoResponse> {
    let agent_id = AgentId::from_uuid(req.agent_id);

    // Validate capabilities against agent's requested capabilities
    // (In a full implementation, we'd check the agent's manifest)

    // Request the grant
    let grant = state
        .grants
        .request_grant(
            &agent_id,
            req.service_provider_id,
            req.capabilities,
            req.behavioral_envelope,
        )
        .await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::GrantRequested)
                .agent_id(req.agent_id)
                .service_provider_id(req.service_provider_id)
                .grant_id(*grant.id.as_uuid())
                .data(serde_json::json!({
                    "capabilities_count": grant.requested_capabilities.len(),
                })),
        )
        .await;

    Ok((StatusCode::CREATED, Json(grant_to_response(&grant))))
}

/// Get grant details.
///
/// GET /v1/grants/:grant_id
pub async fn get_grant(
    State(state): State<AppState>,
    Path(grant_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let grant_id = GrantId::from_uuid(grant_id);

    let row = state
        .grants
        .get_grant_row(&grant_id)
        .await?
        .ok_or_else(|| RegistryError::GrantNotFound(grant_id.to_string()))?;

    Ok(Json(grant_row_to_response(&row)?))
}

/// Approve a grant.
///
/// POST /v1/grants/:grant_id/approve
pub async fn approve_grant(
    State(state): State<AppState>,
    Path(grant_id): Path<Uuid>,
    Json(req): Json<ApproveGrantRequest>,
) -> Result<impl IntoResponse> {
    let grant_id = GrantId::from_uuid(grant_id);

    // Approve the grant
    let grant = state
        .grants
        .approve_grant(
            &grant_id,
            req.approved_by,
            &req.approval_nonce,
            &req.approval_signature,
        )
        .await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::GrantApproved)
                .agent_id(*grant.agent_id.as_uuid())
                .service_provider_id(grant.service_provider_id.0)
                .human_principal_id(req.approved_by)
                .grant_id(*grant.id.as_uuid()),
        )
        .await;

    Ok(Json(grant_to_response(&grant)))
}

/// Deny a grant.
///
/// POST /v1/grants/:grant_id/deny
pub async fn deny_grant(
    State(state): State<AppState>,
    Path(grant_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let grant_id = GrantId::from_uuid(grant_id);

    let grant = state.grants.deny_grant(&grant_id).await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::GrantDenied)
                .agent_id(*grant.agent_id.as_uuid())
                .service_provider_id(grant.service_provider_id.0)
                .grant_id(*grant.id.as_uuid()),
        )
        .await;

    Ok(Json(grant_to_response(&grant)))
}

/// Revoke a grant.
///
/// POST /v1/grants/:grant_id/revoke
pub async fn revoke_grant(
    State(state): State<AppState>,
    Path(grant_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let grant_id_val = GrantId::from_uuid(grant_id);

    // Get grant first for audit logging
    let grant = state
        .grants
        .get_grant(&grant_id_val)
        .await?
        .ok_or_else(|| RegistryError::GrantNotFound(grant_id_val.to_string()))?;

    state.grants.revoke_grant(&grant_id_val).await?;

    // Record audit event
    let _ = state
        .audit
        .record(
            AuditEvent::new(AuditEventType::GrantRevoked)
                .agent_id(*grant.agent_id.as_uuid())
                .service_provider_id(grant.service_provider_id.0)
                .grant_id(grant_id),
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Convert grant to response (with names defaulting to empty — used for create/approve/deny).
fn grant_to_response(grant: &CapabilityGrant) -> GrantResponse {
    grant_to_response_with_names(grant, String::new(), String::new())
}

/// Convert grant to response with agent and service provider names.
fn grant_to_response_with_names(
    grant: &CapabilityGrant,
    agent_name: String,
    service_provider_name: String,
) -> GrantResponse {
    let approved_by = grant
        .approval_assertion
        .as_ref()
        .map(|_| grant.human_principal_id.0);

    let id = *grant.id.as_uuid();

    GrantResponse {
        id,
        grant_id: id,
        agent_id: *grant.agent_id.as_uuid(),
        agent_name,
        service_provider_id: grant.service_provider_id.0,
        service_provider_name,
        granted_capabilities: grant.requested_capabilities.clone(),
        requested_capabilities: grant.requested_capabilities.clone(),
        behavioral_envelope: grant.requested_envelope.clone(),
        requested_envelope: grant.requested_envelope.clone(),
        status: status_to_string(grant.status),
        created_at: grant.created_at,
        approved_by,
        approved_at: grant.approved_at,
        expires_at: Some(grant.expires_at),
    }
}

/// Convert status to string.
fn status_to_string(status: GrantStatus) -> String {
    match status {
        GrantStatus::Pending => "pending".to_string(),
        GrantStatus::Approved => "approved".to_string(),
        GrantStatus::Denied => "denied".to_string(),
        GrantStatus::Revoked => "revoked".to_string(),
        GrantStatus::Expired => "expired".to_string(),
    }
}

/// Convert a database grant row directly to a response (includes joined names).
fn grant_row_to_response(row: &GrantRow) -> Result<GrantResponse> {
    let capabilities: Vec<Capability> =
        serde_json::from_value(row.granted_capabilities.clone()).map_err(|e| {
            RegistryError::Internal(format!("failed to parse capabilities: {e}"))
        })?;

    let envelope: BehavioralEnvelope =
        serde_json::from_value(row.behavioral_envelope.clone()).map_err(|e| {
            RegistryError::Internal(format!("failed to parse envelope: {e}"))
        })?;

    Ok(GrantResponse {
        id: row.id,
        grant_id: row.id,
        agent_id: row.agent_id,
        agent_name: row.agent_name.clone(),
        service_provider_id: row.service_provider_id,
        service_provider_name: row.service_provider_name.clone(),
        granted_capabilities: capabilities.clone(),
        requested_capabilities: capabilities,
        behavioral_envelope: envelope.clone(),
        requested_envelope: envelope,
        status: row.status.clone(),
        created_at: row.requested_at,
        approved_by: row.approved_by,
        approved_at: row.decided_at,
        expires_at: Some(row.expires_at),
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
