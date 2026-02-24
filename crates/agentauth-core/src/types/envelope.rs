//! Behavioral envelope types for AgentAuth.
//!
//! A behavioral envelope constrains how an agent can use its capabilities,
//! including rate limits, time windows, and human confirmation requirements.

use serde::{Deserialize, Serialize};

/// A time window during which the agent is allowed to operate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start hour (0-23, UTC).
    pub start_hour: u8,
    /// End hour (0-23, UTC).
    pub end_hour: u8,
    /// Days of week (0=Sunday, 6=Saturday). If empty, all days allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub days_of_week: Vec<u8>,
}

impl TimeWindow {
    /// Validates the time window.
    ///
    /// # Errors
    ///
    /// Returns an error if hours are out of range or days are invalid.
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        if self.start_hour > 23 {
            return Err(crate::CoreError::InvalidEnvelope(format!(
                "start_hour {} is invalid (must be 0-23)",
                self.start_hour
            )));
        }
        if self.end_hour > 23 {
            return Err(crate::CoreError::InvalidEnvelope(format!(
                "end_hour {} is invalid (must be 0-23)",
                self.end_hour
            )));
        }
        for &day in &self.days_of_week {
            if day > 6 {
                return Err(crate::CoreError::InvalidEnvelope(format!(
                    "day_of_week {day} is invalid (must be 0-6)"
                )));
            }
        }
        Ok(())
    }

    /// Checks if the given UTC time falls within this window.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn contains(&self, time: &chrono::DateTime<chrono::Utc>) -> bool {
        use chrono::{Datelike, Timelike};

        // Safe: hour() is 0-23, weekday is 0-6, both fit in u8
        let hour = time.hour() as u8;
        let day = time.weekday().num_days_from_sunday() as u8;

        // Check day of week if specified
        if !self.days_of_week.is_empty() && !self.days_of_week.contains(&day) {
            return false;
        }

        // Check hour range (handles wrap-around, e.g., 22:00 to 06:00)
        if self.start_hour <= self.end_hour {
            hour >= self.start_hour && hour < self.end_hour
        } else {
            hour >= self.start_hour || hour < self.end_hour
        }
    }
}

/// Behavioral constraints for an agent's token usage.
///
/// These constraints are enforced both server-side (registry) and client-side (SDK).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehavioralEnvelope {
    /// Maximum requests per minute the agent can make.
    pub max_requests_per_minute: u32,

    /// Maximum burst size (requests that can be made in quick succession).
    pub max_burst: u32,

    /// Whether the human principal must be online for the agent to act.
    #[serde(default)]
    pub requires_human_online: bool,

    /// Value threshold requiring human confirmation (for Transact capabilities).
    /// If a transaction exceeds this value, human confirmation is required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_confirmation_threshold: Option<u64>,

    /// Time windows when the agent is allowed to operate (UTC).
    /// If empty, the agent can operate at any time.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_time_windows: Vec<TimeWindow>,

    /// Maximum session duration in seconds.
    /// After this duration, the agent must re-authenticate.
    pub max_session_duration_secs: u32,
}

impl BehavioralEnvelope {
    /// Creates a new behavioral envelope with default values.
    #[must_use]
    pub fn default_restrictive() -> Self {
        Self {
            max_requests_per_minute: 30,
            max_burst: 5,
            requires_human_online: true,
            human_confirmation_threshold: Some(100),
            allowed_time_windows: vec![],
            max_session_duration_secs: 3600, // 1 hour
        }
    }

    /// Creates a new behavioral envelope with permissive values (for testing).
    #[must_use]
    pub fn default_permissive() -> Self {
        Self {
            max_requests_per_minute: 600,
            max_burst: 60,
            requires_human_online: false,
            human_confirmation_threshold: None,
            allowed_time_windows: vec![],
            max_session_duration_secs: 86400, // 24 hours
        }
    }

    /// Validates the envelope for internal consistency.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `max_burst` > `max_requests_per_minute` (nonsensical)
    /// - `max_session_duration_secs` is 0
    /// - Any time window is invalid
    pub fn validate(&self) -> Result<(), crate::CoreError> {
        // Token lifetime must not exceed 15 minutes per security rules
        const MAX_TOKEN_LIFETIME_SECS: u32 = 900;

        if self.max_burst > self.max_requests_per_minute {
            return Err(crate::CoreError::InvalidEnvelope(format!(
                "max_burst ({}) cannot exceed max_requests_per_minute ({})",
                self.max_burst, self.max_requests_per_minute
            )));
        }

        if self.max_requests_per_minute == 0 {
            return Err(crate::CoreError::InvalidEnvelope(
                "max_requests_per_minute cannot be 0".to_string(),
            ));
        }

        if self.max_burst == 0 {
            return Err(crate::CoreError::InvalidEnvelope(
                "max_burst cannot be 0".to_string(),
            ));
        }

        if self.max_session_duration_secs == 0 {
            return Err(crate::CoreError::InvalidEnvelope(
                "max_session_duration_secs cannot be 0".to_string(),
            ));
        }

        if self.max_session_duration_secs > MAX_TOKEN_LIFETIME_SECS {
            // This is actually enforced at the token level, but the envelope
            // shouldn't promise a longer session than is possible
            tracing::warn!(
                max_session_duration_secs = self.max_session_duration_secs,
                max_allowed = MAX_TOKEN_LIFETIME_SECS,
                "BehavioralEnvelope max_session_duration_secs exceeds max token lifetime"
            );
        }

        for window in &self.allowed_time_windows {
            window.validate()?;
        }

        Ok(())
    }

    /// Checks if the agent is allowed to operate at the given time.
    #[must_use]
    pub fn is_allowed_at_time(&self, time: &chrono::DateTime<chrono::Utc>) -> bool {
        if self.allowed_time_windows.is_empty() {
            return true;
        }
        self.allowed_time_windows.iter().any(|w| w.contains(time))
    }

    /// Converts the envelope to a human-readable description.
    #[must_use]
    pub fn to_human_readable(&self) -> String {
        let mut parts = vec![];

        parts.push(format!(
            "Up to {} actions per minute",
            self.max_requests_per_minute
        ));

        if self.max_burst < self.max_requests_per_minute {
            parts.push(format!("Burst limit: {} actions", self.max_burst));
        }

        if self.requires_human_online {
            parts.push("Requires you to be online".to_string());
        }

        if let Some(threshold) = self.human_confirmation_threshold {
            parts.push(format!(
                "Transactions over {threshold} require your confirmation"
            ));
        }

        if !self.allowed_time_windows.is_empty() {
            parts.push("Can only operate during specified time windows".to_string());
        }

        let hours = self.max_session_duration_secs / 3600;
        let minutes = (self.max_session_duration_secs % 3600) / 60;
        if hours > 0 {
            parts.push(format!("Session expires after {hours} hour(s)"));
        } else {
            parts.push(format!("Session expires after {minutes} minute(s)"));
        }

        parts.join(". ")
    }
}

impl Default for BehavioralEnvelope {
    fn default() -> Self {
        Self::default_restrictive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_envelope_validation_burst_exceeds_rpm() {
        let envelope = BehavioralEnvelope {
            max_requests_per_minute: 10,
            max_burst: 20,
            requires_human_online: false,
            human_confirmation_threshold: None,
            allowed_time_windows: vec![],
            max_session_duration_secs: 3600,
        };

        assert!(envelope.validate().is_err());
    }

    #[test]
    fn test_envelope_validation_zero_values() {
        let mut envelope = BehavioralEnvelope::default_restrictive();
        envelope.max_requests_per_minute = 0;
        assert!(envelope.validate().is_err());

        let mut envelope = BehavioralEnvelope::default_restrictive();
        envelope.max_burst = 0;
        assert!(envelope.validate().is_err());

        let mut envelope = BehavioralEnvelope::default_restrictive();
        envelope.max_session_duration_secs = 0;
        assert!(envelope.validate().is_err());
    }

    #[test]
    fn test_time_window_contains() {
        let window = TimeWindow {
            start_hour: 9,
            end_hour: 17,
            days_of_week: vec![1, 2, 3, 4, 5], // Monday-Friday
        };

        // Monday 10:00 UTC
        let monday_10am = chrono::Utc.with_ymd_and_hms(2025, 1, 6, 10, 0, 0).unwrap();
        assert!(window.contains(&monday_10am));

        // Saturday 10:00 UTC
        let saturday_10am = chrono::Utc.with_ymd_and_hms(2025, 1, 4, 10, 0, 0).unwrap();
        assert!(!window.contains(&saturday_10am));

        // Monday 20:00 UTC (outside hours)
        let monday_8pm = chrono::Utc.with_ymd_and_hms(2025, 1, 6, 20, 0, 0).unwrap();
        assert!(!window.contains(&monday_8pm));
    }

    #[test]
    fn test_time_window_wrap_around() {
        let window = TimeWindow {
            start_hour: 22,
            end_hour: 6,
            days_of_week: vec![],
        };

        // 23:00 should be in range
        let time_23 = chrono::Utc.with_ymd_and_hms(2025, 1, 1, 23, 0, 0).unwrap();
        assert!(window.contains(&time_23));

        // 03:00 should be in range
        let time_03 = chrono::Utc.with_ymd_and_hms(2025, 1, 1, 3, 0, 0).unwrap();
        assert!(window.contains(&time_03));

        // 12:00 should not be in range
        let time_12 = chrono::Utc.with_ymd_and_hms(2025, 1, 1, 12, 0, 0).unwrap();
        assert!(!window.contains(&time_12));
    }

    #[test]
    fn test_envelope_human_readable() {
        let envelope = BehavioralEnvelope {
            max_requests_per_minute: 30,
            max_burst: 5,
            requires_human_online: true,
            human_confirmation_threshold: Some(100),
            allowed_time_windows: vec![],
            max_session_duration_secs: 3600,
        };

        let readable = envelope.to_human_readable();
        assert!(readable.contains("30 actions per minute"));
        assert!(readable.contains("online"));
        assert!(readable.contains("100"));
    }

    #[test]
    fn test_envelope_serialization_roundtrip() {
        let envelope = BehavioralEnvelope::default_restrictive();
        let json = serde_json::to_string(&envelope).expect("serialize");
        let deserialized: BehavioralEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(envelope, deserialized);
    }
}
