//! Discovery document handlers.

use crate::state::AppState;
use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::Serialize;

/// AgentAuth discovery document.
///
/// Published at `/.well-known/agentauth`
#[derive(Debug, Serialize)]
pub struct DiscoveryDocument {
    /// Protocol version.
    pub agentauth_version: String,
    /// Registry endpoint URL.
    pub registry_endpoint: String,
    /// Verifier endpoint URL.
    pub verifier_endpoint: String,
    /// Supported capabilities.
    pub supported_capabilities: Vec<String>,
    /// Supported resources.
    pub supported_resources: Vec<String>,
    /// Trusted model origins.
    pub trusted_model_origins: Vec<String>,
    /// Token verification endpoint.
    pub token_endpoint: String,
    /// Approval UI endpoint.
    pub approval_ui_endpoint: String,
    /// Bootstrap endpoint.
    pub bootstrap_endpoint: String,
    /// Current registry public key (base64url).
    pub public_key: String,
    /// Keys endpoint URL.
    pub keys_endpoint: String,
    /// Behavioral limits.
    pub behavioral_limits: BehavioralLimits,
}

/// Behavioral limits from the registry.
#[derive(Debug, Serialize)]
pub struct BehavioralLimits {
    /// Maximum requests per minute.
    pub max_requests_per_minute: u32,
    /// Maximum burst.
    pub max_burst: u32,
    /// Maximum token lifetime in seconds.
    pub max_token_lifetime_seconds: u32,
}

/// Public key entry.
#[derive(Debug, Serialize)]
pub struct PublicKeyEntry {
    /// Key ID.
    pub kid: String,
    /// Key type.
    pub kty: String,
    /// Curve (for EC keys).
    pub crv: String,
    /// Public key bytes (base64url).
    pub x: String,
    /// Key status.
    pub status: String,
    /// Expires at (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Keys response.
#[derive(Debug, Serialize)]
pub struct KeysResponse {
    /// List of public keys.
    pub keys: Vec<PublicKeyEntry>,
}

/// Get the AgentAuth discovery document.
///
/// GET /.well-known/agentauth
pub async fn get_discovery_document(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let config = &state.config;

    // Get the current public key
    let public_key = match state.signer.public_key().await {
        Ok(pk) => base64_url_encode(pk.as_bytes()),
        Err(_) => String::new(),
    };

    let base_url = &config.server.external_url;

    let verifier_url = &config.server.verifier_url;
    let doc = DiscoveryDocument {
        agentauth_version: "1.0".to_string(),
        registry_endpoint: format!("{base_url}/v1"),
        verifier_endpoint: config.server.verifier_url.clone(),
        supported_capabilities: vec![
            "read".to_string(),
            "write".to_string(),
            "transact".to_string(),
            "custom".to_string(),
        ],
        supported_resources: vec![
            "calendar".to_string(),
            "email".to_string(),
            "files".to_string(),
            "messages".to_string(),
        ],
        trusted_model_origins: vec![
            "anthropic.com".to_string(),
            "openai.com".to_string(),
        ],
        token_endpoint: format!("{verifier_url}/v1/tokens/verify"),
        approval_ui_endpoint: config.server.approval_ui_url.clone(),
        bootstrap_endpoint: format!("{base_url}/v1/agents/bootstrap"),
        public_key,
        keys_endpoint: format!("{base_url}/.well-known/agentauth/keys"),
        behavioral_limits: BehavioralLimits {
            max_requests_per_minute: config.grants.max_requests_per_minute,
            max_burst: config.grants.max_burst,
            #[allow(clippy::cast_possible_truncation)]
            max_token_lifetime_seconds: config.tokens.lifetime_secs as u32,
        },
    };

    Json(doc)
}

/// Get all public keys (current and retired verify-only).
///
/// GET /.well-known/agentauth/keys
pub async fn get_keys(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut keys = Vec::new();

    // Get current signing key
    match state.signer.public_key().await {
        Ok(pk) => {
            keys.push(PublicKeyEntry {
                kid: state.signer.key_id().to_string(),
                kty: "OKP".to_string(),
                crv: "Ed25519".to_string(),
                x: base64_url_encode(pk.as_bytes()),
                status: "active".to_string(),
                expires_at: None,
            });
        }
        Err(e) => {
            tracing::error!("Failed to get public key: {e}");
        }
    }

    // In a full implementation, we'd also query historical keys from the database
    // that are still valid for verification but no longer used for signing

    Json(KeysResponse { keys })
}

/// Base64url encode bytes.
fn base64_url_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data)
}
