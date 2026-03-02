//! PostgreSQL advisory lock for leader election.
//!
//! Only one archiver instance should run at a time. We use a PostgreSQL
//! advisory lock to ensure this without external coordination.

use sqlx::PgPool;
use tracing::{info, warn};

use crate::error::Result;

/// Fixed advisory lock ID derived from "AUDITA" (0x41_55_44_49_54_41).
/// This avoids collision with other advisory lock users in the same database.
const LOCK_ID: i64 = 0x0041_5544_4954_4100;

/// A held advisory lock that releases on drop (via explicit `release()`).
pub struct AdvisoryLock<'a> {
    pool: &'a PgPool,
    held: bool,
}

impl<'a> AdvisoryLock<'a> {
    /// Attempts to acquire the archiver advisory lock.
    ///
    /// Returns `Ok(Some(lock))` if acquired, `Ok(None)` if another instance
    /// holds it. Does not block.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn try_acquire(pool: &'a PgPool) -> Result<Option<Self>> {
        let row: (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)")
            .bind(LOCK_ID)
            .fetch_one(pool)
            .await?;

        if row.0 {
            info!(lock_id = LOCK_ID, "acquired advisory lock");
            Ok(Some(Self { pool, held: true }))
        } else {
            warn!(lock_id = LOCK_ID, "another archiver instance holds the lock");
            Ok(None)
        }
    }

    /// Releases the advisory lock.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn release(mut self) -> Result<()> {
        self.release_inner().await
    }

    async fn release_inner(&mut self) -> Result<()> {
        if self.held {
            let _: (bool,) = sqlx::query_as("SELECT pg_advisory_unlock($1)")
                .bind(LOCK_ID)
                .fetch_one(self.pool)
                .await?;
            self.held = false;
            info!(lock_id = LOCK_ID, "released advisory lock");
        }
        Ok(())
    }
}

impl Drop for AdvisoryLock<'_> {
    fn drop(&mut self) {
        if self.held {
            // Best-effort warning — the lock will be released when the
            // connection/session closes anyway, but explicit release is preferred.
            warn!("advisory lock dropped without explicit release");
        }
    }
}

/// Returns the advisory lock ID used by the archiver (for testing/logging).
#[must_use]
#[allow(dead_code)] // Used in tests
pub const fn lock_id() -> i64 {
    LOCK_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lock_id_is_stable() {
        assert_eq!(lock_id(), 0x0041_5544_4954_4100);
    }
}
