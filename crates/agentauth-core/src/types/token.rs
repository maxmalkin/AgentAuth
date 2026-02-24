//! Token types for AgentAuth.
//!
//! This module defines the AgentAccessToken (AAT) and related types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::agent::{AgentId, HumanPrincipalId, ServiceProviderId};
use super::capability::Capability;
use super::envelope::BehavioralEnvelope;

/// Unique identifier for a token (UUID v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenId(pub Uuid);

impl TokenId {
    /// Creates a new token ID using UUID v7 (time-ordered).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a token ID from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TokenId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TokenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a grant (UUID v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrantId(pub Uuid);

impl GrantId {
    /// Creates a new grant ID using UUID v7 (time-ordered).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a grant ID from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the inner UUID.
    #[must_use]
    pub const fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for GrantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for GrantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Token binding (confirmation) claim for DPoP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenBinding {
    /// The JWK thumbprint of the DPoP public key.
    #[serde(rename = "jkt")]
    pub jwk_thumbprint: String,
}

/// The Agent Access Token (AAT).
///
/// This is the primary token type issued to agents after successful grant approval.
/// It contains the agent's granted capabilities and behavioral constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAccessToken {
    /// JWT ID - unique identifier for this token (UUID v7).
    pub jti: TokenId,

    /// The agent this token was issued to.
    pub agent_id: AgentId,

    /// The human principal who approved this grant.
    pub human_principal_id: HumanPrincipalId,

    /// The service provider this token is valid for.
    pub service_provider_id: ServiceProviderId,

    /// The capabilities granted to the agent.
    pub granted_capabilities: Vec<Capability>,

    /// The behavioral constraints for this token.
    pub behavioral_envelope: BehavioralEnvelope,

    /// When this token was issued.
    pub issued_at: DateTime<Utc>,

    /// When this token expires (max 15 minutes from issued_at).
    pub expires_at: DateTime<Utc>,

    /// Token binding for DPoP sender-constraint.
    #[serde(rename = "cnf", default, skip_serializing_if = "Option::is_none")]
    pub confirmation: Option<TokenBinding>,

    /// Key ID used for signing this token (for key rotation support).
    pub key_id: String,
}

impl AgentAccessToken {
    /// Maximum allowed token lifetime in seconds (15 minutes).
    pub const MAX_LIFETIME_SECS: i64 = 900;

    /// Validates the token's internal consistency.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Token lifetime exceeds 15 minutes
    /// - expires_at is before issued_at
    /// - No capabilities are granted
    /// - key_id is empty
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        let lifetime = self.expires_at.signed_duration_since(self.issued_at);

        if lifetime.num_seconds() <= 0 {
            return Err(crate::CoreError::Validation(
                "expires_at must be after issued_at".to_string(),
            ));
        }

        if lifetime.num_seconds() > Self::MAX_LIFETIME_SECS {
            return Err(crate::CoreError::Validation(format!(
                "token lifetime ({} seconds) exceeds maximum ({} seconds)",
                lifetime.num_seconds(),
                Self::MAX_LIFETIME_SECS
            )));
        }

        if self.granted_capabilities.is_empty() {
            return Err(crate::CoreError::Validation(
                "granted_capabilities cannot be empty".to_string(),
            ));
        }

        if self.key_id.is_empty() {
            return Err(crate::CoreError::Validation(
                "key_id cannot be empty".to_string(),
            ));
        }

        // Validate behavioral envelope
        self.behavioral_envelope.validate()?;

        // Validate each capability
        for cap in &self.granted_capabilities {
            cap.validate()?;
        }

        Ok(())
    }

    /// Checks if this token has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Checks if this token is within the refresh window (2 minutes before expiry).
    #[must_use]
    pub fn should_refresh(&self) -> bool {
        let refresh_threshold = self.expires_at - chrono::Duration::seconds(120);
        Utc::now() >= refresh_threshold
    }

    /// Serializes the token to canonical JSON bytes for signing.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, crate::CoreError> {
        let value = serde_json::to_value(self)
            .map_err(|e| crate::CoreError::Serialization(e.to_string()))?;
        serde_json::to_vec(&value).map_err(|e| crate::CoreError::Serialization(e.to_string()))
    }

    /// Checks if this token grants a specific capability type for a resource.
    #[must_use]
    pub fn has_capability(&self, cap_type: &str, resource: &str) -> bool {
        self.granted_capabilities.iter().any(|cap| {
            cap.capability_type() == cap_type && cap.resource() == resource
        })
    }
}

/// A signed Agent Access Token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedAAT {
    /// The token being signed.
    pub token: AgentAccessToken,

    /// Ed25519 signature over the canonical token bytes (base64url-encoded).
    pub signature: String,
}

impl SignedAAT {
    /// Returns the canonical bytes of the token for verification.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn token_bytes(&self) -> Result<Vec<u8>, crate::CoreError> {
        self.token.to_canonical_bytes()
    }
}

/// Human approval assertion for a grant.
///
/// This is signed by the human principal via WebAuthn/Passkey and proves
/// that the human explicitly approved the capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalAssertion {
    /// The grant being approved.
    pub grant_id: GrantId,

    /// The agent being granted capabilities.
    pub agent_id: AgentId,

    /// The exact capabilities being approved.
    /// Must match what was shown to the human in the approval UI.
    pub granted_capabilities: Vec<Capability>,

    /// The behavioral constraints being approved.
    pub behavioral_envelope: BehavioralEnvelope,

    /// When the approval was made.
    pub approved_at: DateTime<Utc>,

    /// Random nonce to prevent replay attacks.
    pub approval_nonce: [u8; 32],

    /// WebAuthn signature from the human principal (base64url-encoded).
    /// This is the signature over the assertion data.
    pub human_signature: String,

    /// WebAuthn credential ID used for signing (base64url-encoded).
    pub credential_id: String,

    /// WebAuthn authenticator data (base64url-encoded).
    pub authenticator_data: String,

    /// WebAuthn client data JSON hash (base64url-encoded).
    pub client_data_hash: String,
}

impl ApprovalAssertion {
    /// Validates the assertion's internal consistency.
    ///
    /// Note: This does NOT verify the WebAuthn signature - that must be done
    /// by the registry using the human principal's registered credentials.
    ///
    /// # Errors
    ///
    /// Returns an error if the assertion is internally inconsistent.
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        if self.granted_capabilities.is_empty() {
            return Err(crate::CoreError::Validation(
                "granted_capabilities cannot be empty".to_string(),
            ));
        }

        if self.human_signature.is_empty() {
            return Err(crate::CoreError::Validation(
                "human_signature cannot be empty".to_string(),
            ));
        }

        if self.credential_id.is_empty() {
            return Err(crate::CoreError::Validation(
                "credential_id cannot be empty".to_string(),
            ));
        }

        // Validate behavioral envelope
        self.behavioral_envelope.validate()?;

        // Validate each capability
        for cap in &self.granted_capabilities {
            cap.validate()?;
        }

        Ok(())
    }

    /// Serializes the assertion data (excluding signature) for verification.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_signable_bytes(&self) -> Result<Vec<u8>, crate::CoreError> {
        use sha2::{Digest, Sha256};

        // Create a signable representation without the signature itself
        let signable = serde_json::json!({
            "grant_id": self.grant_id,
            "agent_id": self.agent_id,
            "granted_capabilities": self.granted_capabilities,
            "behavioral_envelope": self.behavioral_envelope,
            "approved_at": self.approved_at,
            "approval_nonce": base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, self.approval_nonce),
        });

        let bytes = serde_json::to_vec(&signable)
            .map_err(|e| crate::CoreError::Serialization(e.to_string()))?;

        // Return SHA-256 hash of the signable data
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hasher.finalize().to_vec())
    }
}

/// Status of a capability grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantStatus {
    /// Grant is pending human approval.
    Pending,
    /// Grant has been approved.
    Approved,
    /// Grant has been denied.
    Denied,
    /// Grant has expired without action.
    Expired,
    /// Grant has been revoked.
    Revoked,
}

/// A capability grant record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGrant {
    /// Unique identifier for this grant.
    pub id: GrantId,

    /// The agent requesting capabilities.
    pub agent_id: AgentId,

    /// The service provider the capabilities are for.
    pub service_provider_id: ServiceProviderId,

    /// The human principal who must approve.
    pub human_principal_id: HumanPrincipalId,

    /// The capabilities being requested.
    pub requested_capabilities: Vec<Capability>,

    /// The behavioral envelope being requested.
    pub requested_envelope: BehavioralEnvelope,

    /// Current status of the grant.
    pub status: GrantStatus,

    /// When the grant request was created.
    pub created_at: DateTime<Utc>,

    /// When the grant expires (for pending requests).
    pub expires_at: DateTime<Utc>,

    /// When the grant was approved (if approved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,

    /// The approval assertion (if approved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_assertion: Option<ApprovalAssertion>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_token() -> AgentAccessToken {
        let now = Utc::now();
        AgentAccessToken {
            jti: TokenId::new(),
            agent_id: AgentId::new(),
            human_principal_id: HumanPrincipalId::new(),
            service_provider_id: ServiceProviderId::new(),
            granted_capabilities: vec![Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            }],
            behavioral_envelope: BehavioralEnvelope::default_restrictive(),
            issued_at: now,
            expires_at: now + chrono::Duration::minutes(15),
            confirmation: None,
            key_id: "key-1".to_string(),
        }
    }

    #[test]
    fn test_token_validation_success() {
        let token = create_test_token();
        assert!(token.validate().is_ok());
    }

    #[test]
    fn test_token_validation_exceeds_max_lifetime() {
        let now = Utc::now();
        let mut token = create_test_token();
        token.expires_at = now + chrono::Duration::minutes(20);
        assert!(token.validate().is_err());
    }

    #[test]
    fn test_token_validation_expires_before_issued() {
        let now = Utc::now();
        let mut token = create_test_token();
        token.expires_at = now - chrono::Duration::minutes(1);
        assert!(token.validate().is_err());
    }

    #[test]
    fn test_token_validation_empty_capabilities() {
        let mut token = create_test_token();
        token.granted_capabilities = vec![];
        assert!(token.validate().is_err());
    }

    #[test]
    fn test_token_validation_empty_key_id() {
        let mut token = create_test_token();
        token.key_id = String::new();
        assert!(token.validate().is_err());
    }

    #[test]
    fn test_token_is_expired() {
        let now = Utc::now();
        let mut token = create_test_token();
        token.expires_at = now - chrono::Duration::seconds(1);
        assert!(token.is_expired());
    }

    #[test]
    fn test_token_should_refresh() {
        let now = Utc::now();
        let mut token = create_test_token();
        token.expires_at = now + chrono::Duration::seconds(60); // 1 minute left
        assert!(token.should_refresh());

        token.expires_at = now + chrono::Duration::minutes(10); // 10 minutes left
        assert!(!token.should_refresh());
    }

    #[test]
    fn test_token_has_capability() {
        let token = create_test_token();
        assert!(token.has_capability("read", "calendar"));
        assert!(!token.has_capability("write", "calendar"));
        assert!(!token.has_capability("read", "email"));
    }

    #[test]
    fn test_token_serialization_deterministic() {
        let token = create_test_token();
        let bytes1 = token.to_canonical_bytes().expect("serialize");
        let bytes2 = token.to_canonical_bytes().expect("serialize");
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_grant_status_serialization() {
        let status = GrantStatus::Pending;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"pending\"");

        let deserialized: GrantStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, deserialized);
    }
}
