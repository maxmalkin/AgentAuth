//! Grant management service.

use crate::config::GrantConfig;
use crate::db::{self, DbPool};
use crate::error::{RegistryError, Result};
use auth_core::{
    AgentId, BehavioralEnvelope, Capability, CapabilityGrant, GrantId, GrantStatus,
    HumanPrincipalId, ServiceProviderId,
};
use chrono::{Duration as ChronoDuration, Utc};
use uuid::Uuid;

/// Grant service for managing capability grants.
pub struct GrantService {
    /// Database pool.
    db: DbPool,
    /// Grant configuration.
    config: GrantConfig,
}

impl GrantService {
    /// Create a new grant service.
    pub fn new(db: DbPool, config: GrantConfig) -> Self {
        Self { db, config }
    }

    /// Request a new grant.
    pub async fn request_grant(
        &self,
        agent_id: &AgentId,
        service_provider_id: Uuid,
        capabilities: Vec<Capability>,
        envelope: BehavioralEnvelope,
    ) -> Result<CapabilityGrant> {
        // Check pending grant limit
        let pending_count = db::count_pending_grants(self.db.primary(), agent_id).await?;
        if pending_count >= i64::from(self.config.max_pending_per_agent) {
            return Err(RegistryError::TooManyPendingGrants);
        }

        // Get agent to find human_principal_id
        let agent = db::get_agent(self.db.read_replica(), agent_id)
            .await?
            .ok_or_else(|| RegistryError::AgentNotFound(agent_id.to_string()))?;

        // Return existing pending grant if one exists (idempotent across restarts)
        if let Some(existing) =
            db::get_pending_grant_for_agent_sp(self.db.read_replica(), agent_id, service_provider_id)
                .await?
        {
            return Self::row_to_grant(&existing);
        }

        // Create grant
        let grant_id = GrantId::new();
        let now = Utc::now();
        #[allow(clippy::cast_possible_wrap)]
        let expires_at = now + ChronoDuration::seconds(self.config.expiry_secs as i64);

        db::insert_grant(
            self.db.primary(),
            &grant_id,
            agent_id,
            service_provider_id,
            &capabilities,
            &envelope,
            expires_at,
        )
        .await?;

        Ok(CapabilityGrant {
            id: grant_id,
            agent_id: *agent_id,
            service_provider_id: ServiceProviderId::from_uuid(service_provider_id),
            human_principal_id: HumanPrincipalId::from_uuid(agent.human_principal_id),
            requested_capabilities: capabilities,
            requested_envelope: envelope,
            status: GrantStatus::Pending,
            created_at: now,
            expires_at,
            approved_at: None,
            approval_assertion: None,
        })
    }

    /// Get a grant by ID.
    pub async fn get_grant(&self, grant_id: &GrantId) -> Result<Option<CapabilityGrant>> {
        let row = db::get_grant(self.db.read_replica(), grant_id).await?;
        row.map(|r| Self::row_to_grant(&r)).transpose()
    }

    /// Get a grant by ID, returning the raw database row with joined names.
    pub async fn get_grant_row(&self, grant_id: &GrantId) -> Result<Option<db::GrantRow>> {
        db::get_grant(self.db.read_replica(), grant_id).await
    }

    /// Approve a grant.
    pub async fn approve_grant(
        &self,
        grant_id: &GrantId,
        approved_by: Uuid,
        approval_nonce: &[u8],
        approval_signature: &[u8],
    ) -> Result<CapabilityGrant> {
        // Get the grant first
        let grant = self
            .get_grant(grant_id)
            .await?
            .ok_or_else(|| RegistryError::GrantNotFound(grant_id.to_string()))?;

        // Check if grant is pending
        if grant.status != GrantStatus::Pending {
            return Err(RegistryError::GrantNotPending);
        }

        // Check if grant has expired
        if Utc::now() > grant.expires_at {
            return Err(RegistryError::GrantExpired);
        }

        // Approve in database
        let approved = db::approve_grant(
            self.db.primary(),
            grant_id,
            approved_by,
            approval_nonce,
            approval_signature,
        )
        .await?;

        if !approved {
            return Err(RegistryError::GrantNotPending);
        }

        // Return updated grant
        // Note: approval_assertion would need full WebAuthn data to populate
        let updated = CapabilityGrant {
            status: GrantStatus::Approved,
            approved_at: Some(Utc::now()),
            ..grant
        };

        Ok(updated)
    }

    /// Deny a grant.
    pub async fn deny_grant(&self, grant_id: &GrantId) -> Result<CapabilityGrant> {
        // Get the grant first
        let grant = self
            .get_grant(grant_id)
            .await?
            .ok_or_else(|| RegistryError::GrantNotFound(grant_id.to_string()))?;

        // Check if grant is pending
        if grant.status != GrantStatus::Pending {
            return Err(RegistryError::GrantNotPending);
        }

        // Deny in database
        let denied = db::deny_grant(self.db.primary(), grant_id).await?;

        if !denied {
            return Err(RegistryError::GrantNotPending);
        }

        // Return updated grant
        Ok(CapabilityGrant {
            status: GrantStatus::Denied,
            ..grant
        })
    }

    /// Revoke a grant.
    pub async fn revoke_grant(&self, grant_id: &GrantId) -> Result<()> {
        let revoked = db::revoke_grant(self.db.primary(), grant_id).await?;
        if !revoked {
            let grant = self.get_grant(grant_id).await?;
            match grant {
                None => return Err(RegistryError::GrantNotFound(grant_id.to_string())),
                Some(g) if g.status != GrantStatus::Approved => {
                    return Err(RegistryError::GrantNotPending);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Convert a database row to a grant.
    fn row_to_grant(row: &db::GrantRow) -> Result<CapabilityGrant> {
        let capabilities: Vec<Capability> =
            serde_json::from_value(row.granted_capabilities.clone()).map_err(|e| {
                RegistryError::Internal(format!("failed to parse capabilities: {e}"))
            })?;

        let envelope: BehavioralEnvelope = serde_json::from_value(row.behavioral_envelope.clone())
            .map_err(|e| RegistryError::Internal(format!("failed to parse envelope: {e}")))?;

        let status = match row.status.as_str() {
            "pending" => GrantStatus::Pending,
            "approved" => GrantStatus::Approved,
            "denied" => GrantStatus::Denied,
            "revoked" => GrantStatus::Revoked,
            "expired" => GrantStatus::Expired,
            _ => {
                return Err(RegistryError::Internal(format!(
                    "unknown grant status: {}",
                    row.status
                )))
            }
        };

        Ok(CapabilityGrant {
            id: GrantId::from_uuid(row.id),
            agent_id: AgentId::from_uuid(row.agent_id),
            service_provider_id: ServiceProviderId::from_uuid(row.service_provider_id),
            human_principal_id: HumanPrincipalId::from_uuid(row.human_principal_id),
            requested_capabilities: capabilities,
            requested_envelope: envelope,
            status,
            created_at: row.requested_at,
            expires_at: row.expires_at,
            approved_at: row.decided_at,
            approval_assertion: None, // Would need to reconstruct from nonce/signature if needed
        })
    }
}
