//! Test data factory functions.

use auth_core::{AgentId, AgentManifest, Capability, HumanPrincipalId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use super::setup::TestSigningBackend;

/// Create a test agent manifest signed by the given backend.
/// Returns (manifest_json, agent_id, human_principal_id).
pub fn create_signed_agent(signer: &TestSigningBackend) -> (serde_json::Value, Uuid, Uuid, Uuid) {
    let agent_id = Uuid::now_v7();
    let hp_id = Uuid::now_v7();
    let sp_id = Uuid::now_v7();
    let now = Utc::now();

    let public_key = URL_SAFE_NO_PAD.encode(signer.public_key_bytes());
    let key_id = "test-key-1";

    let manifest = AgentManifest {
        id: AgentId::from_uuid(agent_id),
        public_key: public_key.clone(),
        key_id: key_id.to_string(),
        capabilities_requested: vec![
            Capability::Read {
                resource: "calendar".into(),
                filter: None,
            },
            Capability::Write {
                resource: "files".into(),
                conditions: None,
            },
        ],
        human_principal_id: HumanPrincipalId::from_uuid(hp_id),
        issued_at: now,
        expires_at: now + Duration::hours(24),
        name: format!("Test Agent {agent_id}"),
        description: Some("Integration test agent".into()),
        model_origin: Some("anthropic.com".into()),
    };

    let canonical_bytes = manifest
        .to_canonical_bytes()
        .expect("manifest serialization");
    let signature = signer.sign_bytes(&canonical_bytes);
    let sig_hex = hex::encode(signature);

    let manifest_json = serde_json::to_value(&manifest).expect("manifest to json");

    let body = json!({
        "manifest": manifest_json,
        "signature": sig_hex,
    });

    (body, agent_id, hp_id, sp_id)
}

/// Create a grant request body.
pub fn create_grant_request(agent_id: Uuid, sp_id: Uuid) -> serde_json::Value {
    json!({
        "agent_id": agent_id,
        "service_provider_id": sp_id,
        "capabilities": [
            { "type": "read", "resource": "calendar" }
        ],
        "behavioral_envelope": default_envelope_json(),
    })
}

/// Create an approve grant request body.
pub fn create_approve_request(hp_id: Uuid) -> serde_json::Value {
    let nonce = hex::encode(auth_core::crypto::generate_nonce());
    // For testing, we use a dummy signature (32 bytes + 32 bytes = 64 bytes).
    let dummy_sig = hex::encode([0xABu8; 64]);

    json!({
        "approved_by": hp_id,
        "approval_nonce": nonce,
        "approval_signature": dummy_sig,
    })
}

/// Create a token issuance request body.
pub fn create_issue_request(
    grant_id: Uuid,
    agent_id: Uuid,
    sp_id: Uuid,
    hp_id: Uuid,
) -> serde_json::Value {
    json!({
        "grant_id": grant_id,
        "agent_id": agent_id,
        "service_provider_id": sp_id,
        "human_principal_id": hp_id,
        "capabilities": [
            { "type": "read", "resource": "calendar" }
        ],
        "behavioral_envelope": default_envelope_json(),
    })
}

/// Create a verify token request body.
pub fn create_verify_request(jti: Uuid, sp_id: Uuid) -> serde_json::Value {
    let nonce = hex::encode(auth_core::crypto::generate_nonce());
    json!({
        "jti": jti,
        "service_provider_id": sp_id,
        "nonce": nonce,
    })
}

/// Create a verify token request with a specific nonce.
pub fn create_verify_request_with_nonce(jti: Uuid, sp_id: Uuid, nonce: &str) -> serde_json::Value {
    json!({
        "jti": jti,
        "service_provider_id": sp_id,
        "nonce": nonce,
    })
}

/// Create a revoke token request body.
pub fn create_revoke_request(jti: Uuid) -> serde_json::Value {
    json!({
        "jti": jti,
        "reason": "integration test revocation",
    })
}

/// Default behavioral envelope as JSON.
fn default_envelope_json() -> serde_json::Value {
    json!({
        "max_requests_per_minute": 30,
        "max_burst": 5,
        "requires_human_online": false,
        "max_session_duration_secs": 3600
    })
}
