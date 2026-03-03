//! Dependency failure recovery tests.

use std::time::Duration;

/// System recovers after Redis primary failure within 30 seconds.
///
/// Requires docker-compose environment with Redis cluster.
/// Test procedure: run load, stop a Redis node, verify degraded mode,
/// restart Redis, verify recovery.
#[tokio::test]
#[ignore = "stability test: requires docker control, nightly pipeline only"]
async fn test_redis_recovery_within_30s() {
    let verifier_url =
        std::env::var("VERIFIER_URL").unwrap_or_else(|_| "http://localhost:8081".into());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client");

    // Phase 1: Verify service is healthy
    let resp = client
        .get(format!("{verifier_url}/health/ready"))
        .send()
        .await
        .expect("health check failed");
    assert!(resp.status().is_success(), "verifier should be ready");

    // Phase 2: Stop Redis node
    let stop_result = tokio::process::Command::new("docker")
        .args(["stop", "agentauth-redis-1"])
        .output()
        .await;
    assert!(stop_result.is_ok(), "failed to stop Redis container");

    // Phase 3: Verify degraded mode (verifier should fall back to DB)
    tokio::time::sleep(Duration::from_secs(2)).await;

    let nonce = hex::encode(auth_core::crypto::generate_nonce());
    let body = serde_json::json!({
        "jti": uuid::Uuid::now_v7(),
        "service_provider_id": uuid::Uuid::now_v7(),
        "nonce": nonce,
    });

    // Service should still respond (degraded, possibly 503 for Redis-dependent ops)
    let resp = client
        .post(format!("{verifier_url}/v1/tokens/verify"))
        .json(&body)
        .send()
        .await;
    // Either success or 503 (degraded) — but not connection refused
    assert!(
        resp.is_ok(),
        "verifier should still respond during Redis outage"
    );

    // Phase 4: Restart Redis
    let _ = tokio::process::Command::new("docker")
        .args(["start", "agentauth-redis-1"])
        .output()
        .await;

    // Phase 5: Wait for recovery (max 30 seconds)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut recovered = false;

    while tokio::time::Instant::now() < deadline {
        let resp = client
            .get(format!("{verifier_url}/health/ready"))
            .send()
            .await;
        if let Ok(r) = resp {
            if r.status().is_success() {
                recovered = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    assert!(
        recovered,
        "verifier should recover within 30 seconds after Redis restart"
    );
}

/// System recovers after PostgreSQL primary failure within 60 seconds.
///
/// Writes should fail, reads from replica should continue.
#[tokio::test]
#[ignore = "stability test: requires docker control, nightly pipeline only"]
async fn test_postgres_recovery_within_60s() {
    let registry_url =
        std::env::var("REGISTRY_URL").unwrap_or_else(|_| "http://localhost:8080".into());
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build HTTP client");

    // Phase 1: Verify health
    let resp = client
        .get(format!("{registry_url}/health/ready"))
        .send()
        .await
        .expect("health check failed");
    assert!(resp.status().is_success());

    // Phase 2: Stop primary PostgreSQL
    let _ = tokio::process::Command::new("docker")
        .args(["stop", "agentauth-postgres-primary"])
        .output()
        .await;

    tokio::time::sleep(Duration::from_secs(5)).await;

    // Phase 3: Writes should fail
    let body = serde_json::json!({
        "manifest": {
            "id": uuid::Uuid::now_v7(),
            "public_key": "dGVzdA",
            "key_id": "test",
            "capabilities_requested": [{ "type": "read", "resource": "test" }],
            "human_principal_id": uuid::Uuid::now_v7(),
            "issued_at": chrono::Utc::now(),
            "expires_at": chrono::Utc::now() + chrono::Duration::hours(1),
            "name": "test",
        },
        "signature": hex::encode([0u8; 64]),
    });

    let resp = client
        .post(format!("{registry_url}/v1/agents/register"))
        .json(&body)
        .send()
        .await;
    // Should fail or return error
    if let Ok(r) = resp {
        assert!(
            r.status().is_server_error(),
            "writes should fail when primary is down"
        );
    }

    // Phase 4: Restart PostgreSQL
    let _ = tokio::process::Command::new("docker")
        .args(["start", "agentauth-postgres-primary"])
        .output()
        .await;

    // Phase 5: Wait for recovery (max 60 seconds)
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let mut recovered = false;

    while tokio::time::Instant::now() < deadline {
        let resp = client
            .get(format!("{registry_url}/health/ready"))
            .send()
            .await;
        if let Ok(r) = resp {
            if r.status().is_success() {
                recovered = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    assert!(
        recovered,
        "registry should recover within 60 seconds after PostgreSQL restart"
    );
}
