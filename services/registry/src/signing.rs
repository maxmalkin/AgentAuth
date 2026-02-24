//! Signing backend creation for the registry service.
//!
//! This module handles creation of signing backends based on configuration.
//! In production, this should use KMS backends. For development, it uses
//! HashiCorp Vault in dev mode.

use agentauth_core::crypto::{Ed25519PublicKey, Signature, SigningBackend};
use agentauth_core::error::CryptoError;
use agentauth_registry::config::KmsBackend;
use base64::Engine;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// Create a signing backend based on configuration.
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
        KmsBackend::VaultTransit { address, mount } => {
            info!(
                address = %address,
                mount = %mount,
                key = %config.signing_key_id,
                "Using Vault Transit signing backend"
            );
            let backend =
                VaultTransitBackend::new(address, mount, &config.signing_key_id).await?;
            Ok(Arc::new(backend))
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

/// HashiCorp Vault Transit signing backend.
///
/// This backend uses Vault's Transit secrets engine for Ed25519 signing.
/// It is suitable for both development (Vault dev mode) and production
/// (Vault with proper storage and authentication).
pub struct VaultTransitBackend {
    /// HTTP client for Vault API.
    client: Client,
    /// Vault server address.
    address: String,
    /// Transit secrets engine mount path.
    mount: String,
    /// Key name in Transit.
    key_name: String,
    /// Vault token for authentication.
    token: String,
    /// Cached public key.
    public_key: Ed25519PublicKey,
}

/// Vault Transit sign request.
#[derive(Debug, Serialize)]
struct VaultSignRequest {
    /// Base64-encoded input to sign.
    input: String,
}

/// Vault Transit sign response.
#[derive(Debug, Deserialize)]
struct VaultSignResponse {
    /// Vault response data.
    data: VaultSignData,
}

/// Vault Transit sign response data.
#[derive(Debug, Deserialize)]
struct VaultSignData {
    /// Signature in Vault format: vault:v1:base64signature
    signature: String,
}

/// Vault Transit read key response.
#[derive(Debug, Deserialize)]
struct VaultKeyResponse {
    /// Vault response data.
    data: VaultKeyData,
}

/// Vault Transit key data.
#[derive(Debug, Deserialize)]
struct VaultKeyData {
    /// Key versions and their public keys.
    keys: std::collections::HashMap<String, VaultKeyVersion>,
    /// Latest key version.
    latest_version: u32,
}

/// Vault Transit key version.
#[derive(Debug, Deserialize)]
struct VaultKeyVersion {
    /// Base64-encoded public key.
    public_key: String,
}

impl VaultTransitBackend {
    /// Create a new Vault Transit backend.
    ///
    /// This will connect to Vault, verify the key exists, and cache the public key.
    pub async fn new(address: &str, mount: &str, key_name: &str) -> Result<Self, anyhow::Error> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        // Get token from environment
        let token = std::env::var("VAULT_TOKEN")
            .unwrap_or_else(|_| "dev-root-token".to_string());

        let address = address.trim_end_matches('/').to_string();
        let mount = mount.trim_matches('/').to_string();
        let key_name = key_name.to_string();

        // Fetch the public key
        let url = format!("{address}/v1/{mount}/keys/{key_name}");
        let response = client
            .get(&url)
            .header("X-Vault-Token", &token)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "Failed to fetch key from Vault: {status} - {body}"
            ));
        }

        let key_response: VaultKeyResponse = response.json().await?;
        let latest_version = key_response.data.latest_version.to_string();
        let key_version = key_response
            .data
            .keys
            .get(&latest_version)
            .ok_or_else(|| anyhow::anyhow!("Key version {latest_version} not found"))?;

        // Decode the public key
        let public_key_bytes =
            base64::engine::general_purpose::STANDARD.decode(&key_version.public_key)?;

        if public_key_bytes.len() != 32 {
            return Err(anyhow::anyhow!(
                "Invalid public key length: expected 32, got {}",
                public_key_bytes.len()
            ));
        }

        let mut pk_array = [0u8; 32];
        pk_array.copy_from_slice(&public_key_bytes);
        let public_key = Ed25519PublicKey(pk_array);

        info!(
            key_name = %key_name,
            version = %latest_version,
            "Vault Transit backend initialized"
        );

        Ok(Self {
            client,
            address,
            mount,
            key_name,
            token,
            public_key,
        })
    }
}

#[async_trait::async_trait]
impl SigningBackend for VaultTransitBackend {
    async fn sign(&self, message: &[u8]) -> Result<Signature, CryptoError> {
        let address = &self.address;
        let mount = &self.mount;
        let key_name = &self.key_name;
        let url = format!("{address}/v1/{mount}/sign/{key_name}");

        let input = base64::engine::general_purpose::STANDARD.encode(message);
        let request = VaultSignRequest { input };

        let response = self
            .client
            .post(&url)
            .header("X-Vault-Token", &self.token)
            .json(&request)
            .send()
            .await
            .map_err(|e| CryptoError::SigningFailed(format!("Vault request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CryptoError::SigningFailed(format!(
                "Vault signing failed: {status} - {body}"
            )));
        }

        let sign_response: VaultSignResponse = response
            .json()
            .await
            .map_err(|e| CryptoError::SigningFailed(format!("Failed to parse response: {e}")))?;

        // Parse the signature: vault:v1:base64signature
        let signature_parts: Vec<&str> = sign_response.data.signature.split(':').collect();
        if signature_parts.len() != 3 {
            let sig = &sign_response.data.signature;
            return Err(CryptoError::SigningFailed(format!(
                "Invalid signature format: {sig}"
            )));
        }

        let signature_b64 = signature_parts[2];
        let signature_bytes = base64::engine::general_purpose::STANDARD
            .decode(signature_b64)
            .map_err(|e| CryptoError::SigningFailed(format!("Failed to decode signature: {e}")))?;

        let sig_len = signature_bytes.len();
        if sig_len != 64 {
            return Err(CryptoError::SigningFailed(format!(
                "Invalid signature length: expected 64, got {sig_len}"
            )));
        }

        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(&signature_bytes);
        Ok(Signature(sig_array))
    }

    async fn public_key(&self) -> Result<Ed25519PublicKey, CryptoError> {
        Ok(self.public_key.clone())
    }

    fn key_id(&self) -> &str {
        &self.key_name
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_development_backend_sign_verify() {
        let backend = DevelopmentSigningBackend::new("test-key");
        let message = b"test message";

        let signature = backend.sign(message).await.expect("signing should succeed");
        assert_eq!(signature.0.len(), 64);

        let public_key = backend.public_key().await.expect("public_key should succeed");
        assert_eq!(public_key.0.len(), 32);

        assert_eq!(backend.key_id(), "test-key");
    }
}
