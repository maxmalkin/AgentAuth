//! Error types for agentauth-core.

use thiserror::Error;

/// Core library errors.
#[derive(Debug, Error)]
pub enum CoreError {
    /// Cryptographic operation failed.
    #[error("cryptographic error: {0}")]
    Crypto(#[from] CryptoError),

    /// Serialization/deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Validation failed.
    #[error("validation error: {0}")]
    Validation(String),

    /// Invalid capability specification.
    #[error("invalid capability: {0}")]
    InvalidCapability(String),

    /// Invalid behavioral envelope.
    #[error("invalid behavioral envelope: {0}")]
    InvalidEnvelope(String),

    /// Token has expired.
    #[error("token expired at {0}")]
    TokenExpired(chrono::DateTime<chrono::Utc>),

    /// Token signature verification failed.
    #[error("token signature verification failed")]
    InvalidSignature,

    /// Key not found.
    #[error("key not found: {0}")]
    KeyNotFound(String),
}

/// Cryptographic errors.
#[derive(Debug, Error)]
pub enum CryptoError {
    /// Signing operation failed.
    #[error("signing failed: {0}")]
    SigningFailed(String),

    /// Verification failed.
    #[error("verification failed: {0}")]
    VerificationFailed(String),

    /// Key generation failed.
    #[error("key generation failed: {0}")]
    KeyGenerationFailed(String),

    /// Invalid key format.
    #[error("invalid key format: {0}")]
    InvalidKeyFormat(String),

    /// KMS operation failed.
    #[error("KMS operation failed: {0}")]
    KmsError(String),

    /// Hash chain integrity violation.
    #[error("hash chain integrity violation: expected {expected}, got {actual}")]
    HashChainViolation {
        /// Expected hash.
        expected: String,
        /// Actual hash.
        actual: String,
    },
}
