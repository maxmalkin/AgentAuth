//! Concurrency and race condition tests.

use crate::helpers::assertions::parse_json;
use crate::helpers::factories;
use crate::helpers::setup::{
    seed_human_principal, seed_service_provider, Body, BodyExt, Request, ServiceExt, TestApp,
};

/// Helper: full flow through token issuance.
async fn issue_test_token(app: &TestApp) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let (register_body, agent_id, hp_id, sp_id) = factories::create_signed_agent(&app.signer);
    seed_human_principal(&app.db_pool, hp_id).await;
    seed_service_provider(&app.db_pool, sp_id).await;

    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    let grant_body = factories::create_grant_request(agent_id, sp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/grants/request")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&grant_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    let body = parse_json(resp).await;
    let grant_id: uuid::Uuid = serde_json::from_value(body["id"].clone()).unwrap();

    let approve_body = factories::create_approve_request(hp_id);
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/v1/grants/{grant_id}/approve"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&approve_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    let issue_body = factories::create_issue_request(grant_id, agent_id, sp_id, hp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/issue")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&issue_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    let body = parse_json(resp).await;
    let jti: uuid::Uuid = serde_json::from_value(body["jti"].clone()).unwrap();

    (jti, agent_id, sp_id)
}

/// 50 concurrent token verifications on the same token — all should succeed.
#[tokio::test]
async fn test_50_concurrent_verifications() {
    let app = TestApp::new().await;
    let (jti, _agent_id, sp_id) = issue_test_token(&app).await;

    let mut handles = Vec::new();
    for _ in 0..50 {
        let router = app.verifier_router.clone();
        let verify_body = factories::create_verify_request(jti, sp_id);

        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/tokens/verify")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
                .unwrap();

            let resp = router.oneshot(req).await.expect("request failed");
            let body_bytes = resp
                .into_body()
                .collect()
                .await
                .expect("body read failed")
                .to_bytes();
            let body: serde_json::Value =
                serde_json::from_slice(&body_bytes).expect("json parse failed");
            body["outcome"].as_str().unwrap_or("error").to_string()
        }));
    }

    let mut allowed = 0;
    for handle in handles {
        let outcome = handle.await.expect("task panicked");
        if outcome == "allowed" {
            allowed += 1;
        }
    }

    // All 50 should succeed (each uses a unique nonce from create_verify_request)
    assert_eq!(
        allowed, 50,
        "all 50 concurrent verifications should succeed"
    );
}

/// Concurrent grant requests: only max_pending_per_agent should succeed.
#[tokio::test]
async fn test_concurrent_grant_flood() {
    let app = TestApp::new().await;
    let (register_body, agent_id, hp_id, sp_id) = factories::create_signed_agent(&app.signer);
    seed_human_principal(&app.db_pool, hp_id).await;
    seed_service_provider(&app.db_pool, sp_id).await;

    // Register agent
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    // Fire 10 grant requests concurrently — only first 5 should succeed (max_pending = 5)
    let mut handles = Vec::new();
    for _ in 0..10 {
        let router = app.registry_router.clone();
        let grant_body = factories::create_grant_request(agent_id, sp_id);

        handles.push(tokio::spawn(async move {
            let req = Request::builder()
                .method("POST")
                .uri("/v1/grants/request")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&grant_body).unwrap()))
                .unwrap();

            let resp = router.oneshot(req).await.expect("request failed");
            resp.status().as_u16()
        }));
    }

    let mut created = 0;
    let mut rejected = 0;
    for handle in handles {
        let status = handle.await.expect("task panicked");
        if status == 201 {
            created += 1;
        } else if status == 429 {
            rejected += 1;
        }
    }

    assert_eq!(created, 5, "exactly 5 grants should be created");
    assert_eq!(rejected, 5, "exactly 5 grants should be rejected");
}
