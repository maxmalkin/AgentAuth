//! Cryptographic operations for AgentAuth.
//!
//! This module provides signing backends and verification functions.
//! Production deployments must use [`KmsSigningBackend`].
//!
//! # Security Notes
//!
//! - All signature verification uses timing-safe comparison via `subtle`
//! - [`InMemorySigningBackend`] is only available in test builds
//! - Never log key material, signatures, or nonces

pub mod backend;
pub mod signing;
pub mod verification;

pub use backend::{AgentKeyBackend, Ed25519PublicKey, Signature, SigningBackend};
pub use signing::{sign_manifest, sign_token};
pub use verification::{verify_key_id, verify_manifest, verify_token};

/// Returns the canonical bytes for manifest signing.
///
/// This is useful when you need to verify a manifest signature
/// with raw signature bytes rather than a SignedManifest wrapper.
pub fn manifest_signing_bytes(manifest: &crate::types::AgentManifest) -> Vec<u8> {
    manifest
        .to_canonical_bytes()
        .unwrap_or_default()
}

/// Verifies an agent manifest with raw signature bytes.
///
/// This is a convenience function for verifying manifests when you have
/// the signature as raw bytes rather than a SignedManifest wrapper.
///
/// # Arguments
///
/// * `manifest` - The manifest to verify
/// * `signature` - Raw signature bytes (not base64 encoded)
///
/// # Errors
///
/// Returns an error if verification fails.
pub fn verify_manifest_bytes(
    manifest: &crate::types::AgentManifest,
    signature: &[u8],
) -> Result<(), crate::error::CryptoError> {
    use crate::error::CryptoError;
    use ed25519_dalek::{Signature as DalekSig, Verifier, VerifyingKey};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    // Get the canonical bytes
    let bytes = manifest
        .to_canonical_bytes()
        .map_err(|e| CryptoError::VerificationFailed(format!("serialization failed: {e}")))?;

    // Decode the public key from the manifest
    let pk_bytes = URL_SAFE_NO_PAD
        .decode(&manifest.public_key)
        .map_err(|e| CryptoError::InvalidKeyFormat(format!("invalid public key encoding: {e}")))?;

    if pk_bytes.len() != 32 {
        return Err(CryptoError::InvalidKeyFormat(format!(
            "public key must be 32 bytes, got {}",
            pk_bytes.len()
        )));
    }

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&pk_bytes);

    // Parse the verifying key
    let vk = VerifyingKey::from_bytes(&pk_array)
        .map_err(|e| CryptoError::InvalidKeyFormat(e.to_string()))?;

    // Parse the signature
    if signature.len() != 64 {
        return Err(CryptoError::VerificationFailed(format!(
            "signature must be 64 bytes, got {}",
            signature.len()
        )));
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(signature);
    let sig = DalekSig::from_bytes(&sig_array);

    // Verify
    vk.verify(&bytes, &sig)
        .map_err(|_| CryptoError::VerificationFailed("signature verification failed".to_string()))
}

/// Generates a cryptographically secure random nonce (32 bytes).
#[must_use]
pub fn generate_nonce() -> [u8; 32] {
    use rand::RngCore;
    let mut nonce = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

/// Computes the hash chain for an audit event.
///
/// The hash chain ensures audit log integrity by including the previous
/// event's hash in each new event.
///
/// # Arguments
///
/// * `previous_hash` - The hash of the previous event (or zeros for the first event)
/// * `event_content` - The canonical bytes of the current event
///
/// # Returns
///
/// A 32-byte SHA-256 hash of the concatenated inputs.
#[must_use]
pub fn hash_chain_event(previous_hash: &[u8; 32], event_content: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(previous_hash);
    hasher.update(event_content);
    hasher.finalize().into()
}

/// Computes a SHA-256 hash of the input bytes.
#[must_use]
pub fn sha256(input: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize().into()
}

/// Performs a constant-time comparison of two byte slices.
///
/// This must be used for all security-sensitive comparisons to prevent
/// timing attacks.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_nonce_uniqueness() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();
        assert_ne!(nonce1, nonce2);
    }

    #[test]
    fn test_generate_nonce_length() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), 32);
    }

    #[test]
    fn test_hash_chain_deterministic() {
        let prev = [0u8; 32];
        let content = b"test event content";

        let hash1 = hash_chain_event(&prev, content);
        let hash2 = hash_chain_event(&prev, content);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_chain_changes_with_previous() {
        let prev1 = [0u8; 32];
        let prev2 = [1u8; 32];
        let content = b"test event content";

        let hash1 = hash_chain_event(&prev1, content);
        let hash2 = hash_chain_event(&prev2, content);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_chain_changes_with_content() {
        let prev = [0u8; 32];
        let content1 = b"test event content 1";
        let content2 = b"test event content 2";

        let hash1 = hash_chain_event(&prev, content1);
        let hash2 = hash_chain_event(&prev, content2);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_constant_time_eq_equal() {
        let a = [1u8, 2, 3, 4];
        let b = [1u8, 2, 3, 4];
        assert!(constant_time_eq(&a, &b));
    }

    #[test]
    fn test_constant_time_eq_not_equal() {
        let a = [1u8, 2, 3, 4];
        let b = [1u8, 2, 3, 5];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        let a = [1u8, 2, 3, 4];
        let b = [1u8, 2, 3];
        assert!(!constant_time_eq(&a, &b));
    }
}
