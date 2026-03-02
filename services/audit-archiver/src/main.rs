//! AgentAuth Audit Archiver Service
//!
//! One-shot job that manages audit log partition lifecycle:
//! 1. Creates future partitions (7 days in advance)
//! 2. Archives expired partitions to Parquet in cold storage
//! 3. Drops archived partitions from PostgreSQL
//!
//! Designed to run as a Kubernetes CronJob with leader election
//! via PostgreSQL advisory locks.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]

mod config;
mod error;
mod export;
mod leader;
mod parquet;
mod partition;
mod storage;

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::config::ArchiverConfig;
use crate::storage::ArchiveMetadata;

/// Pipeline outcome counters for the summary log line.
struct PipelineResult {
    partitions_created: u32,
    partitions_archived: u32,
    partitions_skipped: u32,
    total_rows_exported: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let config = ArchiverConfig::from_env().map_err(|e| {
        eprintln!("configuration error: {e}");
        e
    })?;

    init_tracing(&config.observability.log_level);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        service = "audit-archiver",
        "starting audit archiver"
    );

    let started = Instant::now();

    // Connect to PostgreSQL primary
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(
            config.database.connect_timeout_secs,
        ))
        .connect(&config.database.url)
        .await?;

    info!("connected to PostgreSQL");

    // Acquire advisory lock — exit cleanly if another instance is running
    let Some(lock) = leader::AdvisoryLock::try_acquire(&pool).await? else {
        info!("another archiver instance is running, exiting");
        return Ok(());
    };

    // Set up a shutdown signal listener for graceful SIGTERM handling
    let shutdown = tokio::sync::watch::channel(false);
    let shutdown_rx = shutdown.1.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown.0.send(true);
    });

    // Run the pipeline
    let result = run_pipeline(&config, &pool, &shutdown_rx).await;

    // Release the lock before exiting
    if let Err(e) = lock.release().await {
        error!(error = %e, "failed to release advisory lock");
    }

    pool.close().await;

    match result {
        Ok(outcome) => {
            info!(
                partitions_created = outcome.partitions_created,
                partitions_archived = outcome.partitions_archived,
                partitions_skipped = outcome.partitions_skipped,
                total_rows_exported = outcome.total_rows_exported,
                duration_secs = started.elapsed().as_secs_f64(),
                "archiver completed successfully"
            );
            Ok(())
        }
        Err(e) => {
            error!(
                error = %e,
                duration_secs = started.elapsed().as_secs_f64(),
                "archiver failed"
            );
            Err(e.into())
        }
    }
}

/// Runs the full archival pipeline.
async fn run_pipeline(
    config: &ArchiverConfig,
    pool: &sqlx::PgPool,
    shutdown: &tokio::sync::watch::Receiver<bool>,
) -> error::Result<PipelineResult> {
    let mut result = PipelineResult {
        partitions_created: 0,
        partitions_archived: 0,
        partitions_skipped: 0,
        total_rows_exported: 0,
    };

    // Step A: Ensure future partitions exist
    if *shutdown.borrow() {
        warn!("shutdown requested, skipping partition creation");
        return Ok(result);
    }

    let today = Utc::now().date_naive();
    ensure_future_partitions(pool, &config.retention, today, &mut result).await;

    // Step B: Archive expired partitions
    if *shutdown.borrow() {
        warn!("shutdown requested, skipping archival");
        return Ok(result);
    }

    archive_expired(config, pool, shutdown, today, &mut result).await?;

    Ok(result)
}

/// Creates partitions for the current and upcoming months.
async fn ensure_future_partitions(
    pool: &sqlx::PgPool,
    retention: &config::RetentionConfig,
    today: chrono::NaiveDate,
    result: &mut PipelineResult,
) {
    let needed = partition::partitions_to_create(today, retention.advance_partition_days);

    for (year, month) in &needed {
        match partition::create_partition(pool, *year, *month).await {
            Ok(true) => result.partitions_created += 1,
            Ok(false) => {} // Already exists
            Err(e) => {
                warn!(year, month, error = %e, "failed to create partition, continuing");
            }
        }
    }

    info!(
        created = result.partitions_created,
        checked = needed.len(),
        "partition creation step complete"
    );
}

/// Archives and drops partitions that have exceeded the hot retention window.
async fn archive_expired(
    config: &ArchiverConfig,
    pool: &sqlx::PgPool,
    shutdown: &tokio::sync::watch::Receiver<bool>,
    today: chrono::NaiveDate,
    result: &mut PipelineResult,
) -> error::Result<()> {
    let existing = partition::list_partitions(pool).await?;
    let to_archive =
        partition::partitions_to_archive(&existing, today, config.retention.hot_retention_days);

    if to_archive.is_empty() {
        info!("no partitions eligible for archival");
        return Ok(());
    }

    info!(count = to_archive.len(), "partitions eligible for archival");

    let cold_storage = storage::create_storage(&config.storage).await?;
    let schema = Arc::new(parquet::audit_events_schema());

    for partition_info in &to_archive {
        if *shutdown.borrow() {
            warn!(partition = %partition_info.name, "shutdown requested, stopping archival");
            break;
        }

        let key = storage::storage_key(&config.storage.s3_prefix, &partition_info.name);
        archive_single_partition(pool, &*cold_storage, &schema, partition_info, &key, result)
            .await;
    }

    Ok(())
}

/// Archives a single partition: export, compress, upload, then drop.
async fn archive_single_partition(
    pool: &sqlx::PgPool,
    cold_storage: &dyn storage::ColdStorage,
    schema: &Arc<arrow::datatypes::Schema>,
    partition_info: &partition::PartitionInfo,
    key: &str,
    result: &mut PipelineResult,
) {
    // Idempotency: skip if already archived
    match cold_storage.exists(key).await {
        Ok(true) => {
            info!(partition = %partition_info.name, key, "already archived, dropping partition");
            drop_partition_with_warning(pool, &partition_info.name).await;
            result.partitions_skipped += 1;
            return;
        }
        Ok(false) => {} // Proceed with export
        Err(e) => {
            warn!(partition = %partition_info.name, error = %e, "failed to check archive, skipping");
            return;
        }
    }

    // Export rows
    let batches = match export::export_partition(pool, &partition_info.name, 1000).await {
        Ok(b) => b,
        Err(e) => {
            error!(partition = %partition_info.name, error = %e, "failed to export partition");
            return;
        }
    };

    let row_count: u64 = batches.iter().map(|b| b.len() as u64).sum();

    if row_count == 0 {
        info!(partition = %partition_info.name, "partition is empty, dropping");
        drop_partition_with_warning(pool, &partition_info.name).await;
        result.partitions_archived += 1;
        return;
    }

    // Convert to Arrow and write Parquet
    let record_batches: Vec<_> = batches
        .iter()
        .filter_map(|rows| {
            parquet::rows_to_record_batch(rows, schema)
                .map_err(|e| error!(partition = %partition_info.name, error = %e, "record batch error"))
                .ok()
        })
        .collect();

    let parquet_bytes = match parquet::write_parquet(&record_batches, schema) {
        Ok(bytes) => bytes,
        Err(e) => {
            error!(partition = %partition_info.name, error = %e, "parquet write failed");
            return;
        }
    };

    let metadata = ArchiveMetadata {
        partition_name: partition_info.name.clone(),
        row_count,
    };

    // Upload and verify
    if let Err(e) = cold_storage.upload(key, parquet_bytes, &metadata).await {
        error!(partition = %partition_info.name, error = %e, "upload failed");
        return;
    }

    match cold_storage.exists(key).await {
        Ok(true) => {}
        Ok(false) => {
            error!(partition = %partition_info.name, key, "upload verification failed");
            return;
        }
        Err(e) => {
            error!(partition = %partition_info.name, error = %e, "verification check failed");
            return;
        }
    }

    // Drop the partition
    drop_partition_with_warning(pool, &partition_info.name).await;

    result.partitions_archived += 1;
    result.total_rows_exported += row_count;

    info!(partition = %partition_info.name, rows = row_count, "partition archived and dropped");
}

/// Detaches and drops a partition, logging a warning on failure.
async fn drop_partition_with_warning(pool: &sqlx::PgPool, name: &str) {
    if let Err(e) = partition::detach_partition(pool, name).await {
        warn!(partition = %name, error = %e, "failed to detach partition");
        return;
    }
    if let Err(e) = partition::drop_partition(pool, name).await {
        warn!(partition = %name, error = %e, "failed to drop partition");
    }
}

/// Initializes structured JSON tracing.
fn init_tracing(log_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(log_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .with_target(true)
        .with_thread_ids(false)
        .init();
}

/// Waits for a shutdown signal (Ctrl+C or SIGTERM).
#[allow(clippy::expect_used)] // Signal handler setup is infallible in practice;
                               // panicking here is appropriate since the process
                               // cannot function without signal handling.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => info!("received Ctrl+C"),
        () = terminate => info!("received SIGTERM"),
    }
}
