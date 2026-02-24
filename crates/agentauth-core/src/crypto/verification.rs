//! Verification operations for manifests and tokens.
//!
//! All verification uses timing-safe comparison to prevent timing attacks.

use crate::error::CryptoError;
use crate::types::{SignedManifest, SignedAAT};

use super::backend::{Ed25519PublicKey, Signature};

/// Verifies a signed agent manifest.
///
/// # Arguments
///
/// * `signed` - The signed manifest to verify.
/// * `public_key` - The public key to verify against.
///
/// # Errors
///
/// Returns an error if:
/// - Signature decoding fails
/// - Serialization fails
/// - Signature verification fails
///
/// # Security
///
/// This function uses timing-safe comparison for the signature verification.
pub fn verify_manifest(
    signed: &SignedManifest,
    public_key: &Ed25519PublicKey,
) -> Result<(), CryptoError> {
    // Decode the signature
    let signature = Signature::from_base64url(&signed.signature)?;

    // Get the canonical bytes
    let bytes = signed
        .manifest_bytes()
        .map_err(|e| CryptoError::VerificationFailed(format!("serialization failed: {e}")))?;

    // Verify using ed25519-dalek with timing-safe comparison
    verify_ed25519(&bytes, &signature, public_key)
}

/// Verifies a signed agent access token.
///
/// # Arguments
///
/// * `signed` - The signed token to verify.
/// * `public_key` - The public key to verify against.
///
/// # Errors
///
/// Returns an error if:
/// - Signature decoding fails
/// - Serialization fails
/// - Signature verification fails
///
/// # Security
///
/// This function uses timing-safe comparison for the signature verification.
pub fn verify_token(
    signed: &SignedAAT,
    public_key: &Ed25519PublicKey,
) -> Result<(), CryptoError> {
    // Decode the signature
    let signature = Signature::from_base64url(&signed.signature)?;

    // Get the canonical bytes
    let bytes = signed
        .token_bytes()
        .map_err(|e| CryptoError::VerificationFailed(format!("serialization failed: {e}")))?;

    // Verify using ed25519-dalek with timing-safe comparison
    verify_ed25519(&bytes, &signature, public_key)
}

/// Performs Ed25519 signature verification with timing-safe comparison.
fn verify_ed25519(
    message: &[u8],
    signature: &Signature,
    public_key: &Ed25519PublicKey,
) -> Result<(), CryptoError> {
    use ed25519_dalek::{Signature as DalekSig, Verifier, VerifyingKey};

    // Parse the verifying key
    let vk = VerifyingKey::from_bytes(&public_key.0)
        .map_err(|e| CryptoError::InvalidKeyFormat(e.to_string()))?;

    // Parse the signature
    let sig = DalekSig::from_bytes(&signature.0);

    // Verify - ed25519-dalek uses constant-time comparison internally
    vk.verify(message, &sig)
        .map_err(|_| CryptoError::VerificationFailed("signature verification failed".to_string()))
}

/// Verifies that a key_id matches the expected value.
///
/// # Security
///
/// This function uses constant-time comparison to prevent timing attacks.
#[must_use]
pub fn verify_key_id(expected: &str, actual: &str) -> bool {
    use subtle::ConstantTimeEq;
    expected.as_bytes().ct_eq(actual.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::backend::InMemorySigningBackend;
    use crate::crypto::signing::{sign_manifest, sign_token};
    use crate::types::{
        AgentId, AgentManifest, AgentAccessToken, BehavioralEnvelope, Capability,
        HumanPrincipalId, ServiceProviderId, TokenId,
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
            description: None,
            model_origin: None,
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
    async fn test_verify_manifest_valid() {
        let backend = InMemorySigningBackend::new_random();
        let manifest = create_test_manifest();

        let signed = sign_manifest(&manifest, &backend).await.expect("sign");
        let public_key = backend.public_key().await.expect("public key");

        assert!(verify_manifest(&signed, &public_key).is_ok());
    }

    #[tokio::test]
    async fn test_verify_manifest_wrong_key() {
        let backend1 = InMemorySigningBackend::new_random();
        let backend2 = InMemorySigningBackend::new_random();
        let manifest = create_test_manifest();

        let signed = sign_manifest(&manifest, &backend1).await.expect("sign");
        let wrong_key = backend2.public_key().await.expect("wrong key");

        assert!(verify_manifest(&signed, &wrong_key).is_err());
    }

    #[tokio::test]
    async fn test_verify_manifest_tampered() {
        let backend = InMemorySigningBackend::new_random();
        let manifest = create_test_manifest();

        let mut signed = sign_manifest(&manifest, &backend).await.expect("sign");
        signed.manifest.name = "Tampered Name".to_string();

        let public_key = backend.public_key().await.expect("public key");

        assert!(verify_manifest(&signed, &public_key).is_err());
    }

    #[tokio::test]
    async fn test_verify_token_valid() {
        let backend = InMemorySigningBackend::new_random();
        let token = create_test_token();

        let signed = sign_token(&token, &backend).await.expect("sign");
        let public_key = backend.public_key().await.expect("public key");

        assert!(verify_token(&signed, &public_key).is_ok());
    }

    #[tokio::test]
    async fn test_verify_token_wrong_key() {
        let backend1 = InMemorySigningBackend::new_random();
        let backend2 = InMemorySigningBackend::new_random();
        let token = create_test_token();

        let signed = sign_token(&token, &backend1).await.expect("sign");
        let wrong_key = backend2.public_key().await.expect("wrong key");

        assert!(verify_token(&signed, &wrong_key).is_err());
    }

    #[tokio::test]
    async fn test_verify_token_tampered_key_id() {
        let backend = InMemorySigningBackend::new_random();
        let token = create_test_token();

        let mut signed = sign_token(&token, &backend).await.expect("sign");
        signed.token.key_id = "tampered-key-id".to_string();

        let public_key = backend.public_key().await.expect("public key");

        // Signature verification should fail because the signed bytes don't match
        assert!(verify_token(&signed, &public_key).is_err());
    }

    #[test]
    fn test_verify_key_id_equal() {
        assert!(verify_key_id("key-1", "key-1"));
    }

    #[test]
    fn test_verify_key_id_not_equal() {
        assert!(!verify_key_id("key-1", "key-2"));
    }

    #[test]
    fn test_verify_key_id_different_lengths() {
        assert!(!verify_key_id("key-1", "key-12"));
    }
}
