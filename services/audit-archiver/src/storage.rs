//! Cold storage backends for archived Parquet files.
//!
//! Provides a `ColdStorage` trait with two implementations:
//! - `S3Storage` for production (AWS S3, MinIO, or any S3-compatible API)
//! - `LocalFsStorage` for local development and testing

use std::path::PathBuf;

use bytes::Bytes;
use tracing::info;

use crate::config::{StorageBackend, StorageConfig};
use crate::error::{ArchiverError, Result};

/// Metadata attached to archived Parquet files.
#[derive(Debug, Clone)]
pub struct ArchiveMetadata {
    /// The partition name (e.g., `audit_events_2025_01`).
    pub partition_name: String,
    /// Number of rows archived.
    pub row_count: u64,
}

/// Trait for cold storage backends.
#[async_trait::async_trait]
pub trait ColdStorage: Send + Sync {
    /// Uploads a Parquet file to cold storage.
    ///
    /// # Errors
    ///
    /// Returns an error if the upload fails.
    async fn upload(&self, key: &str, data: Bytes, metadata: &ArchiveMetadata) -> Result<()>;

    /// Checks if a key already exists in cold storage (for idempotency).
    ///
    /// # Errors
    ///
    /// Returns an error if the check fails.
    async fn exists(&self, key: &str) -> Result<bool>;
}

/// Creates a `ColdStorage` implementation from configuration.
///
/// # Errors
///
/// Returns an error if the configuration is invalid.
pub async fn create_storage(config: &StorageConfig) -> Result<Box<dyn ColdStorage>> {
    match config.backend {
        StorageBackend::S3 => {
            let bucket = config
                .s3_bucket
                .as_deref()
                .ok_or_else(|| ArchiverError::Config("s3_bucket is required when backend=s3".into()))?;
            let storage = S3Storage::new(config, bucket).await?;
            Ok(Box::new(storage))
        }
        StorageBackend::LocalFs => {
            let path = config
                .local_path
                .as_deref()
                .ok_or_else(|| ArchiverError::Config("local_path is required when backend=local_fs".into()))?;
            let storage = LocalFsStorage::new(path)?;
            Ok(Box::new(storage))
        }
    }
}

/// Generates the storage key for a partition's Parquet file.
///
/// Uses Hive-style partitioning: `{prefix}year=YYYY/month=MM/audit_events_YYYY_MM.parquet`
#[must_use]
pub fn storage_key(prefix: &str, partition_name: &str) -> String {
    // Extract year and month from partition name (audit_events_YYYY_MM)
    let year = &partition_name[13..17];
    let month = &partition_name[18..20];
    format!("{prefix}year={year}/month={month}/{partition_name}.parquet")
}

// ── S3 Storage ────────────────────────────────────────────────────────────

/// S3-compatible cold storage (AWS S3, MinIO, GCS via S3 compatibility).
pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl S3Storage {
    /// Creates a new S3 storage backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the AWS SDK configuration fails.
    async fn new(config: &StorageConfig, bucket: &str) -> Result<Self> {
        let mut aws_config_builder = aws_config::from_env().region(
            aws_config::Region::new(config.s3_region.clone()),
        );

        // Override endpoint for MinIO or other S3-compatible services
        if let Some(ref endpoint) = config.s3_endpoint_url {
            aws_config_builder = aws_config_builder.endpoint_url(endpoint);
        }

        let aws_config = aws_config_builder.load().await;

        let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
            .force_path_style(true) // Required for MinIO
            .build();

        let client = aws_sdk_s3::Client::from_conf(s3_config);

        Ok(Self {
            client,
            bucket: bucket.to_string(),
        })
    }
}

#[async_trait::async_trait]
impl ColdStorage for S3Storage {
    async fn upload(&self, key: &str, data: Bytes, metadata: &ArchiveMetadata) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .content_type("application/vnd.apache.parquet")
            .metadata("partition_name", &metadata.partition_name)
            .metadata("row_count", metadata.row_count.to_string())
            .send()
            .await
            .map_err(|e| ArchiverError::Storage(format!("S3 upload failed: {e}")))?;

        info!(
            bucket = %self.bucket,
            key = %key,
            rows = metadata.row_count,
            "uploaded archive to S3"
        );
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(err) => {
                // NotFound means the key doesn't exist
                let is_not_found = err
                    .as_service_error()
                    .is_some_and(aws_sdk_s3::operation::head_object::HeadObjectError::is_not_found);
                if is_not_found {
                    Ok(false)
                } else {
                    Err(ArchiverError::Storage(format!("S3 head_object failed: {err}")))
                }
            }
        }
    }
}

// ── Local Filesystem Storage ──────────────────────────────────────────────

/// Local filesystem cold storage (development and testing only).
pub struct LocalFsStorage {
    base_path: PathBuf,
}

impl LocalFsStorage {
    /// Creates a new local filesystem storage backend.
    ///
    /// Creates the base directory if it doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    fn new(path: &str) -> Result<Self> {
        let base_path = PathBuf::from(path);
        std::fs::create_dir_all(&base_path)
            .map_err(|e| ArchiverError::Storage(format!("failed to create directory {path}: {e}")))?;
        Ok(Self { base_path })
    }
}

#[async_trait::async_trait]
impl ColdStorage for LocalFsStorage {
    async fn upload(&self, key: &str, data: Bytes, metadata: &ArchiveMetadata) -> Result<()> {
        let file_path = self.base_path.join(key);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ArchiverError::Storage(format!("failed to create dirs: {e}")))?;
        }

        tokio::fs::write(&file_path, &data)
            .await
            .map_err(|e| ArchiverError::Storage(format!("failed to write file: {e}")))?;

        info!(
            path = %file_path.display(),
            rows = metadata.row_count,
            size_bytes = data.len(),
            "wrote archive to local filesystem"
        );
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        let file_path = self.base_path.join(key);
        Ok(file_path.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_key_format() {
        let key = storage_key("audit-archive/", "audit_events_2025_01");
        assert_eq!(
            key,
            "audit-archive/year=2025/month=01/audit_events_2025_01.parquet"
        );
    }

    #[test]
    fn test_storage_key_december() {
        let key = storage_key("prefix/", "audit_events_2026_12");
        assert_eq!(
            key,
            "prefix/year=2026/month=12/audit_events_2026_12.parquet"
        );
    }

    #[tokio::test]
    async fn test_local_fs_upload_and_exists() {
        let tmp = tempfile::tempdir().expect("test: create tmpdir");
        let storage = LocalFsStorage::new(tmp.path().to_str().expect("test: path"))
            .expect("test: create storage");

        let key = "year=2025/month=01/test.parquet";
        let data = Bytes::from_static(b"test parquet data");
        let metadata = ArchiveMetadata {
            partition_name: "audit_events_2025_01".to_string(),
            row_count: 42,
        };

        assert!(!storage.exists(key).await.expect("test: exists check"));

        storage
            .upload(key, data, &metadata)
            .await
            .expect("test: upload");

        assert!(storage.exists(key).await.expect("test: exists check"));
    }
}
