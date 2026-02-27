//! AgentAuth SDK
//!
//! This crate provides the SDK for agents to authenticate with AgentAuth-enabled services.
//!
//! # Overview
//!
//! The AgentAuth SDK allows AI agents to:
//! - Register with the AgentAuth registry
//! - Request capability grants from service providers
//! - Obtain and cache access tokens
//! - Authenticate requests to service providers using DPoP proofs
//!
//! # Example
//!
//! ```no_run
//! use sdk::{AgentAuthClient, SdkConfig};
//! use auth_core::types::{Capability, BehavioralEnvelope, ServiceProviderId};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create SDK configuration
//! let config = SdkConfig::new("https://registry.example.com")?;
//!
//! // Create client with agent manifest and private key
//! // (manifest and key would come from agent configuration)
//! # let manifest = todo!();
//! # let private_key = [0u8; 32];
//! let client = AgentAuthClient::new(config, manifest, &private_key)?;
//!
//! // Register the agent (idempotent)
//! client.register().await?;
//!
//! // Request a grant
//! let capabilities = vec![Capability::Read {
//!     resource: "calendar".to_string(),
//!     filter: None,
//! }];
//! let envelope = BehavioralEnvelope::default_restrictive();
//! let service_provider_id = ServiceProviderId::new();
//!
//! let grant = client.request_grant(service_provider_id, capabilities, envelope).await?;
//!
//! // Get a token (cached automatically)
//! let token = client.get_token(&service_provider_id).await?;
//!
//! // Authenticate an outgoing request
//! let mut headers = reqwest::header::HeaderMap::new();
//! client.authenticate_request(
//!     &service_provider_id,
//!     "GET",
//!     "https://api.example.com/calendar",
//!     &mut headers,
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Security
//!
//! The SDK enforces several security measures:
//!
//! - **DPoP Proofs**: Every authenticated request includes a DPoP proof to prevent
//!   token theft and replay attacks.
//! - **Behavioral Rate Limiting**: The SDK enforces the behavioral envelope constraints
//!   client-side, preventing the agent from exceeding its granted limits.
//! - **Token Caching**: Tokens are cached and automatically refreshed before expiry,
//!   reducing unnecessary network calls.
//! - **HTTPS Required**: The SDK requires HTTPS for registry connections (except localhost
//!   for development).

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]

pub mod cache;
pub mod client;
pub mod config;
pub mod dpop;
pub mod error;
pub mod rate_limiter;
pub mod retry;

// Re-export primary types
pub use cache::{CachedToken, TokenCache};
pub use client::{AgentAuthClient, CapabilityGrant};
pub use config::{RetryConfig, SdkConfig, TokenRefreshConfig};
pub use dpop::{DpopGenerator, DpopProof};
pub use error::{SdkError, SdkResult};
pub use rate_limiter::BehavioralRateLimiter;
pub use retry::with_retry;

// Re-export commonly used types from the core crate
pub use auth_core::types::{
    AgentId, AgentManifest, BehavioralEnvelope, Capability, HumanPrincipalId, ServiceProviderId,
    SignedManifest,
};
