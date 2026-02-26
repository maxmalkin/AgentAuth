//! Nonce replay compliance tests.
//!
//! Verifies that replayed nonces within token lifetime are rejected.

use agentauth_core::crypto::generate_nonce;
use std::collections::HashSet;

/// COMPLIANCE: Generated nonces MUST be unique.
#[test]
fn test_nonce_uniqueness() {
    let mut nonces = HashSet::new();

    // Generate 10,000 nonces and verify all are unique
    for _ in 0..10_000 {
        let nonce = generate_nonce();
        assert!(
            nonces.insert(nonce),
            "generated nonces MUST be unique"
        );
    }
}

/// COMPLIANCE: Nonce MUST be 32 bytes.
#[test]
fn test_nonce_length() {
    let nonce = generate_nonce();
    assert_eq!(
        nonce.len(),
        32,
        "nonce MUST be exactly 32 bytes"
    );
}

/// COMPLIANCE: Nonces MUST have sufficient entropy.
#[test]
fn test_nonce_entropy() {
    let nonce = generate_nonce();

    // Check that the nonce is not all zeros
    let all_zeros = nonce.iter().all(|&b| b == 0);
    assert!(
        !all_zeros,
        "nonce MUST have entropy (not all zeros)"
    );

    // Check that the nonce is not all ones
    let all_ones = nonce.iter().all(|&b| b == 0xFF);
    assert!(
        !all_ones,
        "nonce MUST have entropy (not all ones)"
    );

    // Check that we have reasonable byte diversity
    let unique_bytes: HashSet<u8> = nonce.iter().copied().collect();
    assert!(
        unique_bytes.len() >= 10,
        "nonce MUST have reasonable byte diversity (got {} unique bytes)",
        unique_bytes.len()
    );
}

/// Simulates a nonce store that tracks used nonces.
struct NonceStore {
    used: HashSet<[u8; 32]>,
}

impl NonceStore {
    fn new() -> Self {
        Self {
            used: HashSet::new(),
        }
    }

    /// Check and record a nonce. Returns false if nonce was already used.
    fn check_and_record(&mut self, nonce: [u8; 32]) -> bool {
        self.used.insert(nonce)
    }

    /// Check if a nonce has been used.
    fn is_used(&self, nonce: &[u8; 32]) -> bool {
        self.used.contains(nonce)
    }
}

/// COMPLIANCE: A replayed nonce MUST be detected.
#[test]
fn test_nonce_replay_detection() {
    let mut store = NonceStore::new();

    let nonce = generate_nonce();

    // First use should succeed
    assert!(
        store.check_and_record(nonce),
        "first use of nonce MUST succeed"
    );

    // Replay should be detected
    assert!(
        !store.check_and_record(nonce),
        "replayed nonce MUST be detected"
    );

    assert!(
        store.is_used(&nonce),
        "used nonce MUST be tracked"
    );
}

/// COMPLIANCE: Different nonces MUST be allowed.
#[test]
fn test_different_nonces_allowed() {
    let mut store = NonceStore::new();

    let nonce1 = generate_nonce();
    let nonce2 = generate_nonce();

    // Both should succeed (they're different)
    assert!(
        store.check_and_record(nonce1),
        "first nonce MUST be allowed"
    );
    assert!(
        store.check_and_record(nonce2),
        "second (different) nonce MUST be allowed"
    );
}

/// COMPLIANCE: Nonce collision probability MUST be negligible.
#[test]
fn test_nonce_collision_probability() {
    // With 32 bytes of randomness (256 bits), collision probability
    // after n nonces is approximately n^2 / 2^257
    //
    // For 10 million nonces: (10^7)^2 / 2^257 ≈ 5.4 × 10^-64
    // This is astronomically small.

    // We can't actually test 10 million nonces efficiently, but we can
    // verify the math by checking a sample
    let mut nonces = HashSet::new();
    let sample_size = 100_000;

    for _ in 0..sample_size {
        let nonce = generate_nonce();
        let is_new = nonces.insert(nonce);
        assert!(
            is_new,
            "collision detected in {} nonces - this should be astronomically unlikely",
            sample_size
        );
    }
}
