//! Signing operations for manifests and tokens.

use crate::error::CryptoError;
use crate::types::{AgentAccessToken, AgentManifest, SignedAAT, SignedManifest};

use super::backend::SigningBackend;

/// Signs an agent manifest.
///
/// # Arguments
///
/// * `manifest` - The manifest to sign.
/// * `backend` - The signing backend to use.
///
/// # Errors
///
/// Returns an error if:
/// - Manifest validation fails
/// - Serialization fails
/// - Signing fails
pub async fn sign_manifest<B: SigningBackend>(
    manifest: &AgentManifest,
    backend: &B,
) -> Result<SignedManifest, CryptoError> {
    // Validate the manifest first
    manifest
        .validate()
        .map_err(|e| CryptoError::SigningFailed(format!("manifest validation failed: {e}")))?;

    // Get the canonical bytes
    let bytes = manifest
        .to_canonical_bytes()
        .map_err(|e| CryptoError::SigningFailed(format!("serialization failed: {e}")))?;

    // Sign the bytes
    let signature = backend.sign(&bytes).await?;

    Ok(SignedManifest {
        manifest: manifest.clone(),
        signature: signature.to_base64url(),
        signing_key_id: backend.key_id().to_string(),
    })
}

/// Signs an agent access token.
///
/// # Arguments
///
/// * `token` - The token to sign.
/// * `backend` - The signing backend to use.
///
/// # Errors
///
/// Returns an error if:
/// - Token validation fails
/// - Serialization fails
/// - Signing fails
pub async fn sign_token<B: SigningBackend>(
    token: &AgentAccessToken,
    backend: &B,
) -> Result<SignedAAT, CryptoError> {
    // Validate the token first
    token
        .validate()
        .map_err(|e| CryptoError::SigningFailed(format!("token validation failed: {e}")))?;

    // Get the canonical bytes
    let bytes = token
        .to_canonical_bytes()
        .map_err(|e| CryptoError::SigningFailed(format!("serialization failed: {e}")))?;

    // Sign the bytes
    let signature = backend.sign(&bytes).await?;

    Ok(SignedAAT {
        token: token.clone(),
        signature: signature.to_base64url(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::backend::{InMemorySigningBackend, SigningBackend};
    use crate::types::{
        AgentId, BehavioralEnvelope, Capability, HumanPrincipalId, ServiceProviderId, TokenId,
    };
    use chrono::Utc;

    fn create_test_manifest() -> AgentManifest {
        let now = Utc::now();
        AgentManifest {
            id: AgentId::new(),
            public_key: "test-public-key-base64url".to_string(),
            key_id: "key-1".to_string(),
            capabilities_requested: vec![Capability::Read {
                resource: "calendar".to_string(),
                filter: None,
            }],
            human_principal_id: HumanPrincipalId::new(),
            issued_at: now,
            expires_at: now + chrono::Duration::hours(24),
            name: "Test Agent".to_string(),
            description: Some("A test agent".to_string()),
            model_origin: Some("anthropic.com".to_string()),
        }
    }

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

    #[tokio::test]
    async fn test_sign_manifest() {
        let backend = InMemorySigningBackend::new_random();
        let manifest = create_test_manifest();

        let signed = sign_manifest(&manifest, &backend).await.expect("sign");

        assert!(!signed.signature.is_empty());
        assert_eq!(signed.signing_key_id, backend.key_id());
    }

    #[tokio::test]
    async fn test_sign_token() {
        let backend = InMemorySigningBackend::new_random();
        let token = create_test_token();

        let signed = sign_token(&token, &backend).await.expect("sign");

        assert!(!signed.signature.is_empty());
    }

    #[tokio::test]
    async fn test_sign_invalid_manifest_fails() {
        let backend = InMemorySigningBackend::new_random();
        let mut manifest = create_test_manifest();
        manifest.public_key = String::new(); // Invalid

        let result = sign_manifest(&manifest, &backend).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sign_invalid_token_fails() {
        let backend = InMemorySigningBackend::new_random();
        let mut token = create_test_token();
        token.key_id = String::new(); // Invalid

        let result = sign_token(&token, &backend).await;
        assert!(result.is_err());
    }
}
