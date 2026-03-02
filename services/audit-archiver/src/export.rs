//! Export audit event rows from a PostgreSQL partition.
//!
//! Streams rows from a partition table and converts them into a flat
//! struct suitable for Arrow/Parquet conversion.

use sqlx::PgPool;
use tracing::info;

use crate::error::Result;
use crate::partition;

/// A flat representation of an audit event row, with all types converted
/// to Parquet-friendly formats (strings for UUIDs, microseconds for timestamps).
#[derive(Debug, Clone)]
pub struct AuditRow {
    /// Event ID (UUID as hyphenated string).
    pub id: String,
    /// Event type (enum cast to text).
    pub event_type: String,
    /// Agent ID (nullable UUID string).
    pub agent_id: Option<String>,
    /// Service provider ID (nullable UUID string).
    pub service_provider_id: Option<String>,
    /// Human principal ID (nullable UUID string).
    pub human_principal_id: Option<String>,
    /// Grant ID (nullable UUID string).
    pub grant_id: Option<String>,
    /// Token JTI (nullable UUID string).
    pub token_jti: Option<String>,
    /// Event data (JSONB serialized to string).
    pub event_data: String,
    /// Outcome (e.g., "success", "error").
    pub outcome: String,
    /// Error message (nullable).
    pub error_message: Option<String>,
    /// Source IP (INET cast to text, nullable).
    pub source_ip: Option<String>,
    /// User agent string (nullable).
    pub user_agent: Option<String>,
    /// Request ID (nullable UUID string).
    pub request_id: Option<String>,
    /// Trace ID (nullable).
    pub trace_id: Option<String>,
    /// Previous event hash (32 bytes).
    pub previous_event_hash: Vec<u8>,
    /// Row hash (32 bytes).
    pub row_hash: Vec<u8>,
    /// Registry signature (64 bytes).
    pub registry_signature: Vec<u8>,
    /// Created at timestamp as microseconds since Unix epoch (UTC).
    pub created_at_micros: i64,
}

/// Intermediate row type for sqlx deserialization.
#[derive(sqlx::FromRow)]
struct RawAuditRow {
    id: uuid::Uuid,
    event_type: String,
    agent_id: Option<uuid::Uuid>,
    service_provider_id: Option<uuid::Uuid>,
    human_principal_id: Option<uuid::Uuid>,
    grant_id: Option<uuid::Uuid>,
    token_jti: Option<uuid::Uuid>,
    event_data: serde_json::Value,
    outcome: String,
    error_message: Option<String>,
    source_ip: Option<String>,
    user_agent: Option<String>,
    request_id: Option<uuid::Uuid>,
    trace_id: Option<String>,
    previous_event_hash: Vec<u8>,
    row_hash: Vec<u8>,
    registry_signature: Vec<u8>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<RawAuditRow> for AuditRow {
    fn from(raw: RawAuditRow) -> Self {
        Self {
            id: raw.id.to_string(),
            event_type: raw.event_type,
            agent_id: raw.agent_id.map(|u| u.to_string()),
            service_provider_id: raw.service_provider_id.map(|u| u.to_string()),
            human_principal_id: raw.human_principal_id.map(|u| u.to_string()),
            grant_id: raw.grant_id.map(|u| u.to_string()),
            token_jti: raw.token_jti.map(|u| u.to_string()),
            event_data: raw.event_data.to_string(),
            outcome: raw.outcome,
            error_message: raw.error_message,
            source_ip: raw.source_ip,
            user_agent: raw.user_agent,
            request_id: raw.request_id.map(|u| u.to_string()),
            trace_id: raw.trace_id,
            previous_event_hash: raw.previous_event_hash,
            row_hash: raw.row_hash,
            registry_signature: raw.registry_signature,
            created_at_micros: raw.created_at.timestamp_micros(),
        }
    }
}

/// Exports all rows from a partition in batches.
///
/// Returns batches of `AuditRow` suitable for Parquet conversion.
/// Each batch contains up to `batch_size` rows.
///
/// # Errors
///
/// Returns an error if the partition name is invalid or the query fails.
pub async fn export_partition(
    pool: &PgPool,
    partition_name: &str,
    batch_size: usize,
) -> Result<Vec<Vec<AuditRow>>> {
    // Validate partition name before using in SQL
    partition::validate_partition_name_public(partition_name)?;

    // DDL: partition name is validated against strict format above, not user input.
    let sql = format!(
        "SELECT id, event_type::text, agent_id, service_provider_id, human_principal_id, \
         grant_id, token_jti, event_data, outcome, error_message, \
         source_ip::text, user_agent, request_id, trace_id, \
         previous_event_hash, row_hash, registry_signature, created_at \
         FROM {partition_name} ORDER BY created_at ASC"
    );

    let raw_rows: Vec<RawAuditRow> = sqlx::query_as(&sql).fetch_all(pool).await?;

    let total = raw_rows.len();
    info!(
        partition = %partition_name,
        rows = total,
        "exported rows from partition"
    );

    let batches: Vec<Vec<AuditRow>> = raw_rows
        .into_iter()
        .map(AuditRow::from)
        .collect::<Vec<_>>()
        .chunks(batch_size)
        .map(<[AuditRow]>::to_vec)
        .collect();

    Ok(batches)
}
