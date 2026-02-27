//! AgentAuth client for authenticating agents with services.
//!
//! The `AgentAuthClient` is the primary interface for agents to:
//! - Register with the AgentAuth registry
//! - Request capability grants from service providers
//! - Obtain tokens for authenticated requests
//! - Attach authentication headers to outgoing requests

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use url::Url;

use auth_core::types::{
    AgentId, BehavioralEnvelope, Capability, ServiceProviderId, SignedManifest,
};

use crate::cache::{CachedToken, TokenCache};
use crate::config::SdkConfig;
use crate::dpop::DpopGenerator;
use crate::error::{SdkError, SdkResult};
use crate::rate_limiter::BehavioralRateLimiter;
use crate::retry::with_retry;

/// Response from the registry for token issuance.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    /// The access token (AAT).
    access_token: String,
    /// Token ID (JTI).
    jti: String,
    /// When the token expires.
    expires_at: DateTime<Utc>,
    /// Token type (always "AgentBearer").
    #[allow(dead_code)]
    token_type: String,
}

/// Response from the registry for grant requests.
#[derive(Debug, Deserialize)]
struct GrantResponse {
    /// The grant ID.
    grant_id: String,
    /// Grant status.
    status: String,
    /// Granted capabilities (if approved).
    granted_capabilities: Option<Vec<Capability>>,
    /// Behavioral envelope (if approved).
    behavioral_envelope: Option<BehavioralEnvelope>,
}

/// Error response from the registry.
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    /// Error code.
    code: String,
    /// Error message.
    error: String,
}

/// Grant request payload.
#[derive(Debug, Serialize)]
struct GrantRequest {
    agent_id: AgentId,
    service_provider_id: ServiceProviderId,
    requested_capabilities: Vec<Capability>,
    requested_envelope: BehavioralEnvelope,
}

/// Token issuance request payload.
#[derive(Debug, Serialize)]
struct IssueTokenRequest {
    grant_id: String,
    dpop_thumbprint: String,
}

/// A capability grant from the registry.
#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    /// The grant ID.
    pub grant_id: String,
    /// Service provider this grant is for.
    pub service_provider_id: ServiceProviderId,
    /// Granted capabilities.
    pub capabilities: Vec<Capability>,
    /// Behavioral envelope.
    pub envelope: BehavioralEnvelope,
}

/// Client for authenticating agents with AgentAuth-enabled services.
///
/// The client handles:
/// - Agent registration
/// - Grant requests
/// - Token issuance and caching
/// - DPoP proof generation
/// - Rate limiting enforcement
pub struct AgentAuthClient {
    /// SDK configuration.
    config: SdkConfig,

    /// HTTP client (reused for connection pooling).
    http: reqwest::Client,

    /// The agent's signed manifest.
    manifest: SignedManifest,

    /// DPoP generator for proof generation.
    dpop: DpopGenerator,

    /// Token cache.
    token_cache: TokenCache,

    /// Rate limiters per service provider.
    rate_limiters: Arc<RwLock<HashMap<ServiceProviderId, BehavioralRateLimiter>>>,

    /// Active grants per service provider.
    grants: Arc<RwLock<HashMap<ServiceProviderId, CapabilityGrant>>>,
}

impl AgentAuthClient {
    /// Creates a new AgentAuth client.
    ///
    /// # Arguments
    ///
    /// * `config` - SDK configuration
    /// * `manifest` - The agent's signed manifest
    /// * `private_key` - The agent's Ed25519 private key (32 bytes)
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid or the HTTP client
    /// cannot be created.
    pub fn new(
        config: SdkConfig,
        manifest: SignedManifest,
        private_key: &[u8; 32],
    ) -> SdkResult<Self> {
        config.validate()?;

        // Create HTTP client with connection pooling
        let http = reqwest::Client::builder()
            .timeout(config.request_timeout)
            .connect_timeout(config.connect_timeout)
            .pool_max_idle_per_host(10)
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| SdkError::ConfigError(format!("Failed to create HTTP client: {e}")))?;

        // Create DPoP generator
        let dpop = DpopGenerator::new(private_key)?;

        // Create token cache
        let token_cache = TokenCache::new(config.token_refresh.refresh_before_expiry);

        Ok(Self {
            config,
            http,
            manifest,
            dpop,
            token_cache,
            rate_limiters: Arc::new(RwLock::new(HashMap::new())),
            grants: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Returns the agent's ID.
    #[must_use]
    pub fn agent_id(&self) -> &AgentId {
        &self.manifest.manifest.id
    }

    /// Returns the registry URL.
    #[must_use]
    pub fn registry_url(&self) -> &Url {
        &self.config.registry_url
    }

    /// Registers the agent with the registry.
    ///
    /// This should be called once when the agent is first deployed.
    /// Subsequent calls are idempotent.
    ///
    /// # Errors
    ///
    /// Returns an error if registration fails.
    pub async fn register(&self) -> SdkResult<()> {
        let url = self.build_url("/v1/agents/register")?;

        let response = with_retry(&self.config.retry, || async {
            self.http
                .post(url.as_str())
                .header(CONTENT_TYPE, "application/json")
                .json(&self.manifest)
                .send()
                .await
                .map_err(SdkError::from)
        })
        .await?;

        self.handle_response(response).await?;

        tracing::info!(
            agent_id = %self.agent_id(),
            "Agent registered successfully"
        );

        Ok(())
    }

    /// Requests a capability grant from a service provider.
    ///
    /// # Arguments
    ///
    /// * `service_provider_id` - The service provider to request capabilities from
    /// * `capabilities` - The capabilities being requested
    /// * `envelope` - The behavioral envelope for the grant
    ///
    /// # Returns
    ///
    /// The granted capabilities and envelope. Note that these may differ from
    /// what was requested (e.g., more restrictive envelope).
    ///
    /// # Errors
    ///
    /// Returns an error if the grant request fails or is denied.
    pub async fn request_grant(
        &self,
        service_provider_id: ServiceProviderId,
        capabilities: Vec<Capability>,
        envelope: BehavioralEnvelope,
    ) -> SdkResult<CapabilityGrant> {
        let url = self.build_url("/v1/grants/request")?;

        let request_body = GrantRequest {
            agent_id: *self.agent_id(),
            service_provider_id,
            requested_capabilities: capabilities,
            requested_envelope: envelope,
        };

        let response = with_retry(&self.config.retry, || async {
            self.http
                .post(url.as_str())
                .header(CONTENT_TYPE, "application/json")
                .json(&request_body)
                .send()
                .await
                .map_err(SdkError::from)
        })
        .await?;

        let grant_response: GrantResponse = self.handle_json_response(response).await?;

        match grant_response.status.as_str() {
            "approved" => {
                let capabilities = grant_response.granted_capabilities.ok_or_else(|| {
                    SdkError::InternalError("Approved grant missing capabilities".to_string())
                })?;

                let envelope = grant_response.behavioral_envelope.ok_or_else(|| {
                    SdkError::InternalError("Approved grant missing envelope".to_string())
                })?;

                let grant = CapabilityGrant {
                    grant_id: grant_response.grant_id.clone(),
                    service_provider_id,
                    capabilities,
                    envelope: envelope.clone(),
                };

                // Create rate limiter for this grant
                let rate_limiter = BehavioralRateLimiter::new(envelope)?;

                // Store grant and rate limiter
                {
                    let mut grants = self.grants.write().await;
                    grants.insert(service_provider_id, grant.clone());
                }
                {
                    let mut limiters = self.rate_limiters.write().await;
                    limiters.insert(service_provider_id, rate_limiter);
                }

                tracing::info!(
                    agent_id = %self.agent_id(),
                    service_provider_id = %service_provider_id,
                    grant_id = %grant_response.grant_id,
                    "Grant approved"
                );

                Ok(grant)
            }
            "pending" => Err(SdkError::GrantPending {
                grant_id: grant_response.grant_id,
            }),
            "denied" => Err(SdkError::GrantDenied {
                reason: "Grant request denied".to_string(),
            }),
            "expired" => Err(SdkError::GrantExpired {
                grant_id: grant_response.grant_id,
            }),
            status => Err(SdkError::InternalError(format!(
                "Unknown grant status: {status}"
            ))),
        }
    }

    /// Gets a token for a service provider.
    ///
    /// Returns a cached token if available and not near expiry.
    /// Otherwise issues a new token from the registry.
    ///
    /// # Arguments
    ///
    /// * `service_provider_id` - The service provider to get a token for
    ///
    /// # Errors
    ///
    /// Returns an error if no grant exists or token issuance fails.
    pub async fn get_token(&self, service_provider_id: &ServiceProviderId) -> SdkResult<String> {
        // Check cache first
        if let Some(cached) = self.token_cache.get(service_provider_id).await {
            if !cached.should_refresh(self.config.token_refresh.refresh_before_expiry) {
                tracing::debug!(
                    service_provider_id = %service_provider_id,
                    jti = %cached.jti,
                    "Using cached token"
                );
                return Ok(cached.token);
            }

            // Token needs refresh but is still valid - use it while refreshing
            if !cached.is_expired() {
                // Trigger background refresh if proactive refresh is enabled
                if self.config.token_refresh.proactive_refresh {
                    let sp_id = *service_provider_id;
                    let client = self.clone_for_refresh();
                    tokio::spawn(async move {
                        let _ = client.refresh_token(&sp_id).await;
                    });
                }
                return Ok(cached.token);
            }
        }

        // Issue new token
        self.issue_token(service_provider_id).await
    }

    /// Issues a new token from the registry.
    async fn issue_token(&self, service_provider_id: &ServiceProviderId) -> SdkResult<String> {
        // Get the grant
        let grant = {
            let grants = self.grants.read().await;
            grants.get(service_provider_id).cloned()
        }
        .ok_or_else(|| {
            SdkError::CapabilityNotGranted(format!(
                "No grant for service provider: {service_provider_id}"
            ))
        })?;

        let url = self.build_url("/v1/tokens/issue")?;

        let request_body = IssueTokenRequest {
            grant_id: grant.grant_id.clone(),
            dpop_thumbprint: self.dpop.thumbprint(),
        };

        // Generate DPoP proof for this request
        let dpop_proof = self.dpop.generate("POST", url.as_str(), None)?;

        let response = with_retry(&self.config.retry, || async {
            self.http
                .post(url.as_str())
                .header(CONTENT_TYPE, "application/json")
                .header("AgentDPoP", dpop_proof.as_str())
                .json(&request_body)
                .send()
                .await
                .map_err(SdkError::from)
        })
        .await?;

        let token_response: TokenResponse = self.handle_json_response(response).await?;

        // Cache the token
        let cached = CachedToken::new(
            token_response.access_token.clone(),
            token_response.jti.clone(),
            token_response.expires_at,
            *service_provider_id,
        );
        self.token_cache.put(cached).await;

        tracing::debug!(
            service_provider_id = %service_provider_id,
            jti = %token_response.jti,
            expires_at = %token_response.expires_at,
            "Token issued"
        );

        Ok(token_response.access_token)
    }

    /// Refreshes a token for a service provider.
    #[allow(dead_code)]
    async fn refresh_token(&self, service_provider_id: &ServiceProviderId) -> SdkResult<String> {
        tracing::debug!(
            service_provider_id = %service_provider_id,
            "Refreshing token"
        );
        self.issue_token(service_provider_id).await
    }

    /// Authenticates an outgoing request to a service provider.
    ///
    /// This adds the `Authorization` and `AgentDPoP` headers to the request.
    ///
    /// # Arguments
    ///
    /// * `service_provider_id` - The service provider being called
    /// * `method` - The HTTP method
    /// * `url` - The request URL
    /// * `headers` - The request headers to modify
    ///
    /// # Errors
    ///
    /// Returns an error if rate limit is exceeded or token retrieval fails.
    pub async fn authenticate_request(
        &self,
        service_provider_id: &ServiceProviderId,
        method: &str,
        url: &str,
        headers: &mut HeaderMap,
    ) -> SdkResult<()> {
        // Check rate limit
        {
            let limiters = self.rate_limiters.read().await;
            if let Some(limiter) = limiters.get(service_provider_id) {
                // Assume human is not present for automated requests
                // Transaction value is None for non-transact operations
                limiter.check(false, None)?;
            }
        }

        // Get token
        let token = self.get_token(service_provider_id).await?;

        // Generate DPoP proof
        let dpop_proof = self.dpop.generate(method, url, Some(&token))?;

        // Add headers
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("AgentBearer {token}"))
                .map_err(|e| SdkError::InternalError(format!("Invalid token format: {e}")))?,
        );
        headers.insert(
            "AgentDPoP",
            HeaderValue::from_str(dpop_proof.as_str())
                .map_err(|e| SdkError::InternalError(format!("Invalid DPoP format: {e}")))?,
        );

        Ok(())
    }

    /// Builds a URL for a registry endpoint.
    fn build_url(&self, path: &str) -> SdkResult<Url> {
        self.config
            .registry_url
            .join(path)
            .map_err(|e| SdkError::ConfigError(format!("Invalid URL path: {e}")))
    }

    /// Handles a response and checks for errors.
    async fn handle_response(&self, response: reqwest::Response) -> SdkResult<()> {
        let status = response.status();

        if status.is_success() {
            return Ok(());
        }

        self.handle_error_response(response, status.as_u16()).await
    }

    /// Handles a JSON response.
    async fn handle_json_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> SdkResult<T> {
        let status = response.status();

        if !status.is_success() {
            return self.handle_error_response(response, status.as_u16()).await;
        }

        response
            .json()
            .await
            .map_err(|e| SdkError::SerializationError(format!("Failed to parse response: {e}")))
    }

    /// Handles an error response from the registry.
    async fn handle_error_response<T>(
        &self,
        response: reqwest::Response,
        status: u16,
    ) -> SdkResult<T> {
        // Try to parse error response
        let error = response
            .json::<ErrorResponse>()
            .await
            .unwrap_or_else(|_| ErrorResponse {
                code: "UNKNOWN".to_string(),
                error: "Unknown error".to_string(),
            });

        // Check for specific error conditions
        match error.code.as_str() {
            "GRANT_DENIED" => Err(SdkError::GrantDenied {
                reason: error.error,
            }),
            "GRANT_PENDING" => Err(SdkError::GrantPending {
                grant_id: error.error,
            }),
            "AGENT_REVOKED" => Err(SdkError::AgentRevoked {
                agent_id: error.error,
            }),
            "TOKEN_EXPIRED" => Err(SdkError::TokenExpired),
            _ => Err(SdkError::RegistryError {
                code: error.code,
                message: error.error,
                status,
            }),
        }
    }

    /// Creates a lightweight clone for background refresh operations.
    fn clone_for_refresh(&self) -> RefreshClient {
        RefreshClient {
            http: self.http.clone(),
            config: self.config.clone(),
            dpop_thumbprint: self.dpop.thumbprint(),
            token_cache: self.token_cache.clone(),
            grants: self.grants.clone(),
        }
    }
}

impl std::fmt::Debug for AgentAuthClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentAuthClient")
            .field("agent_id", &self.manifest.manifest.id)
            .field("registry_url", &self.config.registry_url)
            .finish_non_exhaustive()
    }
}

/// Lightweight client for background token refresh.
#[allow(dead_code)]
struct RefreshClient {
    http: reqwest::Client,
    config: SdkConfig,
    dpop_thumbprint: String,
    token_cache: TokenCache,
    grants: Arc<RwLock<HashMap<ServiceProviderId, CapabilityGrant>>>,
}

#[allow(dead_code)]
impl RefreshClient {
    #[allow(clippy::unused_async)]
    async fn refresh_token(&self, service_provider_id: &ServiceProviderId) -> SdkResult<()> {
        // This is a simplified refresh - in practice you'd need full DPoP generation
        tracing::debug!(
            service_provider_id = %service_provider_id,
            "Background token refresh"
        );
        // Token refresh would happen here
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use auth_core::types::{AgentManifest, HumanPrincipalId};
    use chrono::Duration;

    fn make_manifest() -> SignedManifest {
        let manifest = AgentManifest {
            id: AgentId::new(),
            public_key: "test-public-key".to_string(),
            key_id: "test-key-id".to_string(),
            capabilities_requested: vec![Capability::Read {
                resource: "test".to_string(),
                filter: None,
            }],
            human_principal_id: HumanPrincipalId::new(),
            issued_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(24),
            name: "Test Agent".to_string(),
            description: None,
            model_origin: None,
        };

        SignedManifest {
            manifest,
            signature: "test-signature".to_string(),
            signing_key_id: "test-key-id".to_string(),
        }
    }

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        key[0] = 1;
        key
    }

    #[tokio::test]
    async fn test_client_creation() {
        let config = SdkConfig::new("https://registry.example.com").expect("config");
        let manifest = make_manifest();

        let client = AgentAuthClient::new(config, manifest, &test_key());
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn test_client_rejects_http_non_localhost() {
        let config = SdkConfig::new("http://registry.example.com");
        assert!(config.is_err());
    }

    #[tokio::test]
    async fn test_client_allows_localhost_http() {
        let config = SdkConfig::new("http://localhost:8080").expect("config");
        let manifest = make_manifest();

        let client = AgentAuthClient::new(config, manifest, &test_key());
        assert!(client.is_ok());
    }
}
