//! Signing backend creation for the registry service.
//!
//! This module handles creation of signing backends based on configuration.
//! In production, this should use KMS backends. For development, it uses
//! an encrypted keyfile backend.

use agentauth_core::crypto::{Ed25519PublicKey, Signature, SigningBackend};
use agentauth_core::error::CryptoError;
use agentauth_registry::config::KmsBackend;
use std::sync::Arc;
use tracing::warn;

/// Create a signing backend based on configuration.
#[allow(clippy::unused_async)] // Will be async when KMS backends are implemented
pub async fn create_signing_backend(
    config: &agentauth_registry::config::KmsConfig,
) -> Result<Arc<dyn SigningBackend>, anyhow::Error> {
    match &config.backend {
        KmsBackend::AwsKms { region: _ } => {
            // TODO: Implement AWS KMS backend
            Err(anyhow::anyhow!("AWS KMS backend not yet implemented"))
        }
        KmsBackend::GcpKms {
            project_id: _,
            location: _,
            key_ring: _,
        } => {
            // TODO: Implement GCP KMS backend
            Err(anyhow::anyhow!("GCP KMS backend not yet implemented"))
        }
        KmsBackend::VaultTransit {
            address: _,
            mount: _,
        } => {
            // TODO: Implement Vault Transit backend
            Err(anyhow::anyhow!("Vault Transit backend not yet implemented"))
        }
        KmsBackend::EncryptedKeyfile { path } => {
            // Development-only: use encrypted keyfile
            warn!(
                path = %path,
                "Using EncryptedKeyfile signing backend - NOT FOR PRODUCTION USE"
            );
            // For now, create a development backend
            // In a real implementation, this would read and decrypt the keyfile
            Ok(Arc::new(DevelopmentSigningBackend::new(&config.signing_key_id)))
        }
    }
}

/// Development signing backend.
///
/// **WARNING**: This backend generates a random key at startup and must NEVER
/// be used in production. It is only for local development and testing.
pub struct DevelopmentSigningBackend {
    /// The signing key.
    signing_key: ed25519_dalek::SigningKey,
    /// Key ID for key rotation.
    key_id: String,
}

impl DevelopmentSigningBackend {
    /// Creates a new development signing backend with a random key.
    #[must_use]
    pub fn new(key_id: &str) -> Self {
        use rand::rngs::OsRng;
        warn!("DevelopmentSigningBackend: generating random key - NOT FOR PRODUCTION");
        Self {
            signing_key: ed25519_dalek::SigningKey::generate(&mut OsRng),
            key_id: key_id.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl SigningBackend for DevelopmentSigningBackend {
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
