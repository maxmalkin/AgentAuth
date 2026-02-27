//! Agent-related types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::capability::Capability;

/// Unique identifier for an agent (UUID v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(pub Uuid);

impl AgentId {
    /// Creates a new agent ID using UUID v7 (time-ordered).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates an agent ID from an existing UUID.
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

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a human principal (UUID v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HumanPrincipalId(pub Uuid);

impl HumanPrincipalId {
    /// Creates a new human principal ID using UUID v7.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a human principal ID from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for HumanPrincipalId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for HumanPrincipalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a service provider (UUID v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ServiceProviderId(pub Uuid);

impl ServiceProviderId {
    /// Creates a new service provider ID using UUID v7.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Creates a service provider ID from an existing UUID.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ServiceProviderId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ServiceProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent identity document containing the agent's public key and requested capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Unique identifier for this agent.
    pub id: AgentId,

    /// Ed25519 public key (base64url-encoded).
    pub public_key: String,

    /// Key ID for key rotation support.
    pub key_id: String,

    /// Capabilities this agent is requesting.
    pub capabilities_requested: Vec<Capability>,

    /// Human principal who owns/controls this agent.
    pub human_principal_id: HumanPrincipalId,

    /// When this manifest was issued.
    pub issued_at: DateTime<Utc>,

    /// When this manifest expires.
    pub expires_at: DateTime<Utc>,

    /// Human-readable name for this agent.
    pub name: String,

    /// Optional description of this agent's purpose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Model origin (e.g., "anthropic.com", "openai.com").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_origin: Option<String>,
}

impl AgentManifest {
    /// Validates the manifest's internal consistency.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The expiry is before the issue time
    /// - The public key is empty
    /// - The key ID is empty
    /// - No capabilities are requested
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        if self.expires_at <= self.issued_at {
            return Err(crate::CoreError::Validation(
                "expires_at must be after issued_at".to_string(),
            ));
        }

        if self.public_key.is_empty() {
            return Err(crate::CoreError::Validation(
                "public_key cannot be empty".to_string(),
            ));
        }

        if self.key_id.is_empty() {
            return Err(crate::CoreError::Validation(
                "key_id cannot be empty".to_string(),
            ));
        }

        if self.capabilities_requested.is_empty() {
            return Err(crate::CoreError::Validation(
                "capabilities_requested cannot be empty".to_string(),
            ));
        }

        // Validate each capability
        for cap in &self.capabilities_requested {
            cap.validate()?;
        }

        Ok(())
    }

    /// Serializes the manifest to canonical JSON bytes for signing.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, crate::CoreError> {
        // Use serde_json with sorted keys for deterministic output
        let value = serde_json::to_value(self)
            .map_err(|e| crate::CoreError::Serialization(e.to_string()))?;
        serde_json::to_vec(&value).map_err(|e| crate::CoreError::Serialization(e.to_string()))
    }
}

/// A signed agent manifest with its signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    /// The manifest being signed.
    pub manifest: AgentManifest,

    /// Ed25519 signature over the canonical manifest bytes (base64url-encoded).
    pub signature: String,

    /// Key ID used for signing (for key rotation support).
    pub signing_key_id: String,
}

impl SignedManifest {
    /// Returns the canonical bytes of the manifest for verification.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn manifest_bytes(&self) -> Result<Vec<u8>, crate::CoreError> {
        self.manifest.to_canonical_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_new() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new();
        let display = format!("{id}");
        assert!(!display.is_empty());
    }

    #[test]
    fn test_manifest_validation_empty_public_key() {
        let manifest = AgentManifest {
            id: AgentId::new(),
            public_key: String::new(),
            key_id: "key-1".to_string(),
            capabilities_requested: vec![Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            }],
            human_principal_id: HumanPrincipalId::new(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(1),
            name: "Test Agent".to_string(),
            description: None,
            model_origin: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_validation_expires_before_issued() {
        let now = Utc::now();
        let manifest = AgentManifest {
            id: AgentId::new(),
            public_key: "test-key".to_string(),
            key_id: "key-1".to_string(),
            capabilities_requested: vec![Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            }],
            human_principal_id: HumanPrincipalId::new(),
            issued_at: now,
            expires_at: now - chrono::Duration::hours(1),
            name: "Test Agent".to_string(),
            description: None,
            model_origin: None,
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_serialization_deterministic() {
        let manifest = AgentManifest {
            id: AgentId::from_uuid(
                Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").expect("valid uuid"),
            ),
            public_key: "test-public-key".to_string(),
            key_id: "key-1".to_string(),
            capabilities_requested: vec![Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            }],
            human_principal_id: HumanPrincipalId::from_uuid(
                Uuid::parse_str("fedcba98-7654-3210-fedc-ba9876543210").expect("valid uuid"),
            ),
            issued_at: DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                .expect("valid date")
                .with_timezone(&Utc),
            expires_at: DateTime::parse_from_rfc3339("2025-01-01T01:00:00Z")
                .expect("valid date")
                .with_timezone(&Utc),
            name: "Test Agent".to_string(),
            description: None,
            model_origin: None,
        };

        let bytes1 = manifest.to_canonical_bytes().expect("serialization");
        let bytes2 = manifest.to_canonical_bytes().expect("serialization");

        assert_eq!(bytes1, bytes2, "Serialization must be deterministic");
    }
}
