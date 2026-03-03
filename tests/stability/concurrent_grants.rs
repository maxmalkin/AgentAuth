//! Concurrent grant request stress test.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Registry handles 1,000 concurrent grant requests without deadlock or timeout.
#[tokio::test]
#[ignore = "stability test: high concurrency, nightly pipeline only"]
async fn test_1000_concurrent_grant_requests() {
    let registry_url =
        std::env::var("REGISTRY_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(100)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build HTTP client");

    let success = Arc::new(AtomicU64::new(0));
    let failure = Arc::new(AtomicU64::new(0));
    let timeout_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();

    // Spawn 1000 concurrent grant requests across 100 agents
    for i in 0..1000u64 {
        let client = client.clone();
        let url = format!("{registry_url}/v1/grants/request");
        let success = success.clone();
        let failure = failure.clone();
        let timeout_count = timeout_count.clone();

        handles.push(tokio::spawn(async move {
            let agent_id = uuid::Uuid::now_v7();
            let sp_id = uuid::Uuid::now_v7();
            let body = serde_json::json!({
                "agent_id": agent_id,
                "service_provider_id": sp_id,
                "capabilities": [
                    { "type": "read", "resource": format!("resource-{i}") }
                ],
                "behavioral_envelope": {
                    "max_requests_per_minute": 30,
                    "max_burst": 5,
                    "requires_human_online": false,
                    "human_confirmation_threshold": null,
                    "allowed_time_windows": null,
                    "max_session_duration_secs": 3600
                }
            });

            match client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) || status == 429 {
                        success.fetch_add(1, Ordering::Relaxed);
                    } else if status == 500 {
                        failure.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // Other statuses (404 for unknown agent, etc.) are expected
                        success.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        timeout_count.fetch_add(1, Ordering::Relaxed);
                    }
                    failure.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    // Wait for all with a global timeout
    let results = tokio::time::timeout(Duration::from_secs(120), async {
        for handle in handles {
            let _ = handle.await;
        }
    })
    .await;

    assert!(
        results.is_ok(),
        "1000 concurrent requests should complete within 120 seconds (no deadlock)"
    );

    let successes = success.load(Ordering::Relaxed);
    let failures = failure.load(Ordering::Relaxed);
    let timeouts = timeout_count.load(Ordering::Relaxed);

    eprintln!("Concurrent grants: success={successes}, failure={failures}, timeouts={timeouts}");

    assert_eq!(
        timeouts, 0,
        "no requests should timeout (deadlock indicator)"
    );

    // Some failures are expected (unknown agents), but no 500s
    assert_eq!(failures, 0, "no 500 errors or connection failures expected");
}
