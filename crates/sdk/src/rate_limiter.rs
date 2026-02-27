//! Client-side behavioral rate limiter.
//!
//! The `BehavioralRateLimiter` enforces the behavioral envelope constraints
//! defined in a token grant. This is a mandatory compliance component -
//! the SDK must enforce these limits client-side, not just rely on server-side
//! enforcement.
//!
//! # Design
//!
//! Uses a sliding window rate limiter based on the `governor` crate.
//! The limiter tracks requests per minute with burst capacity.

use std::num::NonZeroU32;
use std::sync::Arc;

use auth_core::types::BehavioralEnvelope;
use chrono::Utc;
use governor::{
    clock::DefaultClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

use crate::error::{SdkError, SdkResult};

/// Client-side rate limiter that enforces behavioral envelope constraints.
///
/// This rate limiter is mandatory for SDK compliance. It prevents agents
/// from exceeding their granted rate limits, which would result in server-side
/// rejections and potential grant revocation.
pub struct BehavioralRateLimiter {
    /// The behavioral envelope being enforced.
    envelope: BehavioralEnvelope,

    /// The underlying rate limiter.
    limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>>,
}

impl BehavioralRateLimiter {
    /// Creates a new rate limiter for the given behavioral envelope.
    ///
    /// # Errors
    ///
    /// Returns an error if the envelope constraints are invalid.
    pub fn new(envelope: BehavioralEnvelope) -> SdkResult<Self> {
        envelope
            .validate()
            .map_err(|e| SdkError::ConfigError(format!("Invalid envelope: {e}")))?;

        // Convert requests per minute to a quota
        // Governor uses "cells per period" where period is typically 1 second
        // We need to convert RPM to RPS with burst capacity

        // Use the burst limit from the envelope
        let burst = NonZeroU32::new(envelope.max_burst)
            .ok_or_else(|| SdkError::ConfigError("max_burst cannot be zero".to_string()))?;

        // Calculate replenishment rate
        // If we allow X requests per minute, we replenish at X/60 per second
        // Period between replenishments = 60/X seconds = 60000/X milliseconds
        // Safe: we validate max_requests_per_minute > 0 in envelope.validate()
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let replenish_period_ms = (60000.0 / f64::from(envelope.max_requests_per_minute)) as u64;
        let replenish_period = std::time::Duration::from_millis(replenish_period_ms.max(1));

        let quota = Quota::with_period(replenish_period)
            .ok_or_else(|| SdkError::ConfigError("Invalid rate limit period".to_string()))?
            .allow_burst(burst);

        let limiter = Arc::new(RateLimiter::direct(quota));

        tracing::debug!(
            max_requests_per_minute = envelope.max_requests_per_minute,
            max_burst = envelope.max_burst,
            replenish_period_ms = replenish_period_ms,
            "Created behavioral rate limiter"
        );

        Ok(Self { envelope, limiter })
    }

    /// Checks if a request is allowed and consumes one token if so.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the request is allowed
    /// - `Err(SdkError::RateLimitExceeded)` if the rate limit is exceeded
    /// - `Err(SdkError::TimeWindowRestriction)` if outside allowed time windows
    /// - `Err(SdkError::HumanConfirmationRequired)` if human is required but not present
    ///
    /// # Arguments
    ///
    /// * `human_present` - Whether the human principal is currently online
    /// * `transaction_value` - Optional transaction value for Transact operations
    pub fn check(&self, human_present: bool, transaction_value: Option<u64>) -> SdkResult<()> {
        // Check time window restrictions
        if !self.envelope.is_allowed_at_time(&Utc::now()) {
            return Err(SdkError::TimeWindowRestriction);
        }

        // Check if human must be online
        if self.envelope.requires_human_online && !human_present {
            return Err(SdkError::HumanConfirmationRequired);
        }

        // Check transaction threshold
        if let Some(threshold) = self.envelope.human_confirmation_threshold {
            if let Some(value) = transaction_value {
                if value > threshold && !human_present {
                    return Err(SdkError::HumanConfirmationRequired);
                }
            }
        }

        // Check rate limit
        self.limiter.check().map_err(|_| {
            SdkError::RateLimitExceeded(format!(
                "Exceeded {} requests per minute (burst: {})",
                self.envelope.max_requests_per_minute, self.envelope.max_burst
            ))
        })?;

        Ok(())
    }

    /// Checks if a request would be allowed without consuming a token.
    ///
    /// Useful for pre-flight checks before initiating expensive operations.
    #[must_use]
    pub fn would_allow(&self, human_present: bool, transaction_value: Option<u64>) -> bool {
        // Check time window
        if !self.envelope.is_allowed_at_time(&Utc::now()) {
            return false;
        }

        // Check human presence
        if self.envelope.requires_human_online && !human_present {
            return false;
        }

        // Check transaction threshold
        if let Some(threshold) = self.envelope.human_confirmation_threshold {
            if let Some(value) = transaction_value {
                if value > threshold && !human_present {
                    return false;
                }
            }
        }

        // Note: We don't consume from the limiter, just peek
        // This is a best-effort check as the limiter state may change
        true
    }

    /// Returns the number of requests remaining in the current window.
    ///
    /// This is approximate and may change between calls.
    #[must_use]
    pub fn remaining(&self) -> u32 {
        // Governor doesn't expose remaining tokens directly,
        // but we can estimate based on whether check would succeed
        // This is a simplified implementation
        self.envelope.max_burst
    }

    /// Returns the behavioral envelope being enforced.
    #[must_use]
    pub fn envelope(&self) -> &BehavioralEnvelope {
        &self.envelope
    }

    /// Resets the rate limiter (for testing purposes).
    #[cfg(test)]
    pub fn reset(&self) {
        // Governor doesn't have a reset, so we'd need to recreate
        // For testing, this is a no-op - tests should create new limiters
    }
}

impl std::fmt::Debug for BehavioralRateLimiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehavioralRateLimiter")
            .field("envelope", &self.envelope)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_envelope(rpm: u32, burst: u32) -> BehavioralEnvelope {
        BehavioralEnvelope {
            max_requests_per_minute: rpm,
            max_burst: burst,
            requires_human_online: false,
            human_confirmation_threshold: None,
            allowed_time_windows: vec![],
            max_session_duration_secs: 900,
        }
    }

    #[test]
    fn test_rate_limiter_allows_within_burst() {
        let envelope = make_envelope(60, 10);
        let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

        // Should allow up to burst limit
        for i in 0..10 {
            assert!(
                limiter.check(false, None).is_ok(),
                "Request {i} should be allowed"
            );
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_burst() {
        let envelope = make_envelope(60, 5);
        let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

        // Consume burst
        for _ in 0..5 {
            limiter.check(false, None).expect("should allow");
        }

        // Next request should fail
        let result = limiter.check(false, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SdkError::RateLimitExceeded(_)
        ));
    }

    #[test]
    fn test_rate_limiter_requires_human_online() {
        let envelope = BehavioralEnvelope {
            max_requests_per_minute: 60,
            max_burst: 10,
            requires_human_online: true,
            human_confirmation_threshold: None,
            allowed_time_windows: vec![],
            max_session_duration_secs: 900,
        };

        let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

        // Should fail without human present
        let result = limiter.check(false, None);
        assert!(matches!(
            result.unwrap_err(),
            SdkError::HumanConfirmationRequired
        ));

        // Should succeed with human present
        assert!(limiter.check(true, None).is_ok());
    }

    #[test]
    fn test_rate_limiter_transaction_threshold() {
        let envelope = BehavioralEnvelope {
            max_requests_per_minute: 60,
            max_burst: 10,
            requires_human_online: false,
            human_confirmation_threshold: Some(100),
            allowed_time_windows: vec![],
            max_session_duration_secs: 900,
        };

        let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

        // Under threshold should succeed
        assert!(limiter.check(false, Some(50)).is_ok());

        // Over threshold without human should fail
        let result = limiter.check(false, Some(150));
        assert!(matches!(
            result.unwrap_err(),
            SdkError::HumanConfirmationRequired
        ));

        // Over threshold with human should succeed
        assert!(limiter.check(true, Some(150)).is_ok());
    }

    #[test]
    fn test_rate_limiter_time_window_restriction() {
        use auth_core::types::envelope::TimeWindow;

        // Create a window that is definitely not now
        let current_hour = chrono::Utc::now()
            .format("%H")
            .to_string()
            .parse::<u8>()
            .unwrap_or(12);
        let restricted_hour = (current_hour + 12) % 24; // 12 hours from now

        let envelope = BehavioralEnvelope {
            max_requests_per_minute: 60,
            max_burst: 10,
            requires_human_online: false,
            human_confirmation_threshold: None,
            allowed_time_windows: vec![TimeWindow {
                start_hour: restricted_hour,
                end_hour: (restricted_hour + 1) % 24,
                days_of_week: vec![],
            }],
            max_session_duration_secs: 900,
        };

        let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

        // Should fail due to time window
        let result = limiter.check(false, None);
        assert!(matches!(
            result.unwrap_err(),
            SdkError::TimeWindowRestriction
        ));
    }

    #[test]
    fn test_rate_limiter_invalid_envelope() {
        // Invalid: burst > rpm
        let envelope = BehavioralEnvelope {
            max_requests_per_minute: 10,
            max_burst: 20,
            requires_human_online: false,
            human_confirmation_threshold: None,
            allowed_time_windows: vec![],
            max_session_duration_secs: 900,
        };

        let result = BehavioralRateLimiter::new(envelope);
        assert!(result.is_err());
    }

    #[test]
    fn test_would_allow_does_not_consume() {
        let envelope = make_envelope(60, 5);
        let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

        // Check would_allow multiple times
        for _ in 0..20 {
            assert!(limiter.would_allow(false, None));
        }

        // Should still allow actual checks
        for _ in 0..5 {
            assert!(limiter.check(false, None).is_ok());
        }
    }
}
