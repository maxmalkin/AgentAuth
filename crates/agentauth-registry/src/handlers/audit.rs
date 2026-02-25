//! Audit log handlers.

use crate::db;
use crate::error::{RegistryError, Result};
use crate::state::AppState;
use agentauth_core::{crypto::hash_chain_event, AgentId};
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Audit event response.
#[derive(Debug, Serialize)]
pub struct AuditEventResponse {
    /// Event ID.
    pub id: Uuid,
    /// Event type.
    pub event_type: String,
    /// Agent ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Uuid>,
    /// Service provider ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_provider_id: Option<Uuid>,
    /// Human principal ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub human_principal_id: Option<Uuid>,
    /// Grant ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<Uuid>,
    /// Token JTI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_jti: Option<Uuid>,
    /// Event data.
    pub event_data: serde_json::Value,
    /// Outcome.
    pub outcome: String,
    /// Error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Audit log query parameters.
#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    /// Start time filter.
    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,
    /// End time filter.
    #[serde(default)]
    pub end_time: Option<DateTime<Utc>>,
    /// Event type filter.
    #[serde(default)]
    pub event_type: Option<String>,
    /// Limit (default 100, max 1000).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Offset.
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 {
    100
}

/// Audit chain verification response.
#[derive(Debug, Serialize)]
pub struct AuditChainVerifyResponse {
    /// Whether the chain is valid.
    pub valid: bool,
    /// Number of events verified.
    pub events_verified: u64,
    /// First event ID verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_event_id: Option<Uuid>,
    /// Last event ID verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_event_id: Option<Uuid>,
    /// Error message if validation failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get audit events for an agent.
///
/// GET /v1/audit/:agent_id
pub async fn get_agent_audit_log(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
    Query(params): Query<AuditQueryParams>,
) -> Result<impl IntoResponse> {
    let agent_id_val = AgentId::from_uuid(agent_id);

    // Verify agent exists
    if !db::agent_exists(state.db.read_replica(), &agent_id_val).await? {
        return Err(RegistryError::AgentNotFound(agent_id.to_string()));
    }

    let limit = params.limit.min(1000);

    let events = query_audit_events(
        state.db.read_replica(),
        Some(agent_id),
        params.start_time,
        params.end_time,
        params.event_type.as_deref(),
        limit,
        params.offset,
    )
    .await?;

    Ok(Json(events))
}

/// Verify audit chain integrity for an agent.
///
/// GET /v1/audit/:agent_id/verify
pub async fn verify_agent_audit_chain(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<impl IntoResponse> {
    let agent_id_val = AgentId::from_uuid(agent_id);

    // Verify agent exists
    if !db::agent_exists(state.db.read_replica(), &agent_id_val).await? {
        return Err(RegistryError::AgentNotFound(agent_id.to_string()));
    }

    // Query all audit events for this agent in order
    let events = query_audit_events_for_verification(state.db.read_replica(), agent_id).await?;

    if events.is_empty() {
        return Ok(Json(AuditChainVerifyResponse {
            valid: true,
            events_verified: 0,
            first_event_id: None,
            last_event_id: None,
            error: None,
        }));
    }

    // Verify the hash chain
    let mut expected_prev_hash = [0u8; 32]; // Genesis hash
    let mut events_verified = 0u64;
    let first_event_id = events.first().map(|e| e.id);
    let last_event_id = events.last().map(|e| e.id);

    for event in &events {
        // Verify the previous hash matches
        if event.previous_event_hash != expected_prev_hash {
            return Ok(Json(AuditChainVerifyResponse {
                valid: false,
                events_verified,
                first_event_id,
                last_event_id: Some(event.id),
                error: Some(format!(
                    "hash chain broken at event {}: previous hash mismatch",
                    event.id
                )),
            }));
        }

        // Compute expected row hash
        let content = build_verification_content(event);
        let computed_hash = hash_chain_event(&expected_prev_hash, &content);

        if event.row_hash != computed_hash {
            return Ok(Json(AuditChainVerifyResponse {
                valid: false,
                events_verified,
                first_event_id,
                last_event_id: Some(event.id),
                error: Some(format!(
                    "hash chain broken at event {}: row hash mismatch",
                    event.id
                )),
            }));
        }

        expected_prev_hash = event.row_hash;
        events_verified += 1;
    }

    Ok(Json(AuditChainVerifyResponse {
        valid: true,
        events_verified,
        first_event_id,
        last_event_id,
        error: None,
    }))
}

/// Record an audit event (internal endpoint).
///
/// POST /v1/audit/record
///
/// This is primarily used for internal audit recording from other services.
/// In production, audit events are recorded atomically with primary operations.
pub async fn record_audit_event(State(_state): State<AppState>) -> Result<impl IntoResponse> {
    // This endpoint is restricted to internal use only.
    // External callers cannot forge audit events.
    Err::<(), _>(RegistryError::Internal(
        "audit event recording is only available internally".into(),
    ))
}

/// Audit event row for verification.
#[derive(Debug)]
struct AuditEventVerificationRow {
    id: Uuid,
    event_type: String,
    agent_id: Option<Uuid>,
    service_provider_id: Option<Uuid>,
    human_principal_id: Option<Uuid>,
    grant_id: Option<Uuid>,
    token_jti: Option<Uuid>,
    event_data: serde_json::Value,
    outcome: String,
    previous_event_hash: [u8; 32],
    row_hash: [u8; 32],
}

/// Query audit events.
#[allow(clippy::cast_possible_wrap)] // limit/offset won't exceed i32::MAX in practice
async fn query_audit_events(
    pool: &sqlx::PgPool,
    agent_id: Option<Uuid>,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    event_type: Option<&str>,
    limit: u32,
    offset: u32,
) -> Result<Vec<AuditEventResponse>> {
    // EXPLAIN ANALYZE: Uses idx_audit_events_agent_time index
    let rows: Vec<AuditEventResponseRow> = sqlx::query_as(
        r#"
        SELECT id, event_type::text, agent_id, service_provider_id, human_principal_id,
               grant_id, token_jti, event_data, outcome, error_message, created_at
        FROM audit_events
        WHERE ($1::uuid IS NULL OR agent_id = $1)
          AND ($2::timestamptz IS NULL OR created_at >= $2)
          AND ($3::timestamptz IS NULL OR created_at <= $3)
          AND ($4::text IS NULL OR event_type::text = $4)
        ORDER BY created_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(agent_id)
    .bind(start_time)
    .bind(end_time)
    .bind(event_type)
    .bind(limit as i32)
    .bind(offset as i32)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| AuditEventResponse {
            id: r.id,
            event_type: r.event_type,
            agent_id: r.agent_id,
            service_provider_id: r.service_provider_id,
            human_principal_id: r.human_principal_id,
            grant_id: r.grant_id,
            token_jti: r.token_jti,
            event_data: r.event_data,
            outcome: r.outcome,
            error_message: r.error_message,
            created_at: r.created_at,
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct AuditEventResponseRow {
    id: Uuid,
    event_type: String,
    agent_id: Option<Uuid>,
    service_provider_id: Option<Uuid>,
    human_principal_id: Option<Uuid>,
    grant_id: Option<Uuid>,
    token_jti: Option<Uuid>,
    event_data: serde_json::Value,
    outcome: String,
    error_message: Option<String>,
    created_at: DateTime<Utc>,
}

/// Query audit events for chain verification.
async fn query_audit_events_for_verification(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
) -> Result<Vec<AuditEventVerificationRow>> {
    let rows: Vec<AuditVerificationRow> = sqlx::query_as(
        r#"
        SELECT id, event_type::text, agent_id, service_provider_id, human_principal_id,
               grant_id, token_jti, event_data, outcome, previous_event_hash, row_hash
        FROM audit_events
        WHERE agent_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let mut prev_hash = [0u8; 32];
            let mut row_hash = [0u8; 32];
            prev_hash
                .copy_from_slice(&r.previous_event_hash[..32.min(r.previous_event_hash.len())]);
            row_hash.copy_from_slice(&r.row_hash[..32.min(r.row_hash.len())]);

            AuditEventVerificationRow {
                id: r.id,
                event_type: r.event_type,
                agent_id: r.agent_id,
                service_provider_id: r.service_provider_id,
                human_principal_id: r.human_principal_id,
                grant_id: r.grant_id,
                token_jti: r.token_jti,
                event_data: r.event_data,
                outcome: r.outcome,
                previous_event_hash: prev_hash,
                row_hash,
            }
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct AuditVerificationRow {
    id: Uuid,
    event_type: String,
    agent_id: Option<Uuid>,
    service_provider_id: Option<Uuid>,
    human_principal_id: Option<Uuid>,
    grant_id: Option<Uuid>,
    token_jti: Option<Uuid>,
    event_data: serde_json::Value,
    outcome: String,
    previous_event_hash: Vec<u8>,
    row_hash: Vec<u8>,
}

/// Build content for hash verification.
fn build_verification_content(event: &AuditEventVerificationRow) -> Vec<u8> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    hasher.update(event.id.as_bytes());
    hasher.update(event.event_type.as_bytes());
    if let Some(ref id) = event.agent_id {
        hasher.update(id.as_bytes());
    }
    if let Some(ref id) = event.service_provider_id {
        hasher.update(id.as_bytes());
    }
    if let Some(ref id) = event.human_principal_id {
        hasher.update(id.as_bytes());
    }
    if let Some(ref id) = event.grant_id {
        hasher.update(id.as_bytes());
    }
    if let Some(ref id) = event.token_jti {
        hasher.update(id.as_bytes());
    }
    hasher.update(event.event_data.to_string().as_bytes());
    hasher.update(event.outcome.as_bytes());

    hasher.finalize().to_vec()
}
