//! Token verification denial scenario tests.

use crate::helpers::assertions::{assert_status, parse_json};
use crate::helpers::factories;
use crate::helpers::setup::{seed_human_principal, seed_service_provider, Body, Request, TestApp};
use hyper::StatusCode;

/// Helper: register agent, request grant, approve, issue token.
/// Returns (jti, agent_id, sp_id).
async fn issue_test_token(app: &TestApp) -> (uuid::Uuid, uuid::Uuid, uuid::Uuid) {
    let (register_body, agent_id, hp_id, sp_id) = factories::create_signed_agent(&app.signer);
    seed_human_principal(&app.db_pool, hp_id).await;
    seed_service_provider(&app.db_pool, sp_id).await;

    // Register
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    // Request grant
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

    // Approve
    let approve_body = factories::create_approve_request(hp_id);
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/v1/grants/{grant_id}/approve"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&approve_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    // Issue token
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

/// Verify a revoked token returns "revoked" outcome.
#[tokio::test]
async fn test_verify_revoked_token() {
    let app = TestApp::new().await;
    let (jti, _agent_id, sp_id) = issue_test_token(&app).await;

    // Revoke
    let revoke_body = factories::create_revoke_request(jti);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/revoke")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&revoke_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::NO_CONTENT);

    // Verify — should be revoked
    let verify_body = factories::create_verify_request(jti, sp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["outcome"], "revoked");
}

/// Replayed nonce returns "nonce_replay" outcome.
#[tokio::test]
async fn test_verify_replayed_nonce() {
    let app = TestApp::new().await;
    let (jti, _agent_id, sp_id) = issue_test_token(&app).await;

    let fixed_nonce = hex::encode(auth_core::crypto::generate_nonce());

    // First verify — should succeed
    let verify_body = factories::create_verify_request_with_nonce(jti, sp_id, &fixed_nonce);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["outcome"], "allowed");

    // Second verify with same nonce — should be replay
    let verify_body = factories::create_verify_request_with_nonce(jti, sp_id, &fixed_nonce);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["outcome"], "nonce_replay");
}

/// Verify with wrong service provider returns "service_provider_mismatch".
#[tokio::test]
async fn test_verify_wrong_service_provider() {
    let app = TestApp::new().await;
    let (jti, _agent_id, _sp_id) = issue_test_token(&app).await;

    let wrong_sp = uuid::Uuid::now_v7();
    let verify_body = factories::create_verify_request(jti, wrong_sp);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["outcome"], "service_provider_mismatch");
}

/// Verify a nonexistent token returns "not_found".
#[tokio::test]
async fn test_verify_nonexistent_token() {
    let app = TestApp::new().await;

    let random_jti = uuid::Uuid::now_v7();
    let random_sp = uuid::Uuid::now_v7();
    let verify_body = factories::create_verify_request(random_jti, random_sp);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["outcome"], "not_found");
}

/// Verify an expired token returns "expired".
#[tokio::test]
async fn test_verify_expired_token() {
    let app = TestApp::new().await;
    let (jti, _agent_id, sp_id) = issue_test_token(&app).await;

    // Manually expire the token in the database
    sqlx::query("UPDATE issued_tokens SET expires_at = NOW() - INTERVAL '1 hour' WHERE jti = $1")
        .bind(jti)
        .execute(&app.db_pool)
        .await
        .expect("failed to expire token");

    // Also need to invalidate the cache so verifier hits DB
    // The verify request will use a fresh nonce, so the cached version
    // won't have the updated expiry. The verifier always fetches from DB
    // for the full token row, so this should work.
    let verify_body = factories::create_verify_request(jti, sp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["outcome"], "expired");
}
