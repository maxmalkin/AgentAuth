//! Partition management for the `audit_events` table.
//!
//! Handles listing existing partitions, creating future partitions,
//! and detaching/dropping archived partitions.

use chrono::{Datelike, NaiveDate};
use sqlx::PgPool;
use tracing::{info, warn};

use crate::error::{ArchiverError, Result};

/// Information about an existing partition.
#[derive(Debug, Clone)]
pub struct PartitionInfo {
    /// The partition table name (e.g., `audit_events_2025_01`).
    pub name: String,
    /// The lower bound of the partition range (inclusive).
    /// Used by callers to determine partition age for retention decisions.
    #[allow(dead_code)]
    pub range_start: NaiveDate,
    /// The upper bound of the partition range (exclusive).
    pub range_end: NaiveDate,
}

/// Generates the partition table name for a given year and month.
///
/// Format: `audit_events_YYYY_MM`
#[must_use]
pub fn partition_name(year: i32, month: u32) -> String {
    format!("audit_events_{year:04}_{month:02}")
}

/// Validates that a partition name matches the expected format (public version).
///
/// # Errors
///
/// Returns an error if the name doesn't match `audit_events_YYYY_MM`.
pub fn validate_partition_name_public(name: &str) -> Result<()> {
    validate_partition_name(name)
}

/// Validates that a partition name matches the expected format.
/// Prevents SQL injection through generated identifiers.
fn validate_partition_name(name: &str) -> Result<()> {
    // Partition names are program-generated from chrono::NaiveDate, never from
    // user input. This validation is defense-in-depth against programming errors.
    let is_valid = name.len() == "audit_events_YYYY_MM".len()
        && name.starts_with("audit_events_")
        && name[13..17].chars().all(|c| c.is_ascii_digit())
        && name.as_bytes()[17] == b'_'
        && name[18..20].chars().all(|c| c.is_ascii_digit());

    if is_valid {
        Ok(())
    } else {
        Err(ArchiverError::InvalidPartitionName(name.to_string()))
    }
}

/// Lists all existing partitions of the `audit_events` table.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn list_partitions(pool: &PgPool) -> Result<Vec<PartitionInfo>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT child.relname AS partition_name \
         FROM pg_inherits \
         JOIN pg_class parent ON pg_inherits.inhparent = parent.oid \
         JOIN pg_class child ON pg_inherits.inhrelid = child.oid \
         WHERE parent.relname = 'audit_events' \
         ORDER BY child.relname",
    )
    .fetch_all(pool)
    .await?;

    let mut partitions = Vec::with_capacity(rows.len());
    for (name,) in rows {
        if let Some(info) = parse_partition_name(&name) {
            partitions.push(info);
        } else {
            warn!(partition = %name, "skipping partition with unrecognized name format");
        }
    }

    Ok(partitions)
}

/// Parses a partition name into a `PartitionInfo` with date range.
fn parse_partition_name(name: &str) -> Option<PartitionInfo> {
    // Expected format: audit_events_YYYY_MM
    if name.len() != 20 || !name.starts_with("audit_events_") {
        return None;
    }

    let year: i32 = name[13..17].parse().ok()?;
    let month: u32 = name[18..20].parse().ok()?;

    let range_start = NaiveDate::from_ymd_opt(year, month, 1)?;
    // Next month's first day
    let range_end = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };

    Some(PartitionInfo {
        name: name.to_string(),
        range_start,
        range_end,
    })
}

/// Creates a new monthly partition if it does not already exist.
///
/// # Safety (SQL injection)
///
/// The partition name and date boundaries are generated from `chrono::NaiveDate`
/// values, never from user input. The name is validated against a strict regex
/// pattern before use. SQLx does not support parameterized identifiers in DDL,
/// so `format!` is used here — this is safe because the inputs are trusted
/// program-generated values.
///
/// # Errors
///
/// Returns an error if the partition name is invalid or the query fails.
pub async fn create_partition(pool: &PgPool, year: i32, month: u32) -> Result<bool> {
    let name = partition_name(year, month);
    validate_partition_name(&name)?;

    let range_start = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| ArchiverError::InvalidPartitionName(name.clone()))?;
    let range_end = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(|| ArchiverError::InvalidPartitionName(name.clone()))?;

    // Check if partition already exists
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_class WHERE relname = $1 AND relkind = 'r')",
    )
    .bind(&name)
    .fetch_one(pool)
    .await?;

    if exists.0 {
        return Ok(false);
    }

    // DDL: partition name and date boundaries are program-generated, not user input.
    let create_sql = format!(
        "CREATE TABLE {name} PARTITION OF audit_events \
         FOR VALUES FROM ('{range_start}') TO ('{range_end}')"
    );
    sqlx::query(&create_sql).execute(pool).await?;

    // Grant same permissions as parent table
    let grant_sql = format!(
        "GRANT SELECT, INSERT ON {name} TO agentauth_service"
    );
    sqlx::query(&grant_sql).execute(pool).await?;

    let revoke_sql = format!(
        "REVOKE UPDATE, DELETE ON {name} FROM agentauth_service"
    );
    sqlx::query(&revoke_sql).execute(pool).await?;

    info!(partition = %name, start = %range_start, end = %range_end, "created partition");
    Ok(true)
}

/// Detaches a partition from the parent table without blocking writes.
///
/// Uses `CONCURRENTLY` to avoid holding an ACCESS EXCLUSIVE lock on the
/// parent `audit_events` table.
///
/// # Errors
///
/// Returns an error if the partition name is invalid or the query fails.
pub async fn detach_partition(pool: &PgPool, name: &str) -> Result<()> {
    validate_partition_name(name)?;

    // DDL: partition name is validated against strict format above.
    let sql = format!("ALTER TABLE audit_events DETACH PARTITION {name} CONCURRENTLY");
    sqlx::query(&sql).execute(pool).await?;

    info!(partition = %name, "detached partition from parent table");
    Ok(())
}

/// Drops a previously detached partition table.
///
/// # Errors
///
/// Returns an error if the partition name is invalid or the query fails.
pub async fn drop_partition(pool: &PgPool, name: &str) -> Result<()> {
    validate_partition_name(name)?;

    // DDL: partition name is validated against strict format above.
    let sql = format!("DROP TABLE IF EXISTS {name}");
    sqlx::query(&sql).execute(pool).await?;

    info!(partition = %name, "dropped partition table");
    Ok(())
}

/// Counts the number of rows in a partition.
///
/// # Errors
///
/// Returns an error if the partition name is invalid or the query fails.
#[allow(dead_code)] // Utility for diagnostics and future use
pub async fn count_rows(pool: &PgPool, name: &str) -> Result<i64> {
    validate_partition_name(name)?;

    // DDL: partition name is validated against strict format above.
    let sql = format!("SELECT COUNT(*) FROM {name}");
    let row: (i64,) = sqlx::query_as(&sql).fetch_one(pool).await?;
    Ok(row.0)
}

/// Determines which partitions need to be created based on the current date
/// and the advance window.
#[must_use]
pub fn partitions_to_create(today: NaiveDate, advance_days: u32) -> Vec<(i32, u32)> {
    let target_date = today + chrono::Duration::days(i64::from(advance_days));
    let mut result = Vec::new();

    let mut year = today.year();
    let mut month = today.month();

    loop {
        result.push((year, month));

        // Advance to next month
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }

        let month_start = NaiveDate::from_ymd_opt(year, month, 1);
        match month_start {
            Some(d) if d <= target_date => {}
            _ => break,
        }
    }

    // Include the month that contains target_date
    let target_month_start = NaiveDate::from_ymd_opt(target_date.year(), target_date.month(), 1);
    if let Some(tms) = target_month_start {
        let entry = (tms.year(), tms.month());
        if !result.contains(&entry) {
            result.push(entry);
        }
    }

    result
}

/// Determines which partitions are eligible for archival based on retention policy.
#[must_use]
pub fn partitions_to_archive(
    partitions: &[PartitionInfo],
    today: NaiveDate,
    hot_retention_days: u32,
) -> Vec<PartitionInfo> {
    let cutoff = today - chrono::Duration::days(i64::from(hot_retention_days));
    partitions
        .iter()
        .filter(|p| p.range_end <= cutoff)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_partition_name_generation() {
        assert_eq!(partition_name(2025, 1), "audit_events_2025_01");
        assert_eq!(partition_name(2026, 12), "audit_events_2026_12");
    }

    #[test]
    fn test_validate_partition_name_valid() {
        assert!(validate_partition_name("audit_events_2025_01").is_ok());
        assert!(validate_partition_name("audit_events_2026_12").is_ok());
    }

    #[test]
    fn test_validate_partition_name_invalid() {
        assert!(validate_partition_name("audit_events_202_01").is_err());
        assert!(validate_partition_name("other_table_2025_01").is_err());
        assert!(validate_partition_name("audit_events_2025_1").is_err());
        assert!(validate_partition_name("audit_events_abcd_ef").is_err());
        assert!(validate_partition_name("").is_err());
        assert!(validate_partition_name("audit_events_2025_01; DROP TABLE--").is_err());
    }

    #[test]
    fn test_parse_partition_name() {
        let info = parse_partition_name("audit_events_2025_01");
        assert!(info.is_some());
        let info = info.expect("test: known valid");
        assert_eq!(info.range_start, NaiveDate::from_ymd_opt(2025, 1, 1).expect("test"));
        assert_eq!(info.range_end, NaiveDate::from_ymd_opt(2025, 2, 1).expect("test"));
    }

    #[test]
    fn test_parse_partition_name_december() {
        let info = parse_partition_name("audit_events_2025_12");
        assert!(info.is_some());
        let info = info.expect("test: known valid");
        assert_eq!(info.range_end, NaiveDate::from_ymd_opt(2026, 1, 1).expect("test"));
    }

    #[test]
    fn test_partitions_to_create() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 25).expect("test");
        let result = partitions_to_create(today, 7);
        // Feb 25 + 7 = Mar 4, so we need Feb and Mar
        assert!(result.contains(&(2026, 2)));
        assert!(result.contains(&(2026, 3)));
    }

    #[test]
    fn test_partitions_to_create_month_boundary() {
        let today = NaiveDate::from_ymd_opt(2026, 12, 28).expect("test");
        let result = partitions_to_create(today, 7);
        // Dec 28 + 7 = Jan 4, so we need Dec and next Jan
        assert!(result.contains(&(2026, 12)));
        assert!(result.contains(&(2027, 1)));
    }

    #[test]
    fn test_partitions_to_archive() {
        let partitions = vec![
            PartitionInfo {
                name: "audit_events_2025_01".to_string(),
                range_start: NaiveDate::from_ymd_opt(2025, 1, 1).expect("test"),
                range_end: NaiveDate::from_ymd_opt(2025, 2, 1).expect("test"),
            },
            PartitionInfo {
                name: "audit_events_2026_02".to_string(),
                range_start: NaiveDate::from_ymd_opt(2026, 2, 1).expect("test"),
                range_end: NaiveDate::from_ymd_opt(2026, 3, 1).expect("test"),
            },
        ];

        let today = NaiveDate::from_ymd_opt(2026, 3, 1).expect("test");
        let to_archive = partitions_to_archive(&partitions, today, 90);

        // 2025_01 ended Feb 1, which is > 90 days before Mar 1 2026
        assert_eq!(to_archive.len(), 1);
        assert_eq!(to_archive[0].name, "audit_events_2025_01");
    }
}
