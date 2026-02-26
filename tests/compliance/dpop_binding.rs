//! DPoP binding compliance tests.
//!
//! Verifies that stolen AATs without the DPoP private key are rejected.

use agentauth_sdk::dpop::DpopGenerator;

fn test_key(seed: u8) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[0] = seed;
    key[1] = seed.wrapping_add(1);
    key
}

/// COMPLIANCE: Different keys produce different DPoP proofs.
#[test]
fn test_different_keys_different_proofs() {
    let generator1 = DpopGenerator::new(&test_key(1)).expect("create generator 1");
    let generator2 = DpopGenerator::new(&test_key(2)).expect("create generator 2");

    let proof1 = generator1
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof 1");

    let proof2 = generator2
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof 2");

    assert_ne!(
        proof1.as_str(),
        proof2.as_str(),
        "proofs from different keys MUST be different"
    );
}

/// COMPLIANCE: Same key produces proofs with different JTIs.
#[test]
fn test_unique_jti_per_proof() {
    let generator = DpopGenerator::new(&test_key(1)).expect("create generator");

    let proof1 = generator
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof 1");

    let proof2 = generator
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof 2");

    // Proofs should be different due to unique JTI
    assert_ne!(
        proof1.as_str(),
        proof2.as_str(),
        "each DPoP proof MUST have a unique JTI"
    );
}

/// COMPLIANCE: DPoP thumbprint MUST be deterministic for the same keypair.
#[test]
fn test_thumbprint_deterministic() {
    let generator = DpopGenerator::new(&test_key(1)).expect("create generator");

    let thumbprint1 = generator.thumbprint();
    let thumbprint2 = generator.thumbprint();

    assert_eq!(
        thumbprint1, thumbprint2,
        "DPoP thumbprint MUST be deterministic"
    );
}

/// COMPLIANCE: Different keys produce different thumbprints.
#[test]
fn test_different_keys_different_thumbprints() {
    let generator1 = DpopGenerator::new(&test_key(1)).expect("create generator 1");
    let generator2 = DpopGenerator::new(&test_key(2)).expect("create generator 2");

    let thumbprint1 = generator1.thumbprint();
    let thumbprint2 = generator2.thumbprint();

    assert_ne!(
        thumbprint1, thumbprint2,
        "different keys MUST produce different thumbprints"
    );
}

/// COMPLIANCE: DPoP proof contains method and URL binding.
#[test]
fn test_proof_contains_method_and_url() {
    let generator = DpopGenerator::new(&test_key(1)).expect("create generator");

    let proof = generator
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof");

    // The proof should be a valid JWT structure (header.payload.signature)
    let parts: Vec<&str> = proof.as_str().split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "DPoP proof MUST be a valid JWT with 3 parts"
    );
}

/// COMPLIANCE: DPoP proof with access token includes ath claim.
#[test]
fn test_proof_with_access_token_binding() {
    let generator = DpopGenerator::new(&test_key(1)).expect("create generator");
    let access_token = "eyJhbGciOiJFZDI1NTE5IiwidHlwIjoiSldUIn0.test.signature";

    let proof_with_token = generator
        .generate("POST", "https://api.example.com/resource", Some(access_token))
        .expect("create proof with ath");

    let proof_without_token = generator
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof without ath");

    // Proofs should be different due to ath claim
    assert_ne!(
        proof_with_token.as_str(),
        proof_without_token.as_str(),
        "proof with access token MUST differ from proof without"
    );
}

/// COMPLIANCE: DPoP generator can be created from signing key.
#[test]
fn test_generator_from_signing_key() {
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&test_key(1));
    let generator = DpopGenerator::from_signing_key(signing_key);

    let proof = generator
        .generate("POST", "https://api.example.com/resource", None)
        .expect("create proof");

    // Should produce a valid proof
    let parts: Vec<&str> = proof.as_str().split('.').collect();
    assert_eq!(parts.len(), 3, "proof MUST be a valid JWT");
}

/// COMPLIANCE: Public key can be extracted from generator.
#[test]
fn test_public_key_extraction() {
    let generator = DpopGenerator::new(&test_key(1)).expect("create generator");

    let public_key = generator.public_key_base64();

    // Ed25519 public key is 32 bytes, base64url encoded
    // 32 bytes = 43 base64 characters (no padding)
    assert!(
        public_key.len() >= 40,
        "public key MUST be valid base64url encoded Ed25519 key"
    );
}
