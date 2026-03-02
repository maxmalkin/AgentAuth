//! Revocation propagation tests.

use crate::helpers::assertions::{assert_status, parse_json};
use crate::helpers::factories;
use crate::helpers::setup::{seed_human_principal, seed_service_provider, Body, Request, TestApp};
use hyper::StatusCode;

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

/// Revocation propagates: verify succeeds, revoke, verify fails with "revoked".
#[tokio::test]
async fn test_revocation_propagates() {
    let app = TestApp::new().await;
    let (jti, _agent_id, sp_id) = issue_test_token(&app).await;

    // Verify first — should succeed
    let verify_body = factories::create_verify_request(jti, sp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/verify")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&verify_body).unwrap()))
        .unwrap();
    let resp = app.verifier_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["outcome"], "allowed");

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

    // Verify again — should be revoked
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
    assert_eq!(body["outcome"], "revoked");
}

/// Revoking a nonexistent token returns an error.
#[tokio::test]
async fn test_revoke_nonexistent_token() {
    let app = TestApp::new().await;

    let random_jti = uuid::Uuid::now_v7();
    let revoke_body = factories::create_revoke_request(random_jti);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/revoke")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&revoke_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    // Should be 404 since the token doesn't exist
    assert_status(&resp, StatusCode::NOT_FOUND);
}
