//! Capability boundary compliance tests.
//!
//! Verifies that agents cannot request or use capabilities beyond their manifest.

use agentauth_core::types::{
    AgentAccessToken, AgentId, AgentManifest, BehavioralEnvelope, Capability,
    HumanPrincipalId, ServiceProviderId, TokenId,
};
use chrono::Utc;
use std::collections::HashMap;

fn create_test_manifest(capabilities: Vec<Capability>) -> AgentManifest {
    let now = Utc::now();
    AgentManifest {
        id: AgentId::new(),
        public_key: "test-public-key".to_string(),
        key_id: "key-1".to_string(),
        capabilities_requested: capabilities,
        human_principal_id: HumanPrincipalId::new(),
        issued_at: now,
        expires_at: now + chrono::Duration::days(365),
        name: "Test Agent".to_string(),
        description: Some("A test agent".to_string()),
        model_origin: Some("test.example.com".to_string()),
    }
}

fn create_test_token(capabilities: Vec<Capability>) -> AgentAccessToken {
    let now = Utc::now();
    AgentAccessToken {
        jti: TokenId::new(),
        agent_id: AgentId::new(),
        human_principal_id: HumanPrincipalId::new(),
        service_provider_id: ServiceProviderId::new(),
        granted_capabilities: capabilities,
        behavioral_envelope: BehavioralEnvelope::default_restrictive(),
        issued_at: now,
        expires_at: now + chrono::Duration::minutes(15),
        confirmation: None,
        key_id: "key-1".to_string(),
    }
}

/// Checks if requested capabilities are a subset of manifest capabilities.
fn capabilities_within_manifest(
    requested: &[Capability],
    manifest: &AgentManifest,
) -> bool {
    requested.iter().all(|req| {
        manifest.capabilities_requested.iter().any(|m| {
            req.capability_type() == m.capability_type() && req.resource() == m.resource()
        })
    })
}

/// COMPLIANCE: Agent cannot request capabilities beyond manifest.
#[test]
fn test_cannot_request_beyond_manifest() {
    let manifest = create_test_manifest(vec![
        Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        },
    ]);

    // Try to request write capability (not in manifest)
    let requested = vec![
        Capability::Write {
            resource: "calendar".to_string(),
            conditions: None,
        },
    ];

    assert!(
        !capabilities_within_manifest(&requested, &manifest),
        "agent MUST NOT be able to request capabilities beyond manifest"
    );
}

/// COMPLIANCE: Agent can request capabilities within manifest.
#[test]
fn test_can_request_within_manifest() {
    let manifest = create_test_manifest(vec![
        Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        },
        Capability::Write {
            resource: "calendar".to_string(),
            conditions: None,
        },
    ]);

    // Request only read (subset of manifest)
    let requested = vec![
        Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        },
    ];

    assert!(
        capabilities_within_manifest(&requested, &manifest),
        "agent MUST be able to request capabilities within manifest"
    );
}

/// COMPLIANCE: Agent cannot request capabilities for different resource.
#[test]
fn test_cannot_request_different_resource() {
    let manifest = create_test_manifest(vec![
        Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        },
    ]);

    // Try to request same capability type but different resource
    let requested = vec![
        Capability::Read {
            resource: "email".to_string(),
            filter: None,
        },
    ];

    assert!(
        !capabilities_within_manifest(&requested, &manifest),
        "agent MUST NOT be able to request capabilities for resources not in manifest"
    );
}

/// COMPLIANCE: Token with no capabilities MUST be rejected.
#[test]
fn test_empty_capabilities_rejected() {
    let token = create_test_token(vec![]);

    assert!(
        token.validate().is_err(),
        "token with empty capabilities MUST be rejected"
    );
}

/// COMPLIANCE: has_capability correctly checks token capabilities.
#[test]
fn test_has_capability_check() {
    let token = create_test_token(vec![
        Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        },
        Capability::Write {
            resource: "files".to_string(),
            conditions: None,
        },
    ]);

    // Should have read on calendar
    assert!(
        token.has_capability("read", "calendar"),
        "token MUST report having granted capabilities"
    );

    // Should have write on files
    assert!(
        token.has_capability("write", "files"),
        "token MUST report having granted capabilities"
    );

    // Should NOT have write on calendar
    assert!(
        !token.has_capability("write", "calendar"),
        "token MUST NOT report having non-granted capabilities"
    );

    // Should NOT have read on email
    assert!(
        !token.has_capability("read", "email"),
        "token MUST NOT report having non-granted capabilities"
    );
}

/// COMPLIANCE: Capability validation rejects invalid capabilities.
#[test]
fn test_invalid_capability_rejected() {
    // Empty resource should be rejected
    let cap = Capability::Read {
        resource: "".to_string(),
        filter: None,
    };

    assert!(
        cap.validate().is_err(),
        "capability with empty resource MUST be rejected"
    );
}

/// COMPLIANCE: Transact capability requires max_value.
#[test]
fn test_transact_requires_max_value() {
    // Transact with max_value should be valid
    let valid = Capability::Transact {
        resource: "payments".to_string(),
        max_value: 1000,
        currency: None,
    };
    assert!(
        valid.validate().is_ok(),
        "transact with max_value MUST be valid"
    );

    // Transact with zero max_value should be rejected
    let invalid = Capability::Transact {
        resource: "payments".to_string(),
        max_value: 0,
        currency: None,
    };
    assert!(
        invalid.validate().is_err(),
        "transact with zero max_value MUST be rejected"
    );
}

/// COMPLIANCE: Custom capability requires namespace.
#[test]
fn test_custom_requires_namespace() {
    // Custom with namespace should be valid
    let valid = Capability::Custom {
        namespace: "com.example".to_string(),
        name: "special_action".to_string(),
        params: HashMap::new(),
    };
    assert!(
        valid.validate().is_ok(),
        "custom with namespace MUST be valid"
    );

    // Custom with empty namespace should be rejected
    let invalid = Capability::Custom {
        namespace: "".to_string(),
        name: "special_action".to_string(),
        params: HashMap::new(),
    };
    assert!(
        invalid.validate().is_err(),
        "custom with empty namespace MUST be rejected"
    );
}

/// COMPLIANCE: Capability type extraction is correct.
#[test]
fn test_capability_type_extraction() {
    let read = Capability::Read {
        resource: "test".to_string(),
        filter: None,
    };
    assert_eq!(read.capability_type(), "read");

    let write = Capability::Write {
        resource: "test".to_string(),
        conditions: None,
    };
    assert_eq!(write.capability_type(), "write");

    let transact = Capability::Transact {
        resource: "test".to_string(),
        max_value: 100,
        currency: None,
    };
    assert_eq!(transact.capability_type(), "transact");

    let custom = Capability::Custom {
        namespace: "com.example".to_string(),
        name: "test".to_string(),
        params: HashMap::new(),
    };
    assert_eq!(custom.capability_type(), "custom");
}

/// COMPLIANCE: Capability resource extraction is correct.
#[test]
fn test_capability_resource_extraction() {
    let read = Capability::Read {
        resource: "calendar".to_string(),
        filter: None,
    };
    assert_eq!(read.resource(), "calendar");

    let write = Capability::Write {
        resource: "files".to_string(),
        conditions: None,
    };
    assert_eq!(write.resource(), "files");

    let transact = Capability::Transact {
        resource: "payments".to_string(),
        max_value: 100,
        currency: None,
    };
    assert_eq!(transact.resource(), "payments");
}
