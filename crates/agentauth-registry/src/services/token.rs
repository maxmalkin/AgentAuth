//! Token issuance and verification service.

use crate::config::TokenConfig;
use crate::db::{self, DbPool};
use crate::error::{RegistryError, Result};
use crate::services::CacheService;
use agentauth_core::{
    crypto::SigningBackend, AgentAccessToken, AgentId, BehavioralEnvelope, Capability, GrantId,
    HumanPrincipalId, ServiceProviderId, TokenId,
};
use chrono::{Duration as ChronoDuration, Utc};
use std::sync::Arc;
use uuid::Uuid;

/// Token service for issuing and managing tokens.
pub struct TokenService {
    /// Database pool.
    db: DbPool,
    /// Cache service.
    cache: Arc<CacheService>,
    /// Signing backend.
    signer: Arc<dyn SigningBackend>,
    /// Token configuration.
    config: TokenConfig,
}

impl TokenService {
    /// Create a new token service.
    pub fn new(
        db: DbPool,
        cache: Arc<CacheService>,
        signer: Arc<dyn SigningBackend>,
        config: TokenConfig,
    ) -> Self {
        Self {
            db,
            cache,
            signer,
            config,
        }
    }

    /// Issue a new token for an approved grant.
    ///
    /// This is idempotent: calling with the same grant within the idempotency window
    /// returns the same token JTI.
    pub async fn issue_token(
        &self,
        grant_id: &GrantId,
        agent_id: &AgentId,
        service_provider_id: Uuid,
        human_principal_id: Uuid,
        capabilities: Vec<Capability>,
        envelope: BehavioralEnvelope,
        token_binding: Option<Vec<u8>>,
    ) -> Result<AgentAccessToken> {
        // Check for existing token within idempotency window
        #[allow(clippy::cast_possible_wrap)]
        let window_start =
            Utc::now() - ChronoDuration::seconds(self.config.idempotency_window_secs as i64);

        if let Some(existing) = db::find_existing_token(self.db.primary(), grant_id, window_start).await? {
            // Return existing token (idempotent response)
            return Self::row_to_token(&existing, grant_id);
        }

        // Create new token
        let now = Utc::now();
        #[allow(clippy::cast_possible_wrap)]
        let expires_at = now + ChronoDuration::seconds(self.config.lifetime_secs as i64);

        let token = AgentAccessToken {
            jti: TokenId::new(),
            agent_id: *agent_id,
            human_principal_id: HumanPrincipalId::from_uuid(human_principal_id),
            service_provider_id: ServiceProviderId::from_uuid(service_provider_id),
            granted_capabilities: capabilities,
            behavioral_envelope: envelope,
            issued_at: now,
            expires_at,
            confirmation: None, // Token binding converted to confirmation if needed
            key_id: self.signer.key_id().to_string(),
        };

        // Store in database (with grant_id and token_binding as separate params)
        db::insert_token(
            self.db.primary(),
            &token,
            grant_id,
            self.signer.key_id(),
            token_binding.as_deref(),
        )
        .await?;

        // Cache for fast verification
        self.cache
            .cache_token(
                &token.jti,
                &service_provider_id.to_string(),
                expires_at.timestamp(),
                false,
            )
            .await?;

        Ok(token)
    }

    /// Revoke a token.
    pub async fn revoke_token(&self, jti: &TokenId, reason: Option<&str>) -> Result<()> {
        // Revoke in database
        let revoked = db::revoke_token(self.db.primary(), jti, reason).await?;

        if !revoked {
            // Check if token exists
            let token = db::get_token(self.db.read_replica(), jti).await?;
            match token {
                None => return Err(RegistryError::TokenNotFound(jti.to_string())),
                Some(t) if t.is_revoked => return Err(RegistryError::TokenAlreadyRevoked),
                _ => return Err(RegistryError::Internal("unexpected revocation failure".into())),
            }
        }

        // Mark revoked in cache
        self.cache.mark_token_revoked(jti).await?;

        Ok(())
    }

    /// Check if a token is revoked.
    pub async fn is_revoked(&self, jti: &TokenId) -> Result<bool> {
        // Check cache first
        if let Some(cached) = self.cache.get_cached_token(jti).await? {
            return Ok(cached.is_revoked);
        }

        // Fall back to database
        db::is_token_revoked(self.db.read_replica(), jti).await
    }

    /// Get token details.
    pub async fn get_token(&self, jti: &TokenId) -> Result<Option<AgentAccessToken>> {
        let row = db::get_token(self.db.read_replica(), jti).await?;
        row.map(|r| {
            let grant_id = GrantId::from_uuid(r.grant_id);
            Self::row_to_token(&r, &grant_id)
        })
        .transpose()
    }

    /// Convert a database row to a token.
    /// Note: grant_id is not part of AgentAccessToken, it's stored separately in the DB
    fn row_to_token(row: &db::TokenRow, _grant_id: &GrantId) -> Result<AgentAccessToken> {
        let capabilities: Vec<Capability> =
            serde_json::from_value(row.granted_capabilities.clone())
                .map_err(|e| RegistryError::Internal(format!("failed to parse capabilities: {e}")))?;

        let envelope: BehavioralEnvelope =
            serde_json::from_value(row.behavioral_envelope.clone())
                .map_err(|e| RegistryError::Internal(format!("failed to parse envelope: {e}")))?;

        Ok(AgentAccessToken {
            jti: TokenId::from_uuid(row.jti),
            agent_id: AgentId::from_uuid(row.agent_id),
            human_principal_id: HumanPrincipalId::from_uuid(row.human_principal_id),
            service_provider_id: ServiceProviderId::from_uuid(row.service_provider_id),
            granted_capabilities: capabilities,
            behavioral_envelope: envelope,
            issued_at: row.issued_at,
            expires_at: row.expires_at,
            confirmation: None, // Would need to convert from token_binding if needed
            key_id: row.key_id.clone(),
        })
    }
}
