//! Circuit breaker service for external dependencies.
//!
//! This module provides circuit breakers for all external dependencies
//! to prevent cascade failures and enable graceful degradation.
//!
//! Circuit breaker states:
//! - Closed: Requests flow normally
//! - Open: Requests are rejected immediately
//! - HalfOpen: Testing if dependency has recovered

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Circuit breaker state for metrics and observability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed - requests flow normally.
    Closed = 0,
    /// Circuit is open - requests are rejected.
    Open = 1,
    /// Circuit is half-open - testing if dependency recovered.
    HalfOpen = 2,
}

/// Circuit breaker configuration for a dependency.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Name of the dependency (for logging/metrics).
    pub name: String,
    /// Number of failures before opening the circuit.
    pub failure_threshold: u32,
    /// Time window for counting failures (in seconds).
    pub failure_window_secs: u64,
    /// Time to wait before probing if dependency recovered (in seconds).
    pub recovery_timeout_secs: u64,
    /// Number of successes in half-open state before closing.
    pub success_threshold: u32,
}

impl CircuitBreakerConfig {
    /// Configuration for PostgreSQL primary.
    #[must_use]
    pub fn postgres_primary() -> Self {
        Self {
            name: "postgres_primary".to_string(),
            failure_threshold: 3,
            failure_window_secs: 5,
            recovery_timeout_secs: 15,
            success_threshold: 1,
        }
    }

    /// Configuration for PostgreSQL replica.
    #[must_use]
    pub fn postgres_replica() -> Self {
        Self {
            name: "postgres_replica".to_string(),
            failure_threshold: 3,
            failure_window_secs: 5,
            recovery_timeout_secs: 10,
            success_threshold: 1,
        }
    }

    /// Configuration for Redis cluster.
    #[must_use]
    pub fn redis() -> Self {
        Self {
            name: "redis".to_string(),
            failure_threshold: 3,
            failure_window_secs: 2,
            recovery_timeout_secs: 5,
            success_threshold: 1,
        }
    }

    /// Configuration for KMS signing operations.
    #[must_use]
    pub fn kms_signing() -> Self {
        Self {
            name: "kms_signing".to_string(),
            failure_threshold: 5,
            failure_window_secs: 10,
            recovery_timeout_secs: 30,
            success_threshold: 1,
        }
    }

    /// Configuration for KMS key fetch operations.
    #[must_use]
    pub fn kms_key_fetch() -> Self {
        Self {
            name: "kms_key_fetch".to_string(),
            failure_threshold: 3,
            failure_window_secs: 30,
            recovery_timeout_secs: 60,
            success_threshold: 1,
        }
    }

    /// Configuration for audit write operations.
    #[must_use]
    pub fn audit_write() -> Self {
        Self {
            name: "audit_write".to_string(),
            failure_threshold: 5,
            failure_window_secs: 10,
            recovery_timeout_secs: 30,
            success_threshold: 1,
        }
    }
}

/// Internal state for the circuit breaker.
struct CircuitBreakerInner {
    /// Current state.
    state: CircuitState,
    /// Failure count in the current window.
    failure_count: u32,
    /// Success count in half-open state.
    success_count: u32,
    /// Time when the failure window started.
    window_start: Instant,
    /// Time when the circuit opened.
    opened_at: Option<Instant>,
}

/// A circuit breaker wrapper that provides state tracking and metrics.
pub struct DependencyCircuitBreaker {
    /// Configuration.
    config: CircuitBreakerConfig,
    /// Inner state (protected by RwLock).
    inner: RwLock<CircuitBreakerInner>,
    /// Total call count (atomic for metrics).
    total_calls: AtomicU64,
    /// Total failure count (atomic for metrics).
    total_failures: AtomicU64,
}

impl DependencyCircuitBreaker {
    /// Create a new circuit breaker with the given configuration.
    #[must_use]
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            inner: RwLock::new(CircuitBreakerInner {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                window_start: Instant::now(),
                opened_at: None,
            }),
            total_calls: AtomicU64::new(0),
            total_failures: AtomicU64::new(0),
        }
    }

    /// Get the current circuit state.
    pub async fn state(&self) -> CircuitState {
        let mut inner = self.inner.write().await;
        self.update_state(&mut inner);
        inner.state
    }

    /// Check if a request is allowed through the circuit.
    pub async fn is_call_permitted(&self) -> bool {
        let mut inner = self.inner.write().await;
        self.update_state(&mut inner);

        match inner.state {
            CircuitState::Open => false,
            CircuitState::Closed | CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful call.
    pub async fn record_success(&self) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);

        let mut inner = self.inner.write().await;
        self.update_state(&mut inner);

        match inner.state {
            CircuitState::Closed => {
                // Reset failure count on success
                inner.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                inner.success_count += 1;
                if inner.success_count >= self.config.success_threshold {
                    // Close the circuit
                    let old_state = inner.state;
                    inner.state = CircuitState::Closed;
                    inner.failure_count = 0;
                    inner.success_count = 0;
                    inner.opened_at = None;
                    self.log_state_change(old_state, CircuitState::Closed);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen if we check is_call_permitted first
            }
        }
    }

    /// Record a failed call.
    pub async fn record_failure(&self) {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        self.total_failures.fetch_add(1, Ordering::Relaxed);

        let mut inner = self.inner.write().await;
        self.update_state(&mut inner);

        match inner.state {
            CircuitState::Closed => {
                inner.failure_count += 1;
                if inner.failure_count >= self.config.failure_threshold {
                    // Open the circuit
                    let old_state = inner.state;
                    inner.state = CircuitState::Open;
                    inner.opened_at = Some(Instant::now());
                    self.log_state_change(old_state, CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                // Any failure in half-open goes back to open
                let old_state = inner.state;
                inner.state = CircuitState::Open;
                inner.success_count = 0;
                inner.opened_at = Some(Instant::now());
                self.log_state_change(old_state, CircuitState::Open);
            }
            CircuitState::Open => {
                // Already open, nothing to do
            }
        }
    }

    /// Update state based on time (check for recovery timeout, failure window reset).
    fn update_state(&self, inner: &mut CircuitBreakerInner) {
        let now = Instant::now();

        match inner.state {
            CircuitState::Closed => {
                // Check if failure window has expired
                let window_duration = Duration::from_secs(self.config.failure_window_secs);
                if now.duration_since(inner.window_start) > window_duration {
                    // Reset the window
                    inner.failure_count = 0;
                    inner.window_start = now;
                }
            }
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(opened_at) = inner.opened_at {
                    let recovery_duration = Duration::from_secs(self.config.recovery_timeout_secs);
                    if now.duration_since(opened_at) > recovery_duration {
                        // Transition to half-open
                        let old_state = inner.state;
                        inner.state = CircuitState::HalfOpen;
                        inner.success_count = 0;
                        self.log_state_change(old_state, CircuitState::HalfOpen);
                    }
                }
            }
            CircuitState::HalfOpen => {
                // No time-based transitions from half-open
            }
        }
    }

    /// Log a state change.
    fn log_state_change(&self, from: CircuitState, to: CircuitState) {
        match to {
            CircuitState::Open => {
                warn!(
                    dependency = %self.config.name,
                    from = ?from,
                    "Circuit breaker opened - dependency unavailable"
                );
            }
            CircuitState::HalfOpen => {
                info!(
                    dependency = %self.config.name,
                    from = ?from,
                    "Circuit breaker half-open - testing recovery"
                );
            }
            CircuitState::Closed => {
                info!(
                    dependency = %self.config.name,
                    from = ?from,
                    "Circuit breaker closed - dependency recovered"
                );
            }
        }
    }

    /// Get the dependency name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get total call count.
    #[must_use]
    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(Ordering::Relaxed)
    }

    /// Get total failure count.
    #[must_use]
    pub fn total_failures(&self) -> u64 {
        self.total_failures.load(Ordering::Relaxed)
    }
}

/// Collection of circuit breakers for all dependencies.
pub struct CircuitBreakers {
    /// PostgreSQL primary circuit breaker.
    pub postgres_primary: Arc<DependencyCircuitBreaker>,
    /// PostgreSQL replica circuit breaker.
    pub postgres_replica: Arc<DependencyCircuitBreaker>,
    /// Redis circuit breaker.
    pub redis: Arc<DependencyCircuitBreaker>,
    /// KMS signing circuit breaker.
    pub kms_signing: Arc<DependencyCircuitBreaker>,
    /// KMS key fetch circuit breaker.
    pub kms_key_fetch: Arc<DependencyCircuitBreaker>,
    /// Audit write circuit breaker.
    pub audit_write: Arc<DependencyCircuitBreaker>,
}

impl CircuitBreakers {
    /// Create a new collection of circuit breakers with default configurations.
    #[must_use]
    pub fn new() -> Self {
        Self {
            postgres_primary: Arc::new(DependencyCircuitBreaker::new(
                CircuitBreakerConfig::postgres_primary(),
            )),
            postgres_replica: Arc::new(DependencyCircuitBreaker::new(
                CircuitBreakerConfig::postgres_replica(),
            )),
            redis: Arc::new(DependencyCircuitBreaker::new(CircuitBreakerConfig::redis())),
            kms_signing: Arc::new(DependencyCircuitBreaker::new(
                CircuitBreakerConfig::kms_signing(),
            )),
            kms_key_fetch: Arc::new(DependencyCircuitBreaker::new(
                CircuitBreakerConfig::kms_key_fetch(),
            )),
            audit_write: Arc::new(DependencyCircuitBreaker::new(
                CircuitBreakerConfig::audit_write(),
            )),
        }
    }

    /// Get all circuit breaker states for metrics.
    pub async fn all_states(&self) -> Vec<(&str, CircuitState)> {
        vec![
            (
                self.postgres_primary.name(),
                self.postgres_primary.state().await,
            ),
            (
                self.postgres_replica.name(),
                self.postgres_replica.state().await,
            ),
            (self.redis.name(), self.redis.state().await),
            (self.kms_signing.name(), self.kms_signing.state().await),
            (self.kms_key_fetch.name(), self.kms_key_fetch.state().await),
            (self.audit_write.name(), self.audit_write.state().await),
        ]
    }
}

impl Default for CircuitBreakers {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_starts_closed() {
        let cb = DependencyCircuitBreaker::new(CircuitBreakerConfig::redis());
        assert_eq!(cb.state().await, CircuitState::Closed);
        assert!(cb.is_call_permitted().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_failures() {
        let config = CircuitBreakerConfig {
            name: "test".to_string(),
            failure_threshold: 2,
            failure_window_secs: 60,
            recovery_timeout_secs: 30,
            success_threshold: 1,
        };
        let cb = DependencyCircuitBreaker::new(config);

        // Record failures
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
        assert!(!cb.is_call_permitted().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_after_timeout() {
        let config = CircuitBreakerConfig {
            name: "test".to_string(),
            failure_threshold: 1,
            failure_window_secs: 60,
            recovery_timeout_secs: 0, // Immediate recovery for test
            success_threshold: 1,
        };
        let cb = DependencyCircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure().await;
        // With recovery_timeout_secs: 0, calling state() will immediately
        // transition to half-open, so we just verify it's half-open
        assert_eq!(cb.state().await, CircuitState::HalfOpen);
        assert!(cb.is_call_permitted().await);
    }

    #[tokio::test]
    async fn test_circuit_breaker_closes_after_success_in_half_open() {
        let config = CircuitBreakerConfig {
            name: "test".to_string(),
            failure_threshold: 1,
            failure_window_secs: 60,
            recovery_timeout_secs: 0,
            success_threshold: 1,
        };
        let cb = DependencyCircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure().await;

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record success to close
        cb.record_success().await;
        assert_eq!(cb.state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_reopens_on_failure_in_half_open() {
        let config = CircuitBreakerConfig {
            name: "test".to_string(),
            failure_threshold: 1,
            failure_window_secs: 60,
            recovery_timeout_secs: 60, // Long timeout
            success_threshold: 1,
        };
        let cb = DependencyCircuitBreaker::new(config);

        // Open the circuit
        cb.record_failure().await;

        // Force transition to half-open for testing
        {
            let mut inner = cb.inner.write().await;
            inner.state = CircuitState::HalfOpen;
        }

        assert_eq!(cb.state().await, CircuitState::HalfOpen);

        // Record failure in half-open - should go back to open
        cb.record_failure().await;
        // Since recovery_timeout is 60s, it won't immediately transition back
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_failure_count_resets_on_success() {
        let config = CircuitBreakerConfig {
            name: "test".to_string(),
            failure_threshold: 3,
            failure_window_secs: 60,
            recovery_timeout_secs: 30,
            success_threshold: 1,
        };
        let cb = DependencyCircuitBreaker::new(config);

        // Record some failures
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        // Success should reset count
        cb.record_success().await;

        // Need 3 more failures to open
        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }
}
