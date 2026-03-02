//! Latency and throughput measurement utilities.

use std::time::Duration;

/// Results from a load test run.
#[derive(Debug)]
pub struct LoadResult {
    /// Total number of requests sent.
    pub total_requests: u64,
    /// Number of successful requests (2xx).
    pub successful: u64,
    /// Number of failed requests.
    pub failed: u64,
    /// p50 latency in milliseconds.
    pub p50_ms: f64,
    /// p99 latency in milliseconds.
    pub p99_ms: f64,
    /// p999 latency in milliseconds.
    pub p999_ms: f64,
    /// Sustained requests per second.
    pub requests_per_second: f64,
    /// Total test duration.
    pub duration: Duration,
}

impl std::fmt::Display for LoadResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LoadResult {{ total: {}, success: {}, failed: {}, \
             p50: {:.2}ms, p99: {:.2}ms, p999: {:.2}ms, rps: {:.0}, duration: {:?} }}",
            self.total_requests,
            self.successful,
            self.failed,
            self.p50_ms,
            self.p99_ms,
            self.p999_ms,
            self.requests_per_second,
            self.duration,
        )
    }
}
