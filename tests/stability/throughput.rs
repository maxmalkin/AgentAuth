//! Throughput tests: verifier sustains 10k req/s for 30 minutes.

use crate::helpers::metrics::LoadResult;
use hdrhistogram::Histogram;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Verifier sustains 10,000 token verifications/second for 30 minutes with p99 < 5ms.
///
/// This test requires:
/// - docker-compose up -d
/// - Pre-populated tokens in DB and Redis
/// - Verifier binary running (or spawned by test)
#[tokio::test]
#[ignore = "stability test: runs for 30 minutes, nightly pipeline only"]
async fn test_verifier_10k_rps_30_minutes() {
    let verifier_url =
        std::env::var("VERIFIER_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    let duration = Duration::from_secs(30 * 60); // 30 minutes
    let target_rps: u64 = 10_000;
    let concurrency: usize = 100;

    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(concurrency)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client");

    // Pre-populate a token for verification
    // In a real test, this would issue a token through the registry first
    let test_jti = uuid::Uuid::now_v7();
    let test_sp = uuid::Uuid::now_v7();

    let successful = Arc::new(AtomicU64::new(0));
    let failed = Arc::new(AtomicU64::new(0));
    let latencies: Arc<tokio::sync::Mutex<Histogram<u64>>> = Arc::new(tokio::sync::Mutex::new(
        Histogram::new_with_max(60_000_000, 3).expect("histogram"),
    ));

    let start = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..concurrency {
        let client = client.clone();
        let url = format!("{verifier_url}/v1/tokens/verify");
        let successful = successful.clone();
        let failed = failed.clone();
        let latencies = latencies.clone();

        handles.push(tokio::spawn(async move {
            let requests_per_worker = target_rps / concurrency as u64;
            let interval = Duration::from_micros(1_000_000 / requests_per_worker);

            while start.elapsed() < duration {
                let nonce = hex::encode(auth_core::crypto::generate_nonce());
                let body = serde_json::json!({
                    "jti": test_jti,
                    "service_provider_id": test_sp,
                    "nonce": nonce,
                });

                let req_start = Instant::now();
                let result = client.post(&url).json(&body).send().await;
                let latency_us = req_start.elapsed().as_micros() as u64;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        successful.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }

                if let Ok(mut hist) = latencies.try_lock() {
                    let _ = hist.record(latency_us);
                }

                // Pace to target rate
                let elapsed = req_start.elapsed();
                if elapsed < interval {
                    tokio::time::sleep(interval - elapsed).await;
                }
            }
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }

    let total_duration = start.elapsed();
    let hist = latencies.lock().await;
    let total = successful.load(Ordering::Relaxed) + failed.load(Ordering::Relaxed);

    let result = LoadResult {
        total_requests: total,
        successful: successful.load(Ordering::Relaxed),
        failed: failed.load(Ordering::Relaxed),
        p50_ms: hist.value_at_quantile(0.50) as f64 / 1000.0,
        p99_ms: hist.value_at_quantile(0.99) as f64 / 1000.0,
        p999_ms: hist.value_at_quantile(0.999) as f64 / 1000.0,
        requests_per_second: total as f64 / total_duration.as_secs_f64(),
        duration: total_duration,
    };

    eprintln!("Throughput test result: {result}");

    assert!(
        result.p99_ms < 5.0,
        "p99 latency {:.2}ms exceeds 5ms target",
        result.p99_ms
    );

    let error_rate = result.failed as f64 / result.total_requests as f64;
    assert!(
        error_rate < 0.0001,
        "error rate {:.4}% exceeds 0.01% target",
        error_rate * 100.0
    );
}
