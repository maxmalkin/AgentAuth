//! Token security compliance tests.
//!
//! Verifies that tampered tokens are always rejected.

use auth_core::crypto::{constant_time_eq, sha256};
use auth_core::types::{
    AgentAccessToken, AgentId, BehavioralEnvelope, Capability, HumanPrincipalId,
    ServiceProviderId, TokenId,
};
use chrono::Utc;

fn create_valid_token() -> AgentAccessToken {
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
        key_id: "test-key-1".to_string(),
    }
}

/// COMPLIANCE: Token canonical bytes change when claims are modified.
#[test]
fn test_token_canonical_bytes_change_with_claims() {
    let token1 = create_valid_token();
    let mut token2 = token1.clone();
    token2.granted_capabilities = vec![Capability::Write {
        resource: "calendar".to_string(),
        conditions: None,
    }];

    let bytes1 = token1.to_canonical_bytes().expect("serialize token1");
    let bytes2 = token2.to_canonical_bytes().expect("serialize token2");

    assert_ne!(
        bytes1, bytes2,
        "different capabilities MUST produce different canonical bytes"
    );
}

/// COMPLIANCE: Token canonical bytes change when service_provider_id changes.
#[test]
fn test_token_canonical_bytes_change_with_service_provider() {
    let token1 = create_valid_token();
    let mut token2 = token1.clone();
    token2.service_provider_id = ServiceProviderId::new();

    let bytes1 = token1.to_canonical_bytes().expect("serialize token1");
    let bytes2 = token2.to_canonical_bytes().expect("serialize token2");

    assert_ne!(
        bytes1, bytes2,
        "different service_provider_id MUST produce different canonical bytes"
    );
}

/// COMPLIANCE: Token canonical bytes change when agent_id changes.
#[test]
fn test_token_canonical_bytes_change_with_agent_id() {
    let token1 = create_valid_token();
    let mut token2 = token1.clone();
    token2.agent_id = AgentId::new();

    let bytes1 = token1.to_canonical_bytes().expect("serialize token1");
    let bytes2 = token2.to_canonical_bytes().expect("serialize token2");

    assert_ne!(
        bytes1, bytes2,
        "different agent_id MUST produce different canonical bytes"
    );
}

/// COMPLIANCE: Token canonical bytes change when expiry changes.
#[test]
fn test_token_canonical_bytes_change_with_expiry() {
    let token1 = create_valid_token();
    let mut token2 = token1.clone();
    token2.expires_at = Utc::now() + chrono::Duration::hours(24);

    let bytes1 = token1.to_canonical_bytes().expect("serialize token1");
    let bytes2 = token2.to_canonical_bytes().expect("serialize token2");

    assert_ne!(
        bytes1, bytes2,
        "different expires_at MUST produce different canonical bytes"
    );
}

/// COMPLIANCE: Token canonical bytes change when key_id changes.
#[test]
fn test_token_canonical_bytes_change_with_key_id() {
    let token1 = create_valid_token();
    let mut token2 = token1.clone();
    token2.key_id = "different-key".to_string();

    let bytes1 = token1.to_canonical_bytes().expect("serialize token1");
    let bytes2 = token2.to_canonical_bytes().expect("serialize token2");

    assert_ne!(
        bytes1, bytes2,
        "different key_id MUST produce different canonical bytes"
    );
}

/// COMPLIANCE: Token canonical bytes are deterministic.
#[test]
fn test_token_canonical_bytes_deterministic() {
    let token = create_valid_token();

    let bytes1 = token.to_canonical_bytes().expect("serialize 1");
    let bytes2 = token.to_canonical_bytes().expect("serialize 2");

    assert_eq!(
        bytes1, bytes2,
        "canonical bytes MUST be deterministic"
    );
}

/// COMPLIANCE: Token hash changes when any claim is modified.
#[test]
fn test_token_hash_changes_with_any_modification() {
    let token1 = create_valid_token();
    let bytes1 = token1.to_canonical_bytes().expect("serialize");
    let hash1 = sha256(&bytes1);

    // Test agent_id change
    let mut token2 = token1.clone();
    token2.agent_id = AgentId::new();
    let bytes2 = token2.to_canonical_bytes().expect("serialize");
    let hash2 = sha256(&bytes2);
    assert_ne!(hash1, hash2, "changed agent_id MUST produce different hash");

    // Test capability change
    let mut token3 = token1.clone();
    token3.granted_capabilities = vec![];
    let bytes3 = token3.to_canonical_bytes().expect("serialize");
    let hash3 = sha256(&bytes3);
    assert_ne!(hash1, hash3, "changed capabilities MUST produce different hash");

    // Test service_provider_id change
    let mut token4 = token1.clone();
    token4.service_provider_id = ServiceProviderId::new();
    let bytes4 = token4.to_canonical_bytes().expect("serialize");
    let hash4 = sha256(&bytes4);
    assert_ne!(hash1, hash4, "changed service_provider_id MUST produce different hash");
}

/// COMPLIANCE: Constant-time comparison is used for hashes.
#[test]
fn test_constant_time_hash_comparison() {
    let token = create_valid_token();
    let bytes = token.to_canonical_bytes().expect("serialize");
    let hash = sha256(&bytes);

    // Same hash should compare equal
    assert!(
        constant_time_eq(&hash, &hash),
        "identical hashes MUST compare equal"
    );

    // Different hash should compare not equal
    let mut different_hash = hash;
    different_hash[0] ^= 0xFF;
    assert!(
        !constant_time_eq(&hash, &different_hash),
        "different hashes MUST compare not equal"
    );
}

/// COMPLIANCE: Token validation rejects expired tokens.
#[test]
fn test_expired_token_rejected() {
    let now = Utc::now();
    let mut token = create_valid_token();
    token.issued_at = now - chrono::Duration::hours(1);
    token.expires_at = now - chrono::Duration::minutes(1);

    assert!(
        token.validate().is_err(),
        "expired token MUST be rejected"
    );
}

/// COMPLIANCE: Token validation rejects tokens with empty capabilities.
#[test]
fn test_empty_capabilities_rejected() {
    let mut token = create_valid_token();
    token.granted_capabilities = vec![];

    assert!(
        token.validate().is_err(),
        "token with empty capabilities MUST be rejected"
    );
}

/// COMPLIANCE: Token validation accepts valid tokens.
#[test]
fn test_valid_token_accepted() {
    let token = create_valid_token();

    assert!(
        token.validate().is_ok(),
        "valid token MUST be accepted"
    );
}

/// COMPLIANCE: Token JTI (jti) MUST be unique.
#[test]
fn test_token_jti_unique() {
    let token1 = create_valid_token();
    let token2 = create_valid_token();

    assert_ne!(
        token1.jti, token2.jti,
        "each token MUST have a unique JTI"
    );
}
