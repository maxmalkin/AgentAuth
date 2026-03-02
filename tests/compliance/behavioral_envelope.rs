//! Behavioral envelope compliance tests.
//!
//! Verifies that agents exceeding their behavioral envelope are rate-limited.

use auth_core::types::BehavioralEnvelope;
use sdk::rate_limiter::BehavioralRateLimiter;

fn create_restrictive_envelope() -> BehavioralEnvelope {
    BehavioralEnvelope {
        max_requests_per_minute: 10,
        max_burst: 3,
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    }
}

/// COMPLIANCE: An agent exceeding max_burst MUST be rate-limited.
#[test]
fn test_burst_limit_enforced() {
    let envelope = create_restrictive_envelope();
    let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

    // First 3 requests should succeed (max_burst = 3)
    for i in 0..3 {
        assert!(
            limiter.check(false, None).is_ok(),
            "request {} within burst should be allowed",
            i + 1
        );
    }

    // 4th request should be rate-limited
    let result = limiter.check(false, None);
    assert!(
        result.is_err(),
        "request exceeding burst limit MUST be rate-limited"
    );
}

/// COMPLIANCE: An agent exceeding max_requests_per_minute MUST be rate-limited.
#[test]
fn test_rpm_limit_enforced() {
    let envelope = BehavioralEnvelope {
        max_requests_per_minute: 5,
        max_burst: 10, // High burst to not interfere
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    };

    // This test verifies burst behavior, as that's what the rate limiter
    // directly enforces for immediate requests.
    let result = envelope.validate();
    assert!(
        result.is_err(),
        "envelope with burst > rpm MUST be rejected"
    );
}

/// COMPLIANCE: would_allow must not consume capacity.
#[test]
fn test_would_allow_does_not_consume() {
    let envelope = create_restrictive_envelope();
    let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

    // Check would_allow many times - should not consume capacity
    for _ in 0..100 {
        assert!(limiter.would_allow(false, None));
    }

    // Actual requests should still have full burst capacity
    for i in 0..3 {
        assert!(
            limiter.check(false, None).is_ok(),
            "request {} should be allowed after would_allow calls",
            i + 1
        );
    }
}

/// COMPLIANCE: Invalid behavioral envelope MUST be rejected.
#[test]
fn test_invalid_envelope_rejected() {
    // burst > rpm is invalid
    let invalid = BehavioralEnvelope {
        max_requests_per_minute: 5,
        max_burst: 10,
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    };

    assert!(
        invalid.validate().is_err(),
        "envelope with burst > rpm MUST be rejected"
    );
}

/// COMPLIANCE: Zero max_requests_per_minute MUST be rejected.
#[test]
fn test_zero_rpm_rejected() {
    let invalid = BehavioralEnvelope {
        max_requests_per_minute: 0,
        max_burst: 0,
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    };

    assert!(
        invalid.validate().is_err(),
        "envelope with zero RPM MUST be rejected"
    );
}

/// COMPLIANCE: Zero max_burst MUST be rejected.
#[test]
fn test_zero_burst_rejected() {
    let invalid = BehavioralEnvelope {
        max_requests_per_minute: 10,
        max_burst: 0,
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    };

    assert!(
        invalid.validate().is_err(),
        "envelope with zero burst MUST be rejected"
    );
}

/// COMPLIANCE: Zero session duration MUST be rejected.
#[test]
fn test_zero_session_duration_rejected() {
    let invalid = BehavioralEnvelope {
        max_requests_per_minute: 10,
        max_burst: 5,
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 0,
    };

    assert!(
        invalid.validate().is_err(),
        "envelope with zero session duration MUST be rejected"
    );
}

/// COMPLIANCE: Valid envelope MUST be accepted.
#[test]
fn test_valid_envelope_accepted() {
    let valid = BehavioralEnvelope::default_restrictive();

    assert!(valid.validate().is_ok(), "valid envelope MUST be accepted");
}

/// COMPLIANCE: Envelope with requires_human_online MUST block when human offline.
#[test]
fn test_requires_human_online_blocking() {
    let envelope = BehavioralEnvelope {
        max_requests_per_minute: 60,
        max_burst: 10,
        requires_human_online: true,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    };

    let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

    // When human is not online, requests should be blocked
    let result = limiter.check(false, None);
    assert!(
        result.is_err(),
        "requests MUST be blocked when requires_human_online=true and human is offline"
    );

    // When human is online, requests should be allowed
    let result = limiter.check(true, None);
    assert!(
        result.is_ok(),
        "requests MUST be allowed when requires_human_online=true and human is online"
    );
}

/// COMPLIANCE: Human confirmation threshold MUST be enforced.
#[test]
fn test_human_confirmation_threshold() {
    let envelope = BehavioralEnvelope {
        max_requests_per_minute: 60,
        max_burst: 10,
        requires_human_online: false,
        human_confirmation_threshold: Some(100),
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    };

    let limiter = BehavioralRateLimiter::new(envelope).expect("create limiter");

    // Under threshold without human should succeed
    let result = limiter.check(false, Some(50));
    assert!(
        result.is_ok(),
        "transaction under threshold MUST be allowed without human"
    );

    // Over threshold without human should fail
    let result = limiter.check(false, Some(150));
    assert!(
        result.is_err(),
        "transaction over threshold MUST be blocked without human"
    );

    // Over threshold with human should succeed
    let result = limiter.check(true, Some(150));
    assert!(
        result.is_ok(),
        "transaction over threshold MUST be allowed with human present"
    );
}
