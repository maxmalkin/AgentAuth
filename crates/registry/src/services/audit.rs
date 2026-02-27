//! Audit logging service.

use crate::db::{self, DbPool};
use crate::error::{RegistryError, Result};
use auth_core::crypto::{hash_chain_event, SigningBackend};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Audit event types.
#[derive(Debug, Clone, Copy)]
pub enum AuditEventType {
    /// Agent registered.
    AgentRegistered,
    /// Agent updated.
    AgentUpdated,
    /// Agent revoked.
    AgentRevoked,
    /// Grant requested.
    GrantRequested,
    /// Grant approved.
    GrantApproved,
    /// Grant denied.
    GrantDenied,
    /// Grant revoked.
    GrantRevoked,
    /// Token issued.
    TokenIssued,
    /// Token verified and allowed.
    TokenVerifiedAllowed,
    /// Token verified and denied.
    TokenVerifiedDenied,
    /// Token revoked.
    TokenRevoked,
    /// Rate limit exceeded.
    RateLimitExceeded,
    /// Security violation detected.
    SecurityViolation,
}

impl AuditEventType {
    /// Convert to database string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentRegistered => "agent_registered",
            Self::AgentUpdated => "agent_updated",
            Self::AgentRevoked => "agent_revoked",
            Self::GrantRequested => "grant_requested",
            Self::GrantApproved => "grant_approved",
            Self::GrantDenied => "grant_denied",
            Self::GrantRevoked => "grant_revoked",
            Self::TokenIssued => "token_issued",
            Self::TokenVerifiedAllowed => "token_verified_allowed",
            Self::TokenVerifiedDenied => "token_verified_denied",
            Self::TokenRevoked => "token_revoked",
            Self::RateLimitExceeded => "rate_limit_exceeded",
            Self::SecurityViolation => "security_violation",
        }
    }
}

/// Audit event builder.
#[derive(Debug, Default)]
pub struct AuditEvent {
    /// Event type.
    pub event_type: Option<AuditEventType>,
    /// Agent ID.
    pub agent_id: Option<Uuid>,
    /// Service provider ID.
    pub service_provider_id: Option<Uuid>,
    /// Human principal ID.
    pub human_principal_id: Option<Uuid>,
    /// Grant ID.
    pub grant_id: Option<Uuid>,
    /// Token JTI.
    pub token_jti: Option<Uuid>,
    /// Event data (JSON).
    pub event_data: serde_json::Value,
    /// Outcome.
    pub outcome: String,
    /// Error message.
    pub error_message: Option<String>,
    /// Source IP.
    pub source_ip: Option<String>,
    /// User agent.
    pub user_agent: Option<String>,
    /// Request ID.
    pub request_id: Option<Uuid>,
    /// Trace ID.
    pub trace_id: Option<String>,
}

impl AuditEvent {
    /// Create a new audit event builder.
    pub fn new(event_type: AuditEventType) -> Self {
        Self {
            event_type: Some(event_type),
            outcome: "success".to_string(),
            event_data: serde_json::json!({}),
            ..Default::default()
        }
    }

    /// Set agent ID.
    #[must_use]
    pub fn agent_id(mut self, id: Uuid) -> Self {
        self.agent_id = Some(id);
        self
    }

    /// Set service provider ID.
    #[must_use]
    pub fn service_provider_id(mut self, id: Uuid) -> Self {
        self.service_provider_id = Some(id);
        self
    }

    /// Set human principal ID.
    #[must_use]
    pub fn human_principal_id(mut self, id: Uuid) -> Self {
        self.human_principal_id = Some(id);
        self
    }

    /// Set grant ID.
    #[must_use]
    pub fn grant_id(mut self, id: Uuid) -> Self {
        self.grant_id = Some(id);
        self
    }

    /// Set token JTI.
    #[must_use]
    pub fn token_jti(mut self, jti: Uuid) -> Self {
        self.token_jti = Some(jti);
        self
    }

    /// Set event data.
    #[must_use]
    pub fn data(mut self, data: serde_json::Value) -> Self {
        self.event_data = data;
        self
    }

    /// Set outcome.
    #[must_use]
    pub fn outcome(mut self, outcome: &str) -> Self {
        self.outcome = outcome.to_string();
        self
    }

    /// Set error message.
    #[must_use]
    pub fn error(mut self, message: &str) -> Self {
        self.error_message = Some(message.to_string());
        self.outcome = "error".to_string();
        self
    }

    /// Set source IP.
    #[must_use]
    pub fn source_ip(mut self, ip: &str) -> Self {
        self.source_ip = Some(ip.to_string());
        self
    }

    /// Set user agent.
    #[must_use]
    pub fn user_agent(mut self, ua: &str) -> Self {
        self.user_agent = Some(ua.to_string());
        self
    }

    /// Set request ID.
    #[must_use]
    pub fn request_id(mut self, id: Uuid) -> Self {
        self.request_id = Some(id);
        self
    }

    /// Set trace ID.
    #[must_use]
    pub fn trace_id(mut self, id: &str) -> Self {
        self.trace_id = Some(id.to_string());
        self
    }
}

/// Audit service for recording audit events.
pub struct AuditService {
    /// Database pool.
    db: DbPool,
    /// Signing backend for signing audit events.
    signer: Arc<dyn SigningBackend>,
    /// Previous event hash (for hash chain).
    previous_hash: RwLock<[u8; 32]>,
}

impl AuditService {
    /// Create a new audit service.
    pub fn new(db: DbPool, signer: Arc<dyn SigningBackend>) -> Self {
        Self {
            db,
            signer,
            previous_hash: RwLock::new([0u8; 32]),
        }
    }

    /// Initialize the audit service by loading the last event hash.
    #[allow(clippy::unused_async)] // Will be async when we query the last audit event's hash
    pub async fn initialize(&self) -> Result<()> {
        // In a real implementation, we'd query the last audit event's hash
        // For now, we start with zeros (genesis)
        Ok(())
    }

    /// Record an audit event atomically with the primary operation.
    ///
    /// The caller should wrap this in the same transaction as the primary operation.
    pub async fn record(&self, event: AuditEvent) -> Result<Uuid> {
        let event_type = event
            .event_type
            .ok_or_else(|| RegistryError::Internal("audit event type required".into()))?;

        let event_id = Uuid::now_v7();

        // Build the content to hash
        let content = Self::build_hash_content(&event_id, &event);

        // Get previous hash and compute new hash
        let previous_hash = *self.previous_hash.read().await;
        let row_hash = hash_chain_event(&previous_hash, &content);

        // Sign the row hash
        let signature = self
            .signer
            .sign(&row_hash)
            .await
            .map_err(|e| RegistryError::Internal(format!("failed to sign audit event: {e}")))?;

        // Insert the event
        db::insert_audit_event(
            self.db.primary(),
            event_id,
            event_type.as_str(),
            event.agent_id,
            event.service_provider_id,
            event.human_principal_id,
            event.grant_id,
            event.token_jti,
            event.event_data,
            &event.outcome,
            event.error_message.as_deref(),
            event.source_ip.as_deref(),
            event.user_agent.as_deref(),
            event.request_id,
            event.trace_id.as_deref(),
            &previous_hash,
            &row_hash,
            signature.as_bytes(),
        )
        .await?;

        // Update the previous hash
        *self.previous_hash.write().await = row_hash;

        Ok(event_id)
    }

    /// Build the content for hashing.
    fn build_hash_content(event_id: &Uuid, event: &AuditEvent) -> Vec<u8> {
        let mut hasher = Sha256::new();

        hasher.update(event_id.as_bytes());
        if let Some(ref et) = event.event_type {
            hasher.update(et.as_str().as_bytes());
        }
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
}
