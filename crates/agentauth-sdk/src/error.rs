//! Error types for the AgentAuth SDK.

use thiserror::Error;

/// Errors that can occur when using the AgentAuth SDK.
#[derive(Debug, Error)]
pub enum SdkError {
    /// The registry service returned an error.
    #[error("Registry error: {code}: {message}")]
    RegistryError {
        /// Error code from the registry.
        code: String,
        /// Human-readable error message.
        message: String,
        /// HTTP status code.
        status: u16,
    },

    /// The grant request was denied.
    #[error("Grant denied: {reason}")]
    GrantDenied {
        /// The reason the grant was denied.
        reason: String,
    },

    /// The grant request is pending approval.
    #[error("Grant pending approval: {grant_id}")]
    GrantPending {
        /// The grant ID that is pending.
        grant_id: String,
    },

    /// The grant has expired.
    #[error("Grant expired: {grant_id}")]
    GrantExpired {
        /// The grant ID that has expired.
        grant_id: String,
    },

    /// The agent has been revoked.
    #[error("Agent revoked: {agent_id}")]
    AgentRevoked {
        /// The agent ID that has been revoked.
        agent_id: String,
    },

    /// The token has expired and could not be refreshed.
    #[error("Token expired")]
    TokenExpired,

    /// Rate limit exceeded according to the behavioral envelope.
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// The operation is not allowed at this time (time window restriction).
    #[error("Operation not allowed at this time")]
    TimeWindowRestriction,

    /// Human confirmation is required for this operation.
    #[error("Human confirmation required")]
    HumanConfirmationRequired,

    /// Cryptographic operation failed.
    #[error("Crypto error: {0}")]
    CryptoError(String),

    /// Network error communicating with the registry.
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Connection timed out.
    #[error("Connection timeout")]
    Timeout,

    /// Invalid configuration.
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// The requested capability is not granted.
    #[error("Capability not granted: {0}")]
    CapabilityNotGranted(String),

    /// Internal SDK error.
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl SdkError {
    /// Returns true if this error is transient and the operation may succeed on retry.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            SdkError::NetworkError(_) | SdkError::Timeout => true,
            SdkError::RegistryError { status, .. } => is_transient_status(*status),
            _ => false,
        }
    }

    /// Returns true if this error indicates the operation should not be retried.
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        match self {
            SdkError::GrantDenied { .. }
            | SdkError::AgentRevoked { .. }
            | SdkError::ConfigError(_)
            | SdkError::CapabilityNotGranted(_) => true,
            SdkError::RegistryError { status, .. } => is_client_error(*status),
            _ => false,
        }
    }
}

/// Checks if an HTTP status code indicates a transient error.
fn is_transient_status(status: u16) -> bool {
    matches!(status, 502..=504)
}

/// Checks if an HTTP status code indicates a client error (non-retryable).
fn is_client_error(status: u16) -> bool {
    matches!(status, 400 | 401 | 403 | 404 | 409 | 422)
}

impl From<reqwest::Error> for SdkError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            SdkError::Timeout
        } else if err.is_connect() {
            SdkError::NetworkError(format!("Connection failed: {err}"))
        } else {
            SdkError::NetworkError(err.to_string())
        }
    }
}

impl From<serde_json::Error> for SdkError {
    fn from(err: serde_json::Error) -> Self {
        SdkError::SerializationError(err.to_string())
    }
}

impl From<agentauth_core::CoreError> for SdkError {
    fn from(err: agentauth_core::CoreError) -> Self {
        match err {
            agentauth_core::CoreError::Crypto(e) => SdkError::CryptoError(e.to_string()),
            _ => SdkError::InternalError(err.to_string()),
        }
    }
}

impl From<agentauth_core::error::CryptoError> for SdkError {
    fn from(err: agentauth_core::error::CryptoError) -> Self {
        SdkError::CryptoError(err.to_string())
    }
}

/// Result type for SDK operations.
pub type SdkResult<T> = Result<T, SdkError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transient_errors() {
        assert!(SdkError::NetworkError("test".to_string()).is_transient());
        assert!(SdkError::Timeout.is_transient());
        assert!(SdkError::RegistryError {
            code: "UNAVAILABLE".to_string(),
            message: "test".to_string(),
            status: 503,
        }
        .is_transient());
    }

    #[test]
    fn test_permanent_errors() {
        assert!(SdkError::GrantDenied {
            reason: "test".to_string()
        }
        .is_permanent());
        assert!(SdkError::AgentRevoked {
            agent_id: "test".to_string()
        }
        .is_permanent());
        assert!(SdkError::RegistryError {
            code: "NOT_FOUND".to_string(),
            message: "test".to_string(),
            status: 404,
        }
        .is_permanent());
    }

    #[test]
    fn test_non_transient_registry_errors() {
        // 400 is not transient
        assert!(!SdkError::RegistryError {
            code: "BAD_REQUEST".to_string(),
            message: "test".to_string(),
            status: 400,
        }
        .is_transient());
    }
}
