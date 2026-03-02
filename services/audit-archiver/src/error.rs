//! Error types for the audit archiver service.

/// Errors that can occur during audit archival operations.
#[derive(Debug, thiserror::Error)]
pub enum ArchiverError {
    /// Database error during partition management or data export.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Failed to serialize audit events to Parquet format.
    #[error("parquet serialization error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    /// Failed to build Arrow record batch from audit rows.
    #[error("arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// Cold storage upload or verification failed.
    #[error("storage error: {0}")]
    Storage(String),

    /// Configuration is invalid or missing required fields.
    #[error("configuration error: {0}")]
    Config(String),

    /// The generated partition name failed validation.
    #[error("invalid partition name: {0}")]
    InvalidPartitionName(String),
}

/// Result alias for archiver operations.
pub type Result<T> = std::result::Result<T, ArchiverError>;
