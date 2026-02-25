//! DPoP (Demonstrating Proof of Possession) proof generation.
//!
//! DPoP is used to bind tokens to the agent's private key, preventing
//! token theft. Every authenticated request must include a DPoP proof
//! signed over the request method and URL.
//!
//! # Security
//!
//! - Each proof includes a unique JTI (JWT ID) to prevent replay attacks
//! - Proofs are signed using the agent's Ed25519 private key
//! - The registry/verifier validates the proof signature matches the token's `cnf` claim

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::Utc;
use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{SdkError, SdkResult};

/// A DPoP proof to demonstrate possession of the agent's private key.
#[derive(Debug, Clone)]
pub struct DpopProof {
    /// The serialized proof (base64url-encoded header.payload.signature).
    proof: String,
}

impl DpopProof {
    /// Returns the proof as a string for use in HTTP headers.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.proof
    }
}

impl std::fmt::Display for DpopProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.proof)
    }
}

/// DPoP header (JWT header).
#[derive(Debug, Serialize, Deserialize)]
struct DpopHeader {
    /// Algorithm (always "EdDSA" for Ed25519).
    alg: String,
    /// Type (always "dpop+jwt").
    typ: String,
    /// JSON Web Key (public key).
    jwk: Jwk,
}

/// JSON Web Key representation of an Ed25519 public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Jwk {
    /// Key type (always "OKP" for Ed25519).
    kty: String,
    /// Curve (always "Ed25519").
    crv: String,
    /// X coordinate (base64url-encoded public key bytes).
    x: String,
}

/// DPoP payload (JWT claims).
#[derive(Debug, Serialize, Deserialize)]
struct DpopPayload {
    /// Unique identifier for this proof.
    jti: String,
    /// HTTP method (e.g., "POST").
    htm: String,
    /// HTTP URI (full URL without query string).
    htu: String,
    /// Issued at timestamp (seconds since epoch).
    iat: i64,
    /// Optional access token hash (for token-bound proofs).
    #[serde(skip_serializing_if = "Option::is_none")]
    ath: Option<String>,
}

/// Generates DPoP proofs for authenticated requests.
pub struct DpopGenerator {
    /// The agent's Ed25519 signing key.
    signing_key: ed25519_dalek::SigningKey,
    /// The agent's public key in JWK format.
    jwk: Jwk,
}

impl DpopGenerator {
    /// Creates a new DPoP generator from Ed25519 key bytes.
    ///
    /// # Arguments
    ///
    /// * `private_key_bytes` - 32-byte Ed25519 private key seed
    ///
    /// # Errors
    ///
    /// Returns an error if the key bytes are invalid.
    pub fn new(private_key_bytes: &[u8; 32]) -> SdkResult<Self> {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(private_key_bytes);
        let verifying_key = signing_key.verifying_key();
        let public_key_bytes = verifying_key.to_bytes();

        let jwk = Jwk {
            kty: "OKP".to_string(),
            crv: "Ed25519".to_string(),
            x: URL_SAFE_NO_PAD.encode(public_key_bytes),
        };

        Ok(Self { signing_key, jwk })
    }

    /// Creates a new DPoP generator from a signing key.
    #[must_use]
    pub fn from_signing_key(signing_key: ed25519_dalek::SigningKey) -> Self {
        let verifying_key = signing_key.verifying_key();
        let public_key_bytes = verifying_key.to_bytes();

        let jwk = Jwk {
            kty: "OKP".to_string(),
            crv: "Ed25519".to_string(),
            x: URL_SAFE_NO_PAD.encode(public_key_bytes),
        };

        Self { signing_key, jwk }
    }

    /// Generates a DPoP proof for a request.
    ///
    /// # Arguments
    ///
    /// * `method` - HTTP method (e.g., "POST")
    /// * `url` - Request URL (path portion will be extracted)
    /// * `access_token` - Optional access token to bind to the proof
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or signing fails.
    pub fn generate(
        &self,
        method: &str,
        url: &str,
        access_token: Option<&str>,
    ) -> SdkResult<DpopProof> {
        // Generate unique JTI
        let jti = uuid::Uuid::now_v7().to_string();

        // Get current timestamp
        let iat = Utc::now().timestamp();

        // Compute access token hash if provided
        let ath = access_token.map(|token| {
            let hash = Sha256::digest(token.as_bytes());
            URL_SAFE_NO_PAD.encode(hash)
        });

        // Extract the URL without query string for htu
        let htu = extract_url_base(url)?;

        // Build header
        let header = DpopHeader {
            alg: "EdDSA".to_string(),
            typ: "dpop+jwt".to_string(),
            jwk: self.jwk.clone(),
        };

        // Build payload
        let payload = DpopPayload {
            jti,
            htm: method.to_uppercase(),
            htu,
            iat,
            ath,
        };

        // Serialize header and payload
        let header_json = serde_json::to_vec(&header)?;
        let payload_json = serde_json::to_vec(&payload)?;

        let header_b64 = URL_SAFE_NO_PAD.encode(&header_json);
        let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_json);

        // Create signing input
        let signing_input = format!("{header_b64}.{payload_b64}");

        // Sign
        let signature = self.signing_key.sign(signing_input.as_bytes());
        let signature_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        // Combine into JWT
        let proof = format!("{signing_input}.{signature_b64}");

        Ok(DpopProof { proof })
    }

    /// Returns the JWK thumbprint (used for `cnf` claim binding).
    ///
    /// The thumbprint is a SHA-256 hash of the canonical JWK.
    #[must_use]
    pub fn thumbprint(&self) -> String {
        // Canonical JWK format: {"crv":"Ed25519","kty":"OKP","x":"..."}
        // Keys must be in alphabetical order per RFC 7638
        let canonical = format!(
            r#"{{"crv":"{}","kty":"{}","x":"{}"}}"#,
            self.jwk.crv, self.jwk.kty, self.jwk.x
        );

        let hash = Sha256::digest(canonical.as_bytes());
        URL_SAFE_NO_PAD.encode(hash)
    }

    /// Returns the public key as base64url-encoded bytes.
    #[must_use]
    pub fn public_key_base64(&self) -> String {
        self.jwk.x.clone()
    }
}

/// Extracts the base URL (scheme + host + path) without query string.
fn extract_url_base(url: &str) -> SdkResult<String> {
    let parsed =
        url::Url::parse(url).map_err(|e| SdkError::ConfigError(format!("Invalid URL: {e}")))?;

    let mut result = format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or(""));

    if let Some(port) = parsed.port() {
        use std::fmt::Write;
        let _ = write!(result, ":{port}");
    }

    result.push_str(parsed.path());

    Ok(result)
}

/// Verifies a DPoP proof.
///
/// This is primarily for testing; the registry/verifier performs actual verification.
#[cfg(test)]
pub fn verify_proof(proof: &str, method: &str, url: &str, max_age_secs: i64) -> Result<(), String> {
    // Split the proof
    let parts: Vec<&str> = proof.split('.').collect();
    if parts.len() != 3 {
        return Err("Invalid proof format".to_string());
    }

    // Decode header
    let header_bytes = URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|e| format!("Invalid header encoding: {e}"))?;
    let header: DpopHeader =
        serde_json::from_slice(&header_bytes).map_err(|e| format!("Invalid header: {e}"))?;

    // Decode payload
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| format!("Invalid payload encoding: {e}"))?;
    let payload: DpopPayload =
        serde_json::from_slice(&payload_bytes).map_err(|e| format!("Invalid payload: {e}"))?;

    // Decode signature
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(parts[2])
        .map_err(|e| format!("Invalid signature encoding: {e}"))?;

    // Verify algorithm
    if header.alg != "EdDSA" {
        return Err(format!("Unsupported algorithm: {}", header.alg));
    }

    // Verify type
    if header.typ != "dpop+jwt" {
        return Err(format!("Invalid type: {}", header.typ));
    }

    // Verify method
    if payload.htm.to_uppercase() != method.to_uppercase() {
        return Err(format!(
            "Method mismatch: expected {}, got {}",
            method, payload.htm
        ));
    }

    // Verify URL
    let expected_htu = extract_url_base(url).map_err(|e| e.to_string())?;
    if payload.htu != expected_htu {
        return Err(format!(
            "URL mismatch: expected {}, got {}",
            expected_htu, payload.htu
        ));
    }

    // Verify timestamp
    let now = Utc::now().timestamp();
    if payload.iat > now + 60 {
        return Err("Proof issued in the future".to_string());
    }
    if payload.iat < now - max_age_secs {
        return Err("Proof expired".to_string());
    }

    // Extract public key from JWK
    let public_key_bytes = URL_SAFE_NO_PAD
        .decode(&header.jwk.x)
        .map_err(|e| format!("Invalid public key encoding: {e}"))?;

    if public_key_bytes.len() != 32 {
        return Err(format!(
            "Invalid public key length: {}",
            public_key_bytes.len()
        ));
    }

    let mut pk_array = [0u8; 32];
    pk_array.copy_from_slice(&public_key_bytes);

    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pk_array)
        .map_err(|e| format!("Invalid public key: {e}"))?;

    // Verify signature
    let signing_input = format!("{}.{}", parts[0], parts[1]);

    if signature_bytes.len() != 64 {
        return Err(format!(
            "Invalid signature length: {}",
            signature_bytes.len()
        ));
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);
    let signature = ed25519_dalek::Signature::from_bytes(&sig_array);

    use ed25519_dalek::Verifier;
    verifying_key
        .verify(signing_input.as_bytes(), &signature)
        .map_err(|_| "Signature verification failed".to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        key[0] = 1; // Just needs to be valid seed
        key
    }

    #[test]
    fn test_generate_and_verify_proof() {
        let generator = DpopGenerator::new(&test_key()).expect("create generator");

        let proof = generator
            .generate("POST", "https://example.com/v1/tokens/verify", None)
            .expect("generate proof");

        let result = verify_proof(
            proof.as_str(),
            "POST",
            "https://example.com/v1/tokens/verify",
            300,
        );
        assert!(result.is_ok(), "Verification failed: {:?}", result);
    }

    #[test]
    fn test_proof_with_access_token() {
        let generator = DpopGenerator::new(&test_key()).expect("create generator");

        let proof = generator
            .generate("POST", "https://example.com/api", Some("test-access-token"))
            .expect("generate proof");

        // The proof should contain an ath claim
        let parts: Vec<&str> = proof.as_str().split('.').collect();
        let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).expect("decode");
        let payload: DpopPayload = serde_json::from_slice(&payload_bytes).expect("parse");

        assert!(payload.ath.is_some());
    }

    #[test]
    fn test_wrong_method_fails() {
        let generator = DpopGenerator::new(&test_key()).expect("create generator");

        let proof = generator
            .generate("POST", "https://example.com/api", None)
            .expect("generate proof");

        let result = verify_proof(proof.as_str(), "GET", "https://example.com/api", 300);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Method mismatch"));
    }

    #[test]
    fn test_wrong_url_fails() {
        let generator = DpopGenerator::new(&test_key()).expect("create generator");

        let proof = generator
            .generate("POST", "https://example.com/api", None)
            .expect("generate proof");

        let result = verify_proof(proof.as_str(), "POST", "https://other.com/api", 300);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("URL mismatch"));
    }

    #[test]
    fn test_thumbprint_deterministic() {
        let generator = DpopGenerator::new(&test_key()).expect("create generator");

        let thumbprint1 = generator.thumbprint();
        let thumbprint2 = generator.thumbprint();

        assert_eq!(thumbprint1, thumbprint2);
    }

    #[test]
    fn test_url_extraction_strips_query() {
        let url = "https://example.com:8080/api/path?query=value&other=123";
        let result = extract_url_base(url).expect("extract");
        assert_eq!(result, "https://example.com:8080/api/path");
    }

    #[test]
    fn test_url_extraction_preserves_port() {
        let url = "https://example.com:8443/api";
        let result = extract_url_base(url).expect("extract");
        assert_eq!(result, "https://example.com:8443/api");
    }

    #[test]
    fn test_unique_jti_per_proof() {
        let generator = DpopGenerator::new(&test_key()).expect("create generator");

        let proof1 = generator
            .generate("POST", "https://example.com/api", None)
            .expect("proof1");
        let proof2 = generator
            .generate("POST", "https://example.com/api", None)
            .expect("proof2");

        // Proofs should be different (different JTI)
        assert_ne!(proof1.as_str(), proof2.as_str());
    }
}
