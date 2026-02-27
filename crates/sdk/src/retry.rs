//! Retry logic for the AgentAuth SDK.
//!
//! Implements exponential backoff with jitter for transient errors.
//! Non-transient errors (4xx client errors) fail immediately without retry.
//!
//! # Retry Policy
//!
//! - Transient (retryable): 502, 503, 504, connection reset, timeout
//! - Non-transient (not retried): 400, 401, 403, 404, 409, 422
//! - Respects `Retry-After` header when present

use std::future::Future;
use std::time::Duration;

use crate::config::RetryConfig;
use crate::error::{SdkError, SdkResult};

/// Executes an async operation with retry logic.
///
/// # Arguments
///
/// * `config` - Retry configuration (max attempts, backoff settings)
/// * `operation` - The async operation to execute
///
/// # Returns
///
/// The result of the operation if it succeeds, or the last error if all retries fail.
///
/// # Type Parameters
///
/// * `F` - Future factory (closure that returns a future)
/// * `Fut` - The future type
/// * `T` - The success type
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, mut operation: F) -> SdkResult<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = SdkResult<T>>,
{
    let max_attempts = config.max_attempts + 1; // +1 for initial attempt
    let mut last_error = None;

    for attempt in 0..max_attempts {
        if attempt > 0 {
            let delay = config.delay_for_attempt(attempt);
            tracing::debug!(
                attempt = attempt,
                delay_ms = delay.as_millis(),
                "Retrying after transient error"
            );
            tokio::time::sleep(delay).await;
        }

        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    tracing::debug!(attempt = attempt, "Retry succeeded");
                }
                return Ok(result);
            }
            Err(e) => {
                // Check if error is retryable
                if !e.is_transient() {
                    tracing::debug!(
                        error = %e,
                        "Non-transient error, not retrying"
                    );
                    return Err(e);
                }

                tracing::debug!(
                    attempt = attempt,
                    max_attempts = max_attempts,
                    error = %e,
                    "Transient error, will retry"
                );
                last_error = Some(e);
            }
        }
    }

    // All retries exhausted
    tracing::warn!(max_attempts = max_attempts, "All retry attempts exhausted");
    Err(last_error.unwrap_or_else(|| SdkError::InternalError("No attempts made".to_string())))
}

/// Parses a `Retry-After` header value.
///
/// Supports both numeric (seconds) and HTTP-date formats.
///
/// # Arguments
///
/// * `header_value` - The Retry-After header value
///
/// # Returns
///
/// The parsed duration, or `None` if parsing fails.
pub fn parse_retry_after(header_value: &str) -> Option<Duration> {
    // Try parsing as seconds first
    if let Ok(seconds) = header_value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    // Try parsing as HTTP-date
    // Format: "Wed, 21 Oct 2015 07:28:00 GMT"
    if let Ok(datetime) = chrono::DateTime::parse_from_rfc2822(header_value) {
        let now = chrono::Utc::now();
        let retry_at = datetime.with_timezone(&chrono::Utc);
        let diff = retry_at - now;
        return diff.to_std().ok();
    }

    None
}

/// Response wrapper that includes Retry-After information.
#[derive(Debug)]
pub struct RetryableResponse<T> {
    /// The response data.
    pub data: T,
    /// Retry-After duration if the server requested a delay.
    pub retry_after: Option<Duration>,
}

impl<T> RetryableResponse<T> {
    /// Creates a new retryable response.
    #[must_use]
    pub fn new(data: T) -> Self {
        Self {
            data,
            retry_after: None,
        }
    }

    /// Sets the retry-after duration.
    #[must_use]
    pub fn with_retry_after(mut self, duration: Duration) -> Self {
        self.retry_after = Some(duration);
        self
    }
}

/// Context for retry operations, tracking state across attempts.
#[derive(Debug, Clone)]
pub struct RetryContext {
    /// Current attempt number (0-indexed).
    pub attempt: u32,
    /// Maximum number of attempts.
    pub max_attempts: u32,
    /// Time of first attempt.
    pub started_at: std::time::Instant,
    /// Total time spent in retries.
    pub total_delay: Duration,
}

impl RetryContext {
    /// Creates a new retry context.
    #[must_use]
    pub fn new(max_attempts: u32) -> Self {
        Self {
            attempt: 0,
            max_attempts,
            started_at: std::time::Instant::now(),
            total_delay: Duration::ZERO,
        }
    }

    /// Advances to the next attempt.
    pub fn next_attempt(&mut self, delay: Duration) {
        self.attempt += 1;
        self.total_delay += delay;
    }

    /// Returns true if more retry attempts are available.
    #[must_use]
    pub fn has_attempts_remaining(&self) -> bool {
        self.attempt < self.max_attempts
    }

    /// Returns the elapsed time since the first attempt.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_with_retry_succeeds_first_try() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let count = call_count.clone();

        let result = with_retry(&config, || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, SdkError>(42)
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_retries_transient_error() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            multiplier: 2.0,
            jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let count = call_count.clone();

        let result = with_retry(&config, || {
            let count = count.clone();
            async move {
                let c = count.fetch_add(1, Ordering::SeqCst);
                if c < 2 {
                    Err(SdkError::Timeout)
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_does_not_retry_permanent_error() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            multiplier: 2.0,
            jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let count = call_count.clone();

        let result = with_retry(&config, || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(SdkError::RegistryError {
                    code: "NOT_FOUND".to_string(),
                    message: "Not found".to_string(),
                    status: 404,
                })
            }
        })
        .await;

        assert!(result.is_err());
        // Should only have called once (no retries)
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_exhausts_attempts() {
        let config = RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            multiplier: 2.0,
            jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let count = call_count.clone();

        let result = with_retry(&config, || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(SdkError::Timeout)
            }
        })
        .await;

        assert!(result.is_err());
        // Initial + 2 retries = 3 calls
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_parse_retry_after_seconds() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));
        assert_eq!(parse_retry_after("120"), Some(Duration::from_secs(120)));
    }

    #[test]
    fn test_parse_retry_after_invalid() {
        assert_eq!(parse_retry_after("not a number"), None);
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("-1"), None);
    }

    #[test]
    fn test_retry_context() {
        let mut ctx = RetryContext::new(3);
        assert_eq!(ctx.attempt, 0);
        assert!(ctx.has_attempts_remaining());

        ctx.next_attempt(Duration::from_millis(100));
        assert_eq!(ctx.attempt, 1);
        assert!(ctx.has_attempts_remaining());

        ctx.next_attempt(Duration::from_millis(200));
        assert_eq!(ctx.attempt, 2);
        assert!(ctx.has_attempts_remaining());

        ctx.next_attempt(Duration::from_millis(400));
        assert_eq!(ctx.attempt, 3);
        assert!(!ctx.has_attempts_remaining());

        assert_eq!(ctx.total_delay, Duration::from_millis(700));
    }
}
