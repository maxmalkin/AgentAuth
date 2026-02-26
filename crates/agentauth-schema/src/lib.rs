//! AgentAuth JSON Schema
//!
//! This crate provides JSON Schema definitions for AgentAuth discovery documents
//! and validation utilities.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Validation errors.
#[derive(Debug, Error)]
pub enum ValidationError {
    /// JSON parsing failed.
    #[error("Invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    /// Schema validation failed.
    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    /// Schema compilation failed.
    #[error("Failed to compile schema: {0}")]
    SchemaCompilation(String),
}

/// AgentAuth discovery document.
///
/// This is the machine-readable protocol advertisement published at
/// `/.well-known/agentauth`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiscoveryDocument {
    /// Protocol version (e.g., "1.0").
    pub agentauth_version: String,

    /// Registry endpoint URL.
    pub registry_endpoint: String,

    /// Verifier endpoint URL.
    pub verifier_endpoint: String,

    /// Supported capability types.
    pub supported_capabilities: Vec<String>,

    /// Supported resource types.
    pub supported_resources: Vec<String>,

    /// Trusted model origins (domains).
    pub trusted_model_origins: Vec<String>,

    /// Token verification endpoint URL.
    pub token_endpoint: String,

    /// Approval UI endpoint URL.
    pub approval_ui_endpoint: String,

    /// Agent bootstrap endpoint URL.
    pub bootstrap_endpoint: String,

    /// Current registry public key (base64url-encoded Ed25519).
    pub public_key: String,

    /// Keys endpoint URL for key rotation support.
    pub keys_endpoint: String,

    /// Behavioral limits enforced by the registry.
    pub behavioral_limits: BehavioralLimits,
}

/// Behavioral limits from the registry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BehavioralLimits {
    /// Maximum requests per minute allowed.
    pub max_requests_per_minute: u32,

    /// Maximum burst allowed.
    pub max_burst: u32,

    /// Maximum token lifetime in seconds.
    pub max_token_lifetime_seconds: u32,
}

/// Public key entry in the keys response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PublicKeyEntry {
    /// Key ID.
    pub kid: String,

    /// Key type (always "OKP" for Ed25519).
    pub kty: String,

    /// Curve (always "Ed25519").
    pub crv: String,

    /// Public key bytes (base64url-encoded).
    pub x: String,

    /// Key status: "active" or "retired".
    pub status: String,

    /// Expiration time for retired keys (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Keys response from the keys endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeysResponse {
    /// List of public keys.
    pub keys: Vec<PublicKeyEntry>,
}

/// Get the JSON Schema for the discovery document.
#[must_use]
pub fn discovery_document_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(DiscoveryDocument);
    serde_json::to_value(schema).unwrap_or_default()
}

/// Get the JSON Schema for the keys response.
#[must_use]
pub fn keys_response_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(KeysResponse);
    serde_json::to_value(schema).unwrap_or_default()
}

/// Validate a discovery document against the schema.
///
/// # Errors
///
/// Returns an error if the document is invalid JSON or fails schema validation.
pub fn validate_discovery_document(json: &str) -> Result<DiscoveryDocument, ValidationError> {
    // First, parse the JSON
    let value: serde_json::Value = serde_json::from_str(json)?;

    // Get the schema
    let schema = discovery_document_schema();

    // Compile the schema
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|e| ValidationError::SchemaCompilation(e.to_string()))?;

    // Validate
    if let Err(errors) = compiled.validate(&value) {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return Err(ValidationError::SchemaValidation(error_messages.join("; ")));
    }

    // Deserialize to the struct
    let doc: DiscoveryDocument = serde_json::from_value(value)?;
    Ok(doc)
}

/// Validate a keys response against the schema.
///
/// # Errors
///
/// Returns an error if the response is invalid JSON or fails schema validation.
pub fn validate_keys_response(json: &str) -> Result<KeysResponse, ValidationError> {
    // First, parse the JSON
    let value: serde_json::Value = serde_json::from_str(json)?;

    // Get the schema
    let schema = keys_response_schema();

    // Compile the schema
    let compiled = jsonschema::JSONSchema::compile(&schema)
        .map_err(|e| ValidationError::SchemaCompilation(e.to_string()))?;

    // Validate
    if let Err(errors) = compiled.validate(&value) {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        return Err(ValidationError::SchemaValidation(error_messages.join("; ")));
    }

    // Deserialize to the struct
    let response: KeysResponse = serde_json::from_value(value)?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovery_document_schema_generation() {
        let schema = discovery_document_schema();
        assert!(schema.is_object());
        assert!(schema.get("$schema").is_some());
    }

    #[test]
    fn test_keys_response_schema_generation() {
        let schema = keys_response_schema();
        assert!(schema.is_object());
        assert!(schema.get("$schema").is_some());
    }

    #[test]
    fn test_validate_valid_discovery_document() {
        let json = r#"{
            "agentauth_version": "1.0",
            "registry_endpoint": "https://registry.example.com/v1",
            "verifier_endpoint": "https://verifier.example.com/v1",
            "supported_capabilities": ["read", "write", "transact", "custom"],
            "supported_resources": ["calendar", "email", "files", "messages"],
            "trusted_model_origins": ["anthropic.com", "openai.com"],
            "token_endpoint": "https://verifier.example.com/v1/tokens/verify",
            "approval_ui_endpoint": "https://approval.example.com",
            "bootstrap_endpoint": "https://registry.example.com/v1/agents/bootstrap",
            "public_key": "dGVzdC1wdWJsaWMta2V5",
            "keys_endpoint": "https://registry.example.com/.well-known/agentauth/keys",
            "behavioral_limits": {
                "max_requests_per_minute": 60,
                "max_burst": 10,
                "max_token_lifetime_seconds": 900
            }
        }"#;

        let result = validate_discovery_document(json);
        assert!(result.is_ok());

        let doc = result.expect("should parse");
        assert_eq!(doc.agentauth_version, "1.0");
        assert_eq!(doc.supported_capabilities.len(), 4);
    }

    #[test]
    fn test_validate_invalid_discovery_document_missing_field() {
        let json = r#"{
            "agentauth_version": "1.0",
            "registry_endpoint": "https://registry.example.com/v1"
        }"#;

        let result = validate_discovery_document(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_invalid_json() {
        let json = "not valid json";
        let result = validate_discovery_document(json);
        assert!(matches!(result, Err(ValidationError::InvalidJson(_))));
    }

    #[test]
    fn test_validate_valid_keys_response() {
        let json = r#"{
            "keys": [
                {
                    "kid": "key-1",
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": "dGVzdC1wdWJsaWMta2V5",
                    "status": "active"
                }
            ]
        }"#;

        let result = validate_keys_response(json);
        assert!(result.is_ok());

        let response = result.expect("should parse");
        assert_eq!(response.keys.len(), 1);
        assert_eq!(response.keys[0].kid, "key-1");
    }

    #[test]
    fn test_validate_keys_response_with_retired_key() {
        let json = r#"{
            "keys": [
                {
                    "kid": "key-1",
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": "dGVzdC1wdWJsaWMta2V5",
                    "status": "active"
                },
                {
                    "kid": "key-0",
                    "kty": "OKP",
                    "crv": "Ed25519",
                    "x": "b2xkLXB1YmxpYy1rZXk",
                    "status": "retired",
                    "expires_at": "2025-12-31T23:59:59Z"
                }
            ]
        }"#;

        let result = validate_keys_response(json);
        assert!(result.is_ok());

        let response = result.expect("should parse");
        assert_eq!(response.keys.len(), 2);
        assert!(response.keys[1].expires_at.is_some());
    }

    #[test]
    fn test_discovery_document_roundtrip() {
        let doc = DiscoveryDocument {
            agentauth_version: "1.0".to_string(),
            registry_endpoint: "https://registry.example.com/v1".to_string(),
            verifier_endpoint: "https://verifier.example.com/v1".to_string(),
            supported_capabilities: vec!["read".to_string(), "write".to_string()],
            supported_resources: vec!["calendar".to_string()],
            trusted_model_origins: vec!["anthropic.com".to_string()],
            token_endpoint: "https://verifier.example.com/v1/tokens/verify".to_string(),
            approval_ui_endpoint: "https://approval.example.com".to_string(),
            bootstrap_endpoint: "https://registry.example.com/v1/agents/bootstrap".to_string(),
            public_key: "dGVzdC1rZXk".to_string(),
            keys_endpoint: "https://registry.example.com/.well-known/agentauth/keys".to_string(),
            behavioral_limits: BehavioralLimits {
                max_requests_per_minute: 60,
                max_burst: 10,
                max_token_lifetime_seconds: 900,
            },
        };

        let json = serde_json::to_string(&doc).expect("serialize");
        let parsed = validate_discovery_document(&json);
        assert!(parsed.is_ok());
    }
}
