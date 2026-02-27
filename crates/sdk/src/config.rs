//! Configuration types for the AgentAuth SDK.

use std::time::Duration;
use url::Url;

use crate::error::{SdkError, SdkResult};

/// Configuration for the AgentAuth SDK client.
#[derive(Debug, Clone)]
pub struct SdkConfig {
    /// URL of the AgentAuth registry service.
    pub registry_url: Url,

    /// Request timeout for registry operations.
    pub request_timeout: Duration,

    /// Connection timeout.
    pub connect_timeout: Duration,

    /// Retry configuration.
    pub retry: RetryConfig,

    /// Token refresh configuration.
    pub token_refresh: TokenRefreshConfig,

    /// User-Agent header to send with requests.
    pub user_agent: String,
}

impl SdkConfig {
    /// Creates a new SDK configuration with the given registry URL.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or uses an insecure scheme.
    pub fn new(registry_url: &str) -> SdkResult<Self> {
        let url = Url::parse(registry_url)
            .map_err(|e| SdkError::ConfigError(format!("Invalid registry URL: {e}")))?;

        // Require HTTPS in production (allow http for localhost in development)
        if url.scheme() != "https" && !is_localhost(&url) {
            return Err(SdkError::ConfigError(
                "Registry URL must use HTTPS".to_string(),
            ));
        }

        Ok(Self {
            registry_url: url,
            request_timeout: Duration::from_secs(5),
            connect_timeout: Duration::from_secs(2),
            retry: RetryConfig::default(),
            token_refresh: TokenRefreshConfig::default(),
            user_agent: format!("sdk/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    /// Sets the request timeout.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Sets the connection timeout.
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Sets the retry configuration.
    #[must_use]
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Sets the token refresh configuration.
    #[must_use]
    pub fn with_token_refresh(mut self, refresh: TokenRefreshConfig) -> Self {
        self.token_refresh = refresh;
        self
    }

    /// Sets the user-agent string.
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Validates the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> SdkResult<()> {
        if self.request_timeout.is_zero() {
            return Err(SdkError::ConfigError(
                "request_timeout cannot be zero".to_string(),
            ));
        }

        if self.connect_timeout.is_zero() {
            return Err(SdkError::ConfigError(
                "connect_timeout cannot be zero".to_string(),
            ));
        }

        self.retry.validate()?;
        self.token_refresh.validate()?;

        Ok(())
    }
}

/// Retry configuration for transient errors.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_attempts: u32,

    /// Initial delay before first retry.
    pub initial_delay: Duration,

    /// Maximum delay between retries.
    pub max_delay: Duration,

    /// Multiplier for exponential backoff.
    pub multiplier: f64,

    /// Whether to add jitter to delays.
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Creates a retry config with no retries.
    #[must_use]
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 0,
            ..Self::default()
        }
    }

    /// Validates the retry configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> SdkResult<()> {
        if self.multiplier < 1.0 {
            return Err(SdkError::ConfigError(
                "retry multiplier must be >= 1.0".to_string(),
            ));
        }

        if self.initial_delay > self.max_delay {
            return Err(SdkError::ConfigError(
                "initial_delay cannot exceed max_delay".to_string(),
            ));
        }

        Ok(())
    }

    /// Calculates the delay for a given attempt number.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let base_delay = self.initial_delay.as_secs_f64()
            * self
                .multiplier
                .powi(i32::try_from(attempt - 1).unwrap_or(i32::MAX));

        let capped_delay = base_delay.min(self.max_delay.as_secs_f64());

        let final_delay = if self.jitter {
            use rand::Rng;
            let jitter_factor = rand::thread_rng().gen_range(0.0..1.0);
            capped_delay * jitter_factor
        } else {
            capped_delay
        };

        Duration::from_secs_f64(final_delay)
    }
}

/// Token refresh configuration.
#[derive(Debug, Clone)]
pub struct TokenRefreshConfig {
    /// Time before expiry to trigger a refresh.
    pub refresh_before_expiry: Duration,

    /// Whether to proactively refresh tokens in the background.
    pub proactive_refresh: bool,
}

impl Default for TokenRefreshConfig {
    fn default() -> Self {
        Self {
            refresh_before_expiry: Duration::from_secs(120), // 2 minutes before expiry
            proactive_refresh: true,
        }
    }
}

impl TokenRefreshConfig {
    /// Validates the token refresh configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> SdkResult<()> {
        // Tokens have a max lifetime of 15 minutes (900 seconds)
        // Refresh window should be less than that
        if self.refresh_before_expiry > Duration::from_secs(900) {
            return Err(SdkError::ConfigError(
                "refresh_before_expiry cannot exceed 15 minutes".to_string(),
            ));
        }

        Ok(())
    }
}

/// Checks if a URL is localhost.
fn is_localhost(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h == "[::1]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new_valid_https() {
        let config = SdkConfig::new("https://registry.example.com");
        assert!(config.is_ok());
    }

    #[test]
    fn test_config_new_valid_localhost_http() {
        let config = SdkConfig::new("http://localhost:8080");
        assert!(config.is_ok());
    }

    #[test]
    fn test_config_new_rejects_http_non_localhost() {
        let config = SdkConfig::new("http://registry.example.com");
        assert!(config.is_err());
    }

    #[test]
    fn test_config_new_invalid_url() {
        let config = SdkConfig::new("not a url");
        assert!(config.is_err());
    }

    #[test]
    fn test_retry_delay_calculation() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            multiplier: 2.0,
            jitter: false,
        };

        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(400));
    }

    #[test]
    fn test_retry_delay_capped() {
        let config = RetryConfig {
            max_attempts: 10,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(500),
            multiplier: 2.0,
            jitter: false,
        };

        // After 3 attempts: 100 * 2^2 = 400ms
        // After 4 attempts: 100 * 2^3 = 800ms, but capped at 500ms
        assert_eq!(config.delay_for_attempt(4), Duration::from_millis(500));
    }

    #[test]
    fn test_retry_validation() {
        let mut config = RetryConfig::default();
        config.multiplier = 0.5;
        assert!(config.validate().is_err());

        let mut config = RetryConfig::default();
        config.initial_delay = Duration::from_secs(10);
        config.max_delay = Duration::from_secs(1);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_token_refresh_validation() {
        let mut config = TokenRefreshConfig::default();
        config.refresh_before_expiry = Duration::from_secs(1000);
        assert!(config.validate().is_err());
    }
}
