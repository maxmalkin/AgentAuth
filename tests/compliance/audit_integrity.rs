//! Audit log integrity compliance tests.
//!
//! Verifies that the audit log hash chain maintains integrity
//! and that service providers cannot forge events for others.

use auth_core::crypto::{constant_time_eq, hash_chain_event, sha256};
use auth_core::types::{AgentId, ServiceProviderId};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Simulated audit event for testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuditEvent {
    event_id: Uuid,
    service_provider_id: ServiceProviderId,
    agent_id: AgentId,
    action: String,
    timestamp: chrono::DateTime<Utc>,
    previous_hash: [u8; 32],
    row_hash: [u8; 32],
}

impl AuditEvent {
    fn new(
        service_provider_id: ServiceProviderId,
        agent_id: AgentId,
        action: &str,
        previous_hash: [u8; 32],
    ) -> Self {
        let event_id = Uuid::now_v7();
        let timestamp = Utc::now();

        let content = Self::content_bytes(&event_id, &service_provider_id, &agent_id, action, &timestamp);
        let row_hash = hash_chain_event(&previous_hash, &content);

        Self {
            event_id,
            service_provider_id,
            agent_id,
            action: action.to_string(),
            timestamp,
            previous_hash,
            row_hash,
        }
    }

    fn content_bytes(
        event_id: &Uuid,
        service_provider_id: &ServiceProviderId,
        agent_id: &AgentId,
        action: &str,
        timestamp: &chrono::DateTime<Utc>,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(event_id.as_bytes());
        bytes.extend_from_slice(service_provider_id.0.as_bytes());
        bytes.extend_from_slice(agent_id.0.as_bytes());
        bytes.extend_from_slice(action.as_bytes());
        bytes.extend_from_slice(&timestamp.timestamp().to_le_bytes());
        bytes
    }

    fn verify_hash(&self) -> bool {
        let content = Self::content_bytes(
            &self.event_id,
            &self.service_provider_id,
            &self.agent_id,
            &self.action,
            &self.timestamp,
        );
        let expected = hash_chain_event(&self.previous_hash, &content);
        constant_time_eq(&self.row_hash, &expected)
    }
}

/// Simulated audit chain for testing.
struct AuditChain {
    events: Vec<AuditEvent>,
}

impl AuditChain {
    fn new() -> Self {
        Self { events: Vec::new() }
    }

    fn append(&mut self, service_provider_id: ServiceProviderId, agent_id: AgentId, action: &str) {
        let previous_hash = self.last_hash();
        let event = AuditEvent::new(service_provider_id, agent_id, action, previous_hash);
        self.events.push(event);
    }

    fn last_hash(&self) -> [u8; 32] {
        self.events.last().map(|e| e.row_hash).unwrap_or([0u8; 32])
    }

    fn verify_chain(&self) -> bool {
        let mut expected_previous = [0u8; 32];

        for event in &self.events {
            // Check previous hash matches what we expect
            if !constant_time_eq(&event.previous_hash, &expected_previous) {
                return false;
            }

            // Verify the row hash
            if !event.verify_hash() {
                return false;
            }

            expected_previous = event.row_hash;
        }

        true
    }
}

/// COMPLIANCE: Audit chain MUST maintain integrity across events.
#[test]
fn test_audit_chain_integrity() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    // Add several events
    chain.append(sp_id.clone(), agent_id.clone(), "token_issued");
    chain.append(sp_id.clone(), agent_id.clone(), "token_verified");
    chain.append(sp_id.clone(), agent_id.clone(), "token_revoked");

    assert!(
        chain.verify_chain(),
        "audit chain with valid events MUST verify successfully"
    );
}

/// COMPLIANCE: Tampered event content MUST be detected.
#[test]
fn test_tampered_event_content_detected() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id.clone(), agent_id.clone(), "token_issued");
    chain.append(sp_id.clone(), agent_id.clone(), "token_verified");

    // Tamper with the action after hash was computed
    chain.events[1].action = "token_revoked".to_string();

    assert!(
        !chain.verify_chain(),
        "tampered event content MUST be detected"
    );
}

/// COMPLIANCE: Tampered previous hash MUST be detected.
#[test]
fn test_tampered_previous_hash_detected() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id.clone(), agent_id.clone(), "token_issued");
    chain.append(sp_id.clone(), agent_id.clone(), "token_verified");

    // Tamper with the previous hash
    chain.events[1].previous_hash = [0xFF; 32];

    assert!(
        !chain.verify_chain(),
        "tampered previous_hash MUST be detected"
    );
}

/// COMPLIANCE: Tampered row hash MUST be detected.
#[test]
fn test_tampered_row_hash_detected() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id.clone(), agent_id.clone(), "token_issued");
    chain.append(sp_id.clone(), agent_id.clone(), "token_verified");

    // Tamper with the row hash
    chain.events[0].row_hash[0] ^= 0xFF;

    assert!(
        !chain.verify_chain(),
        "tampered row_hash MUST be detected"
    );
}

/// COMPLIANCE: Inserted event MUST break the chain.
#[test]
fn test_inserted_event_breaks_chain() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id.clone(), agent_id.clone(), "token_issued");
    chain.append(sp_id.clone(), agent_id.clone(), "token_revoked");

    // Insert an event in the middle with fabricated hashes
    let fake_event = AuditEvent {
        event_id: Uuid::now_v7(),
        service_provider_id: sp_id.clone(),
        agent_id: agent_id.clone(),
        action: "malicious_action".to_string(),
        timestamp: Utc::now(),
        previous_hash: chain.events[0].row_hash,
        row_hash: [0xAB; 32], // Fabricated hash
    };

    chain.events.insert(1, fake_event);

    assert!(
        !chain.verify_chain(),
        "inserted event MUST break the chain"
    );
}

/// COMPLIANCE: Deleted event MUST break the chain.
#[test]
fn test_deleted_event_breaks_chain() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id.clone(), agent_id.clone(), "event_1");
    chain.append(sp_id.clone(), agent_id.clone(), "event_2");
    chain.append(sp_id.clone(), agent_id.clone(), "event_3");

    // Remove the middle event
    chain.events.remove(1);

    assert!(
        !chain.verify_chain(),
        "deleted event MUST break the chain"
    );
}

/// COMPLIANCE: Reordered events MUST break the chain.
#[test]
fn test_reordered_events_break_chain() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id.clone(), agent_id.clone(), "event_1");
    chain.append(sp_id.clone(), agent_id.clone(), "event_2");
    chain.append(sp_id.clone(), agent_id.clone(), "event_3");

    // Swap events
    chain.events.swap(1, 2);

    assert!(
        !chain.verify_chain(),
        "reordered events MUST break the chain"
    );
}

/// COMPLIANCE: Service provider ID is included in event hash.
#[test]
fn test_service_provider_id_in_hash() {
    let sp_id_1 = ServiceProviderId::new();
    let sp_id_2 = ServiceProviderId::new();
    let agent_id = AgentId::new();
    let previous = [0u8; 32];

    let event1 = AuditEvent::new(sp_id_1.clone(), agent_id.clone(), "action", previous);
    let event2 = AuditEvent::new(sp_id_2.clone(), agent_id.clone(), "action", previous);

    // Different service provider IDs must produce different hashes
    assert_ne!(
        event1.row_hash, event2.row_hash,
        "different service_provider_id MUST produce different row_hash"
    );
}

/// COMPLIANCE: Event with wrong service_provider_id has different hash.
#[test]
fn test_forged_service_provider_detected() {
    let legitimate_sp = ServiceProviderId::new();
    let malicious_sp = ServiceProviderId::new();
    let agent_id = AgentId::new();
    let previous = [0u8; 32];

    // Create a legitimate event
    let mut event = AuditEvent::new(legitimate_sp.clone(), agent_id.clone(), "action", previous);
    let original_hash = event.row_hash;

    // Attempt to forge by changing service_provider_id
    event.service_provider_id = malicious_sp;

    // The event no longer verifies
    assert!(
        !event.verify_hash(),
        "event with forged service_provider_id MUST fail verification"
    );

    // And the hash would be different if recomputed
    let content = AuditEvent::content_bytes(
        &event.event_id,
        &event.service_provider_id,
        &event.agent_id,
        &event.action,
        &event.timestamp,
    );
    let forged_hash = hash_chain_event(&previous, &content);
    assert_ne!(
        original_hash, forged_hash,
        "forged event would have different hash"
    );
}

/// COMPLIANCE: Agent ID is included in event hash.
#[test]
fn test_agent_id_in_hash() {
    let sp_id = ServiceProviderId::new();
    let agent_id_1 = AgentId::new();
    let agent_id_2 = AgentId::new();
    let previous = [0u8; 32];

    let event1 = AuditEvent::new(sp_id.clone(), agent_id_1, "action", previous);
    let event2 = AuditEvent::new(sp_id.clone(), agent_id_2, "action", previous);

    assert_ne!(
        event1.row_hash, event2.row_hash,
        "different agent_id MUST produce different row_hash"
    );
}

/// COMPLIANCE: Event with tampered agent_id fails verification.
#[test]
fn test_forged_agent_id_detected() {
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();
    let malicious_agent = AgentId::new();
    let previous = [0u8; 32];

    let mut event = AuditEvent::new(sp_id.clone(), agent_id, "action", previous);

    // Attempt to forge by changing agent_id
    event.agent_id = malicious_agent;

    assert!(
        !event.verify_hash(),
        "event with forged agent_id MUST fail verification"
    );
}

/// COMPLIANCE: Timestamp is included in event hash.
#[test]
fn test_timestamp_in_hash() {
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();
    let previous = [0u8; 32];

    let event1 = AuditEvent::new(sp_id.clone(), agent_id.clone(), "action", previous);

    // Sleep briefly to ensure different timestamp
    std::thread::sleep(std::time::Duration::from_millis(10));

    let event2 = AuditEvent::new(sp_id.clone(), agent_id.clone(), "action", previous);

    assert_ne!(
        event1.row_hash, event2.row_hash,
        "different timestamp MUST produce different row_hash"
    );
}

/// COMPLIANCE: Event with tampered timestamp fails verification.
#[test]
fn test_tampered_timestamp_detected() {
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();
    let previous = [0u8; 32];

    let mut event = AuditEvent::new(sp_id, agent_id, "action", previous);

    // Tamper with timestamp
    event.timestamp = event.timestamp + chrono::Duration::hours(1);

    assert!(
        !event.verify_hash(),
        "event with tampered timestamp MUST fail verification"
    );
}

/// COMPLIANCE: Hash function produces 32-byte output.
#[test]
fn test_hash_output_size() {
    let input = b"test data";
    let hash = sha256(input);
    assert_eq!(hash.len(), 32, "SHA-256 hash MUST be 32 bytes");
}

/// COMPLIANCE: Hash chain produces deterministic output.
#[test]
fn test_hash_chain_deterministic() {
    let previous = [0u8; 32];
    let content = b"test event content";

    let hash1 = hash_chain_event(&previous, content);
    let hash2 = hash_chain_event(&previous, content);

    assert_eq!(
        hash1, hash2,
        "hash_chain_event MUST be deterministic"
    );
}

/// COMPLIANCE: Constant time comparison is used for hashes.
#[test]
fn test_constant_time_eq_for_hashes() {
    let hash1 = sha256(b"data1");
    let hash2 = sha256(b"data1");
    let hash3 = sha256(b"data2");

    assert!(
        constant_time_eq(&hash1, &hash2),
        "identical hashes MUST compare equal"
    );
    assert!(
        !constant_time_eq(&hash1, &hash3),
        "different hashes MUST compare not equal"
    );
}

/// COMPLIANCE: Empty chain is valid.
#[test]
fn test_empty_chain_valid() {
    let chain = AuditChain::new();
    assert!(
        chain.verify_chain(),
        "empty audit chain MUST be valid"
    );
}

/// COMPLIANCE: Single event chain is valid.
#[test]
fn test_single_event_chain_valid() {
    let mut chain = AuditChain::new();
    let sp_id = ServiceProviderId::new();
    let agent_id = AgentId::new();

    chain.append(sp_id, agent_id, "single_event");

    assert!(
        chain.verify_chain(),
        "single event chain MUST be valid"
    );
}
