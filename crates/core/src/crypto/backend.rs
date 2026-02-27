//! Signing backend implementations.
//!
//! This module defines the [`SigningBackend`] trait and its implementations.
//!
//! # Production Backends
//!
//! - [`KmsSigningBackend`] - AWS KMS, GCP Cloud KMS, or HashiCorp Vault Transit
//!
//! # Development/Test Backends
//!
//! - In-memory signing backend - Only available in `#[cfg(test)]`
//! - Encrypted keyfile - For local development only (emits warning)

use std::path::PathBuf;

use crate::error::CryptoError;

/// Ed25519 public key (32 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ed25519PublicKey(pub [u8; 32]);

impl Ed25519PublicKey {
    /// Creates a public key from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not exactly 32 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKeyFormat(format!(
                "expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Returns the raw bytes of the public key.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Encodes the public key as base64url (no padding).
    #[must_use]
    pub fn to_base64url(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Decodes a public key from base64url.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails or the decoded length is wrong.
    pub fn from_base64url(encoded: &str) -> Result<Self, CryptoError> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|e| CryptoError::InvalidKeyFormat(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

/// Ed25519 signature (64 bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl Signature {
    /// Creates a signature from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the bytes are not exactly 64 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 64 {
            return Err(CryptoError::VerificationFailed(format!(
                "expected 64 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(bytes);
        Ok(Self(arr))
    }

    /// Returns the raw bytes of the signature.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Encodes the signature as base64url (no padding).
    #[must_use]
    pub fn to_base64url(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Decodes a signature from base64url.
    ///
    /// # Errors
    ///
    /// Returns an error if decoding fails or the decoded length is wrong.
    pub fn from_base64url(encoded: &str) -> Result<Self, CryptoError> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|e| CryptoError::VerificationFailed(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}

/// Trait for signing backends.
///
/// Implementations must be thread-safe (`Send + Sync`) and should use
/// async operations for any I/O (e.g., KMS calls).
#[async_trait::async_trait]
pub trait SigningBackend: Send + Sync {
    /// Signs the given message and returns the signature.
    ///
    /// # Errors
    ///
    /// Returns an error if signing fails (e.g., KMS unavailable).
    async fn sign(&self, message: &[u8]) -> Result<Signature, CryptoError>;

    /// Returns the public key for verification.
    ///
    /// # Errors
    ///
    /// Returns an error if the public key cannot be retrieved.
    async fn public_key(&self) -> Result<Ed25519PublicKey, CryptoError>;

    /// Returns the key ID for key rotation support.
    fn key_id(&self) -> &str;
}

/// Agent key backend configuration.
///
/// This enum specifies how an agent's private key is stored/accessed.
#[derive(Debug, Clone)]
pub enum AgentKeyBackend {
    /// AWS KMS key.
    AwsKms {
        /// The KMS key ID or ARN.
        key_id: String,
    },

    /// Google Cloud KMS key.
    GcpKms {
        /// The full resource name (e.g., `projects/*/locations/*/keyRings/*/cryptoKeys/*/cryptoKeyVersions/*`).
        key_resource_name: String,
    },

    /// HashiCorp Vault Transit secrets engine.
    VaultTransit {
        /// The transit mount path.
        mount: String,
        /// The key name.
        key_name: String,
    },

    /// Encrypted keyfile (local development only).
    /// Emits a warning at startup.
    EncryptedKeyfile {
        /// Path to the encrypted keyfile.
        path: PathBuf,
    },

    /// Plaintext keyfile (NEVER use in production).
    /// Only available with the `allow-plaintext-keys` feature.
    #[cfg(feature = "allow-plaintext-keys")]
    PlaintextKeyfile {
        /// Path to the plaintext keyfile.
        path: PathBuf,
    },
}

impl AgentKeyBackend {
    /// Returns true if this backend is safe for production use.
    #[must_use]
    pub fn is_production_safe(&self) -> bool {
        matches!(
            self,
            AgentKeyBackend::AwsKms { .. }
                | AgentKeyBackend::GcpKms { .. }
                | AgentKeyBackend::VaultTransit { .. }
        )
    }

    /// Emits appropriate warnings for non-production backends.
    pub fn warn_if_not_production(&self) {
        match self {
            AgentKeyBackend::EncryptedKeyfile { path } => {
                tracing::warn!(
                    path = %path.display(),
                    "Using EncryptedKeyfile backend - NOT SAFE FOR PRODUCTION"
                );
            }
            #[cfg(feature = "allow-plaintext-keys")]
            AgentKeyBackend::PlaintextKeyfile { path } => {
                tracing::error!(
                    path = %path.display(),
                    "Using PlaintextKeyfile backend - CRITICAL SECURITY RISK - NEVER USE IN PRODUCTION"
                );
            }
            _ => {}
        }
    }
}

/// KMS signing backend for production use.
///
/// This backend delegates signing operations to a Key Management Service
/// (AWS KMS, GCP Cloud KMS, or HashiCorp Vault Transit).
pub struct KmsSigningBackend {
    /// The key backend configuration.
    backend: AgentKeyBackend,
    /// Cached public key.
    cached_public_key: Option<Ed25519PublicKey>,
    /// Key ID for key rotation.
    key_id: String,
}

impl KmsSigningBackend {
    /// Creates a new KMS signing backend.
    ///
    /// # Arguments
    ///
    /// * `backend` - The key backend configuration (must be a KMS variant).
    /// * `key_id` - The key ID for key rotation support.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend is not a KMS variant.
    pub fn new(backend: AgentKeyBackend, key_id: String) -> Result<Self, CryptoError> {
        if !backend.is_production_safe() {
            return Err(CryptoError::KmsError(
                "KmsSigningBackend requires a production-safe backend (AwsKms, GcpKms, or VaultTransit)".to_string()
            ));
        }

        Ok(Self {
            backend,
            cached_public_key: None,
            key_id,
        })
    }
}

#[async_trait::async_trait]
impl SigningBackend for KmsSigningBackend {
    async fn sign(&self, _message: &[u8]) -> Result<Signature, CryptoError> {
        // In a real implementation, this would call the appropriate KMS API
        // For now, we return an error indicating this needs implementation
        match &self.backend {
            AgentKeyBackend::AwsKms { key_id } => {
                // Would use aws-sdk-kms here
                Err(CryptoError::KmsError(format!(
                    "AWS KMS signing not yet implemented for key: {key_id}"
                )))
            }
            AgentKeyBackend::GcpKms { key_resource_name } => {
                // Would use google-cloud-kms here
                Err(CryptoError::KmsError(format!(
                    "GCP KMS signing not yet implemented for key: {key_resource_name}"
                )))
            }
            AgentKeyBackend::VaultTransit { mount, key_name } => {
                // Would use vault-client here
                Err(CryptoError::KmsError(format!(
                    "Vault Transit signing not yet implemented for key: {mount}/{key_name}"
                )))
            }
            AgentKeyBackend::EncryptedKeyfile { .. } => Err(CryptoError::KmsError(
                "EncryptedKeyfile is not supported by KmsSigningBackend".to_string(),
            )),
            #[cfg(feature = "allow-plaintext-keys")]
            AgentKeyBackend::PlaintextKeyfile { .. } => Err(CryptoError::KmsError(
                "PlaintextKeyfile is not supported by KmsSigningBackend".to_string(),
            )),
        }
    }

    async fn public_key(&self) -> Result<Ed25519PublicKey, CryptoError> {
        if let Some(ref cached) = self.cached_public_key {
            return Ok(cached.clone());
        }

        // In a real implementation, this would fetch the public key from KMS
        Err(CryptoError::KmsError(
            "KMS public key fetch not yet implemented".to_string(),
        ))
    }

    fn key_id(&self) -> &str {
        &self.key_id
    }
}

/// In-memory signing backend for testing only.
///
/// **WARNING**: This backend stores the private key in memory and must NEVER
/// be used in production. It is only available in test builds.
#[cfg(test)]
pub struct InMemorySigningBackend {
    /// The signing key.
    signing_key: ed25519_dalek::SigningKey,
    /// Key ID for key rotation.
    key_id: String,
}

#[cfg(test)]
impl InMemorySigningBackend {
    /// Creates a new in-memory signing backend with a random key.
    #[must_use]
    pub fn new_random() -> Self {
        use rand::rngs::OsRng;
        Self {
            signing_key: ed25519_dalek::SigningKey::generate(&mut OsRng),
            key_id: format!("test-key-{}", uuid::Uuid::now_v7()),
        }
    }

    /// Creates a new in-memory signing backend from seed bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the seed is invalid.
    pub fn from_seed(seed: &[u8; 32], key_id: String) -> Result<Self, CryptoError> {
        Ok(Self {
            signing_key: ed25519_dalek::SigningKey::from_bytes(seed),
            key_id,
        })
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl SigningBackend for InMemorySigningBackend {
    async fn sign(&self, message: &[u8]) -> Result<Signature, CryptoError> {
        use ed25519_dalek::Signer;
        let sig = self.signing_key.sign(message);
        Ok(Signature(sig.to_bytes()))
    }

    async fn public_key(&self) -> Result<Ed25519PublicKey, CryptoError> {
        let pk = self.signing_key.verifying_key();
        Ok(Ed25519PublicKey(pk.to_bytes()))
    }

    fn key_id(&self) -> &str {
        &self.key_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_backend_sign_and_verify() {
        let backend = InMemorySigningBackend::new_random();
        let message = b"test message";

        let signature = backend.sign(message).await.expect("sign");
        let public_key = backend.public_key().await.expect("public key");

        // Verify using ed25519_dalek directly
        use ed25519_dalek::{Signature as DalekSig, Verifier, VerifyingKey};
        let vk = VerifyingKey::from_bytes(&public_key.0).expect("verifying key");
        let sig = DalekSig::from_bytes(&signature.0);
        assert!(vk.verify(message, &sig).is_ok());
    }

    #[tokio::test]
    async fn test_in_memory_backend_different_keys() {
        let backend1 = InMemorySigningBackend::new_random();
        let backend2 = InMemorySigningBackend::new_random();

        let pk1 = backend1.public_key().await.expect("pk1");
        let pk2 = backend2.public_key().await.expect("pk2");

        assert_ne!(pk1, pk2);
    }

    #[test]
    fn test_public_key_base64_roundtrip() {
        let bytes = [42u8; 32];
        let pk = Ed25519PublicKey(bytes);
        let encoded = pk.to_base64url();
        let decoded = Ed25519PublicKey::from_base64url(&encoded).expect("decode");
        assert_eq!(pk, decoded);
    }

    #[test]
    fn test_signature_base64_roundtrip() {
        let bytes = [42u8; 64];
        let sig = Signature(bytes);
        let encoded = sig.to_base64url();
        let decoded = Signature::from_base64url(&encoded).expect("decode");
        assert_eq!(sig, decoded);
    }

    #[test]
    fn test_agent_key_backend_production_safety() {
        assert!(AgentKeyBackend::AwsKms {
            key_id: "test".to_string()
        }
        .is_production_safe());

        assert!(AgentKeyBackend::GcpKms {
            key_resource_name: "test".to_string()
        }
        .is_production_safe());

        assert!(AgentKeyBackend::VaultTransit {
            mount: "transit".to_string(),
            key_name: "test".to_string()
        }
        .is_production_safe());

        assert!(!AgentKeyBackend::EncryptedKeyfile {
            path: PathBuf::from("/tmp/key")
        }
        .is_production_safe());
    }
}
