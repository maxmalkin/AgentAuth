//! Idempotency tests.

use crate::helpers::assertions::{assert_status, parse_json};
use crate::helpers::factories;
use crate::helpers::setup::{seed_human_principal, seed_service_provider, Body, Request, TestApp};
use hyper::StatusCode;

/// Token issuance is idempotent: same grant in same window returns same JTI.
#[tokio::test]
async fn test_token_issuance_idempotent() {
    let app = TestApp::new().await;
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

    // Request + approve grant
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

    // Issue token — first time
    let issue_body = factories::create_issue_request(grant_id, agent_id, sp_id, hp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/issue")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&issue_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);
    let body1 = parse_json(resp).await;
    let jti1: uuid::Uuid = serde_json::from_value(body1["jti"].clone()).unwrap();

    // Issue token — second time (same grant, same window)
    let issue_body = factories::create_issue_request(grant_id, agent_id, sp_id, hp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/issue")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&issue_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    // Should succeed
    let body2 = parse_json(resp).await;
    let jti2: uuid::Uuid = serde_json::from_value(body2["jti"].clone()).unwrap();

    assert_eq!(
        jti1, jti2,
        "same grant should produce same JTI within idempotency window"
    );
}

/// Agent registration is idempotent: re-registering returns "already_registered".
#[tokio::test]
async fn test_agent_registration_idempotent() {
    let app = TestApp::new().await;
    let (register_body, agent_id, hp_id, _sp_id) = factories::create_signed_agent(&app.signer);
    seed_human_principal(&app.db_pool, hp_id).await;

    // Register first time
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);

    // Register same agent again
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "already_registered");
    assert_eq!(body["agent_id"], agent_id.to_string());
}
