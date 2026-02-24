//! Database queries.

use crate::error::{RegistryError, Result};
use agentauth_core::{
    AgentAccessToken, AgentId, AgentManifest, BehavioralEnvelope, Capability, GrantId, TokenId,
};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// ============================================================================
// Agent Queries
// ============================================================================

/// Agent manifest row from database.
#[derive(Debug, sqlx::FromRow)]
pub struct AgentRow {
    /// Agent ID.
    pub id: Uuid,
    /// Human principal ID.
    pub human_principal_id: Uuid,
    /// Agent name.
    pub name: String,
    /// Description.
    pub description: Option<String>,
    /// Public key bytes.
    pub public_key: Vec<u8>,
    /// Key ID.
    pub key_id: String,
    /// Requested capabilities as JSON.
    pub requested_capabilities: serde_json::Value,
    /// Default behavioral envelope as JSON.
    pub default_behavioral_envelope: serde_json::Value,
    /// Model origin.
    pub model_origin: Option<String>,
    /// Signature bytes.
    pub signature: Vec<u8>,
    /// Issued at.
    pub issued_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Is active.
    pub is_active: bool,
    /// Created at.
    pub created_at: DateTime<Utc>,
}

/// Insert a new agent manifest.
pub async fn insert_agent(
    pool: &PgPool,
    manifest: &AgentManifest,
    signature: &[u8],
) -> Result<()> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    // EXPLAIN ANALYZE: Uses primary key, single row insert
    // Note: We store capabilities_requested as requested_capabilities in the DB
    // Note: public_key in agentauth-core is base64url string, DB expects BYTEA
    let public_key_bytes = URL_SAFE_NO_PAD
        .decode(&manifest.public_key)
        .map_err(|e| RegistryError::Internal(format!("invalid public key encoding: {e}")))?;

    // Use a default restrictive envelope
    let default_envelope = BehavioralEnvelope::default_restrictive();

    sqlx::query(
        r#"
        INSERT INTO agent_manifests (
            id, human_principal_id, name, description, public_key, key_id,
            requested_capabilities, default_behavioral_envelope, model_origin,
            signature, issued_at, expires_at, is_active
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(manifest.id.as_uuid())
    .bind(manifest.human_principal_id.0)
    .bind(&manifest.name)
    .bind(&manifest.description)
    .bind(&public_key_bytes)
    .bind(&manifest.key_id)
    .bind(serde_json::to_value(&manifest.capabilities_requested).map_err(|e| {
        RegistryError::Internal(format!("failed to serialize capabilities: {e}"))
    })?)
    .bind(serde_json::to_value(&default_envelope).map_err(|e| {
        RegistryError::Internal(format!("failed to serialize envelope: {e}"))
    })?)
    .bind(&manifest.model_origin)
    .bind(signature)
    .bind(manifest.issued_at)
    .bind(manifest.expires_at)
    .bind(true)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get an agent by ID.
pub async fn get_agent(pool: &PgPool, agent_id: &AgentId) -> Result<Option<AgentRow>> {
    // EXPLAIN ANALYZE: Uses primary key index on id
    let row = sqlx::query_as::<_, AgentRow>(
        r#"
        SELECT id, human_principal_id, name, description, public_key, key_id,
               requested_capabilities, default_behavioral_envelope, model_origin,
               signature, issued_at, expires_at, is_active, created_at
        FROM agent_manifests
        WHERE id = $1
        "#,
    )
    .bind(agent_id.as_uuid())
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Check if an agent exists.
pub async fn agent_exists(pool: &PgPool, agent_id: &AgentId) -> Result<bool> {
    // EXPLAIN ANALYZE: Uses primary key index
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agent_manifests WHERE id = $1)")
        .bind(agent_id.as_uuid())
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

/// Deactivate an agent.
pub async fn deactivate_agent(pool: &PgPool, agent_id: &AgentId) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_manifests SET is_active = FALSE, updated_at = NOW() WHERE id = $1 AND is_active = TRUE",
    )
    .bind(agent_id.as_uuid())
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

// ============================================================================
// Grant Queries
// ============================================================================

/// Grant row from database.
#[derive(Debug, sqlx::FromRow)]
pub struct GrantRow {
    /// Grant ID.
    pub id: Uuid,
    /// Agent ID.
    pub agent_id: Uuid,
    /// Service provider ID.
    pub service_provider_id: Uuid,
    /// Human principal ID (from agent).
    pub human_principal_id: Uuid,
    /// Approved by human principal ID.
    pub approved_by: Option<Uuid>,
    /// Granted capabilities as JSON.
    pub granted_capabilities: serde_json::Value,
    /// Behavioral envelope as JSON.
    pub behavioral_envelope: serde_json::Value,
    /// Status.
    pub status: String,
    /// Approval nonce.
    pub approval_nonce: Option<Vec<u8>>,
    /// Approval signature.
    pub approval_signature: Option<Vec<u8>>,
    /// Requested at (created_at).
    pub requested_at: DateTime<Utc>,
    /// Decided at.
    pub decided_at: Option<DateTime<Utc>>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
}

/// Insert a new grant request.
pub async fn insert_grant(
    pool: &PgPool,
    grant_id: &GrantId,
    agent_id: &AgentId,
    service_provider_id: Uuid,
    capabilities: &[Capability],
    envelope: &BehavioralEnvelope,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO capability_grants (
            id, agent_id, service_provider_id, granted_capabilities,
            behavioral_envelope, status, expires_at
        ) VALUES ($1, $2, $3, $4, $5, 'pending', $6)
        "#,
    )
    .bind(grant_id.as_uuid())
    .bind(agent_id.as_uuid())
    .bind(service_provider_id)
    .bind(serde_json::to_value(capabilities).map_err(|e| {
        RegistryError::Internal(format!("failed to serialize capabilities: {e}"))
    })?)
    .bind(serde_json::to_value(envelope).map_err(|e| {
        RegistryError::Internal(format!("failed to serialize envelope: {e}"))
    })?)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get a grant by ID.
pub async fn get_grant(pool: &PgPool, grant_id: &GrantId) -> Result<Option<GrantRow>> {
    let row = sqlx::query_as::<_, GrantRow>(
        r#"
        SELECT g.id, g.agent_id, g.service_provider_id, a.human_principal_id,
               g.approved_by, g.granted_capabilities,
               g.behavioral_envelope, g.status::text as status, g.approval_nonce, g.approval_signature,
               g.requested_at, g.decided_at, g.expires_at
        FROM capability_grants g
        INNER JOIN agent_manifests a ON g.agent_id = a.id
        WHERE g.id = $1
        "#,
    )
    .bind(grant_id.as_uuid())
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Count pending grants for an agent.
pub async fn count_pending_grants(pool: &PgPool, agent_id: &AgentId) -> Result<i64> {
    // EXPLAIN ANALYZE: Uses idx_capability_grants_pending index
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM capability_grants WHERE agent_id = $1 AND status = 'pending'")
            .bind(agent_id.as_uuid())
            .fetch_one(pool)
            .await?;
    Ok(count)
}

/// Approve a grant.
pub async fn approve_grant(
    pool: &PgPool,
    grant_id: &GrantId,
    approved_by: Uuid,
    approval_nonce: &[u8],
    approval_signature: &[u8],
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE capability_grants
        SET status = 'approved', approved_by = $2, approval_nonce = $3,
            approval_signature = $4, decided_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(grant_id.as_uuid())
    .bind(approved_by)
    .bind(approval_nonce)
    .bind(approval_signature)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Deny a grant.
pub async fn deny_grant(pool: &PgPool, grant_id: &GrantId) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE capability_grants
        SET status = 'denied', decided_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND status = 'pending'
        "#,
    )
    .bind(grant_id.as_uuid())
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Revoke a grant.
pub async fn revoke_grant(pool: &PgPool, grant_id: &GrantId) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE capability_grants
        SET status = 'revoked', updated_at = NOW()
        WHERE id = $1 AND status = 'approved'
        "#,
    )
    .bind(grant_id.as_uuid())
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

// ============================================================================
// Token Queries
// ============================================================================

/// Token row from database.
#[derive(Debug, sqlx::FromRow)]
pub struct TokenRow {
    /// JWT ID.
    pub jti: Uuid,
    /// Grant ID.
    pub grant_id: Uuid,
    /// Agent ID.
    pub agent_id: Uuid,
    /// Service provider ID.
    pub service_provider_id: Uuid,
    /// Human principal ID.
    pub human_principal_id: Uuid,
    /// Key ID.
    pub key_id: String,
    /// Token binding.
    pub token_binding: Option<Vec<u8>>,
    /// Granted capabilities as JSON.
    pub granted_capabilities: serde_json::Value,
    /// Behavioral envelope as JSON.
    pub behavioral_envelope: serde_json::Value,
    /// Issued at.
    pub issued_at: DateTime<Utc>,
    /// Expires at.
    pub expires_at: DateTime<Utc>,
    /// Is revoked.
    pub is_revoked: bool,
    /// Revoked at.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Revocation reason.
    pub revocation_reason: Option<String>,
}

/// Insert a new token.
pub async fn insert_token(
    pool: &PgPool,
    token: &AgentAccessToken,
    grant_id: &GrantId,
    key_id: &str,
    token_binding: Option<&[u8]>,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO issued_tokens (
            jti, grant_id, agent_id, service_provider_id, human_principal_id,
            key_id, token_binding, granted_capabilities, behavioral_envelope,
            issued_at, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(token.jti.as_uuid())
    .bind(grant_id.as_uuid())
    .bind(token.agent_id.as_uuid())
    .bind(token.service_provider_id.0)
    .bind(token.human_principal_id.0)
    .bind(key_id)
    .bind(token_binding)
    .bind(serde_json::to_value(&token.granted_capabilities).map_err(|e| {
        RegistryError::Internal(format!("failed to serialize capabilities: {e}"))
    })?)
    .bind(serde_json::to_value(&token.behavioral_envelope).map_err(|e| {
        RegistryError::Internal(format!("failed to serialize envelope: {e}"))
    })?)
    .bind(token.issued_at)
    .bind(token.expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

/// Get a token by JTI.
pub async fn get_token(pool: &PgPool, jti: &TokenId) -> Result<Option<TokenRow>> {
    let row = sqlx::query_as::<_, TokenRow>(
        r#"
        SELECT jti, grant_id, agent_id, service_provider_id, human_principal_id,
               key_id, token_binding, granted_capabilities, behavioral_envelope,
               issued_at, expires_at, is_revoked, revoked_at, revocation_reason
        FROM issued_tokens
        WHERE jti = $1
        "#,
    )
    .bind(jti.as_uuid())
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Find existing token for idempotency check.
pub async fn find_existing_token(
    pool: &PgPool,
    grant_id: &GrantId,
    window_start: DateTime<Utc>,
) -> Result<Option<TokenRow>> {
    // EXPLAIN ANALYZE: Uses idx_issued_tokens_idempotency index
    let row = sqlx::query_as::<_, TokenRow>(
        r#"
        SELECT jti, grant_id, agent_id, service_provider_id, human_principal_id,
               key_id, token_binding, granted_capabilities, behavioral_envelope,
               issued_at, expires_at, is_revoked, revoked_at, revocation_reason
        FROM issued_tokens
        WHERE grant_id = $1 AND issued_at >= $2 AND is_revoked = FALSE
        ORDER BY issued_at DESC
        LIMIT 1
        "#,
    )
    .bind(grant_id.as_uuid())
    .bind(window_start)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}

/// Revoke a token.
pub async fn revoke_token(
    pool: &PgPool,
    jti: &TokenId,
    reason: Option<&str>,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE issued_tokens
        SET is_revoked = TRUE, revoked_at = NOW(), revocation_reason = $2
        WHERE jti = $1 AND is_revoked = FALSE
        "#,
    )
    .bind(jti.as_uuid())
    .bind(reason)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// Check if a token is revoked.
pub async fn is_token_revoked(pool: &PgPool, jti: &TokenId) -> Result<bool> {
    // EXPLAIN ANALYZE: Uses primary key index
    let revoked: Option<bool> =
        sqlx::query_scalar("SELECT is_revoked FROM issued_tokens WHERE jti = $1")
            .bind(jti.as_uuid())
            .fetch_optional(pool)
            .await?;
    Ok(revoked.unwrap_or(true)) // If not found, treat as revoked
}

// ============================================================================
// Audit Queries
// ============================================================================

/// Insert an audit event.
pub async fn insert_audit_event(
    pool: &PgPool,
    event_id: Uuid,
    event_type: &str,
    agent_id: Option<Uuid>,
    service_provider_id: Option<Uuid>,
    human_principal_id: Option<Uuid>,
    grant_id: Option<Uuid>,
    token_jti: Option<Uuid>,
    event_data: serde_json::Value,
    outcome: &str,
    error_message: Option<&str>,
    source_ip: Option<&str>,
    user_agent: Option<&str>,
    request_id: Option<Uuid>,
    trace_id: Option<&str>,
    previous_event_hash: &[u8],
    row_hash: &[u8],
    registry_signature: &[u8],
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO audit_events (
            id, event_type, agent_id, service_provider_id, human_principal_id,
            grant_id, token_jti, event_data, outcome, error_message,
            source_ip, user_agent, request_id, trace_id,
            previous_event_hash, row_hash, registry_signature
        ) VALUES ($1, $2::audit_event_type, $3, $4, $5, $6, $7, $8, $9, $10, $11::inet, $12, $13, $14, $15, $16, $17)
        "#,
    )
    .bind(event_id)
    .bind(event_type)
    .bind(agent_id)
    .bind(service_provider_id)
    .bind(human_principal_id)
    .bind(grant_id)
    .bind(token_jti)
    .bind(event_data)
    .bind(outcome)
    .bind(error_message)
    .bind(source_ip)
    .bind(user_agent)
    .bind(request_id)
    .bind(trace_id)
    .bind(previous_event_hash)
    .bind(row_hash)
    .bind(registry_signature)
    .execute(pool)
    .await?;

    Ok(())
}
