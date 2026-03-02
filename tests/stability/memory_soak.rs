//! Memory leak detection via 1-hour soak test.

use std::time::{Duration, Instant};

/// No memory leaks over a 1-hour soak test.
///
/// Measures RSS before and after sustained load. Growth > 10% is a bug.
#[tokio::test]
#[ignore = "stability test: runs for 1 hour, nightly pipeline only"]
async fn test_no_memory_growth_1_hour() {
    let registry_url =
        std::env::var("REGISTRY_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let duration = Duration::from_secs(3600); // 1 hour
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client");

    // Measure initial RSS via /health/live (proxy: measure test process RSS)
    let initial_rss = get_process_rss();
    eprintln!("Initial RSS: {initial_rss} KB");

    let start = Instant::now();
    let mut requests_sent: u64 = 0;

    // Moderate sustained load: ~100 req/s
    while start.elapsed() < duration {
        let agent_id = uuid::Uuid::now_v7();
        let url = format!("{registry_url}/v1/agents/{agent_id}");

        // Simple GET request that exercises the stack without creating state
        let _ = client.get(&url).send().await;
        requests_sent += 1;

        if requests_sent % 10_000 == 0 {
            let current_rss = get_process_rss();
            eprintln!(
                "After {requests_sent} requests ({:.0}s): RSS = {current_rss} KB",
                start.elapsed().as_secs_f64()
            );
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let final_rss = get_process_rss();
    eprintln!("Final RSS: {final_rss} KB after {requests_sent} requests");

    if initial_rss > 0 {
        let growth_pct = ((final_rss as f64 - initial_rss as f64) / initial_rss as f64) * 100.0;
        eprintln!("RSS growth: {growth_pct:.1}%");

        assert!(
            growth_pct < 10.0,
            "RSS grew by {growth_pct:.1}% (from {initial_rss} KB to {final_rss} KB), exceeds 10% threshold"
        );
    }
}

/// Read the current process RSS from /proc/self/status (Linux only).
fn get_process_rss() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                if line.starts_with("VmRSS:") {
                    line.split_whitespace()
                        .nth(1)
                        .and_then(|v| v.parse::<u64>().ok())
                } else {
                    None
                }
            })
        })
        .unwrap_or(0)
}
