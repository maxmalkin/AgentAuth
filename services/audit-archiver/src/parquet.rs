//! Arrow schema definition and Parquet writer for audit events.

use std::sync::Arc;

use arrow::array::{
    BinaryBuilder, RecordBatch, StringBuilder, TimestampMicrosecondBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use bytes::Bytes;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::error::Result;
use crate::export::AuditRow;

/// Returns the Arrow schema for audit events.
#[must_use]
pub fn audit_events_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("event_type", DataType::Utf8, false),
        Field::new("agent_id", DataType::Utf8, true),
        Field::new("service_provider_id", DataType::Utf8, true),
        Field::new("human_principal_id", DataType::Utf8, true),
        Field::new("grant_id", DataType::Utf8, true),
        Field::new("token_jti", DataType::Utf8, true),
        Field::new("event_data", DataType::Utf8, false),
        Field::new("outcome", DataType::Utf8, false),
        Field::new("error_message", DataType::Utf8, true),
        Field::new("source_ip", DataType::Utf8, true),
        Field::new("user_agent", DataType::Utf8, true),
        Field::new("request_id", DataType::Utf8, true),
        Field::new("trace_id", DataType::Utf8, true),
        Field::new("previous_event_hash", DataType::Binary, false),
        Field::new("row_hash", DataType::Binary, false),
        Field::new("registry_signature", DataType::Binary, false),
        Field::new(
            "created_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ])
}

/// Converts a batch of `AuditRow`s into an Arrow `RecordBatch`.
///
/// # Errors
///
/// Returns an error if the batch cannot be built.
pub fn rows_to_record_batch(rows: &[AuditRow], schema: &Arc<Schema>) -> Result<RecordBatch> {
    let len = rows.len();

    let mut id_builder = StringBuilder::with_capacity(len, len * 36);
    let mut event_type_builder = StringBuilder::with_capacity(len, len * 20);
    let mut agent_id_builder = StringBuilder::with_capacity(len, len * 36);
    let mut sp_id_builder = StringBuilder::with_capacity(len, len * 36);
    let mut hp_id_builder = StringBuilder::with_capacity(len, len * 36);
    let mut grant_id_builder = StringBuilder::with_capacity(len, len * 36);
    let mut token_jti_builder = StringBuilder::with_capacity(len, len * 36);
    let mut event_data_builder = StringBuilder::with_capacity(len, len * 100);
    let mut outcome_builder = StringBuilder::with_capacity(len, len * 10);
    let mut error_msg_builder = StringBuilder::with_capacity(len, len * 50);
    let mut source_ip_builder = StringBuilder::with_capacity(len, len * 15);
    let mut user_agent_builder = StringBuilder::with_capacity(len, len * 50);
    let mut request_id_builder = StringBuilder::with_capacity(len, len * 36);
    let mut trace_id_builder = StringBuilder::with_capacity(len, len * 32);
    let mut prev_hash_builder = BinaryBuilder::with_capacity(len, len * 32);
    let mut row_hash_builder = BinaryBuilder::with_capacity(len, len * 32);
    let mut sig_builder = BinaryBuilder::with_capacity(len, len * 64);
    let mut created_at_builder = TimestampMicrosecondBuilder::with_capacity(len);

    for row in rows {
        id_builder.append_value(&row.id);
        event_type_builder.append_value(&row.event_type);
        append_optional_string(&mut agent_id_builder, row.agent_id.as_deref());
        append_optional_string(&mut sp_id_builder, row.service_provider_id.as_deref());
        append_optional_string(&mut hp_id_builder, row.human_principal_id.as_deref());
        append_optional_string(&mut grant_id_builder, row.grant_id.as_deref());
        append_optional_string(&mut token_jti_builder, row.token_jti.as_deref());
        event_data_builder.append_value(&row.event_data);
        outcome_builder.append_value(&row.outcome);
        append_optional_string(&mut error_msg_builder, row.error_message.as_deref());
        append_optional_string(&mut source_ip_builder, row.source_ip.as_deref());
        append_optional_string(&mut user_agent_builder, row.user_agent.as_deref());
        append_optional_string(&mut request_id_builder, row.request_id.as_deref());
        append_optional_string(&mut trace_id_builder, row.trace_id.as_deref());
        prev_hash_builder.append_value(&row.previous_event_hash);
        row_hash_builder.append_value(&row.row_hash);
        sig_builder.append_value(&row.registry_signature);
        created_at_builder.append_value(row.created_at_micros);
    }

    let batch = RecordBatch::try_new(
        Arc::clone(schema),
        vec![
            Arc::new(id_builder.finish()),
            Arc::new(event_type_builder.finish()),
            Arc::new(agent_id_builder.finish()),
            Arc::new(sp_id_builder.finish()),
            Arc::new(hp_id_builder.finish()),
            Arc::new(grant_id_builder.finish()),
            Arc::new(token_jti_builder.finish()),
            Arc::new(event_data_builder.finish()),
            Arc::new(outcome_builder.finish()),
            Arc::new(error_msg_builder.finish()),
            Arc::new(source_ip_builder.finish()),
            Arc::new(user_agent_builder.finish()),
            Arc::new(request_id_builder.finish()),
            Arc::new(trace_id_builder.finish()),
            Arc::new(prev_hash_builder.finish()),
            Arc::new(row_hash_builder.finish()),
            Arc::new(sig_builder.finish()),
            Arc::new(created_at_builder.finish().with_timezone("UTC")),
        ],
    )?;

    Ok(batch)
}

fn append_optional_string(builder: &mut StringBuilder, value: Option<&str>) {
    match value {
        Some(v) => builder.append_value(v),
        None => builder.append_null(),
    }
}

/// Writes record batches to Parquet format in memory and returns the bytes.
///
/// Uses ZSTD compression and a row group size of 65536.
///
/// # Errors
///
/// Returns an error if the Parquet writer fails.
pub fn write_parquet(batches: &[RecordBatch], schema: &Arc<Schema>) -> Result<Bytes> {
    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(parquet::basic::ZstdLevel::default()))
        .set_max_row_group_size(65536)
        .set_created_by("agentauth-audit-archiver".to_string())
        .build();

    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, Arc::clone(schema), Some(props))?;

    for batch in batches {
        writer.write(batch)?;
    }

    writer.close()?;
    Ok(Bytes::from(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_18_fields() {
        let schema = audit_events_schema();
        assert_eq!(schema.fields().len(), 18);
    }

    #[test]
    fn test_schema_id_is_not_nullable() {
        let schema = audit_events_schema();
        let id_field = schema.field_with_name("id").expect("test: id field exists");
        assert!(!id_field.is_nullable());
    }

    #[test]
    fn test_schema_agent_id_is_nullable() {
        let schema = audit_events_schema();
        let field = schema.field_with_name("agent_id").expect("test: field exists");
        assert!(field.is_nullable());
    }

    #[test]
    fn test_roundtrip_parquet() {
        let schema = Arc::new(audit_events_schema());
        let rows = vec![AuditRow {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            event_type: "token_issued".to_string(),
            agent_id: Some("agent-1".to_string()),
            service_provider_id: None,
            human_principal_id: None,
            grant_id: None,
            token_jti: None,
            event_data: "{}".to_string(),
            outcome: "success".to_string(),
            error_message: None,
            source_ip: None,
            user_agent: None,
            request_id: None,
            trace_id: None,
            previous_event_hash: vec![0u8; 32],
            row_hash: vec![1u8; 32],
            registry_signature: vec![2u8; 64],
            created_at_micros: 1_700_000_000_000_000,
        }];

        let batch = rows_to_record_batch(&rows, &schema).expect("test: batch creation");
        assert_eq!(batch.num_rows(), 1);

        let bytes = write_parquet(&[batch], &schema).expect("test: parquet write");
        assert!(!bytes.is_empty());

        // Verify we can read it back
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(
            bytes::Bytes::from(bytes.to_vec()),
            1024,
        )
        .expect("test: parquet reader");

        let batches: Vec<_> = reader.into_iter().collect::<std::result::Result<Vec<_>, _>>().expect("test: read batches");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 1);
    }
}
