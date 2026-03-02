//! Full end-to-end happy path tests.

use crate::helpers::assertions::{assert_status, parse_json};
use crate::helpers::factories;
use crate::helpers::setup::{seed_human_principal, seed_service_provider, Body, Request, TestApp};
use hyper::StatusCode;

/// Full happy path: register agent → request grant → approve → issue token → verify.
#[tokio::test]
async fn test_full_happy_path() {
    let app = TestApp::new().await;
    let (register_body, agent_id, hp_id, sp_id) = factories::create_signed_agent(&app.signer);

    // Seed required entities
    seed_human_principal(&app.db_pool, hp_id).await;
    seed_service_provider(&app.db_pool, sp_id).await;

    // 1. Register agent
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();

    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "registered");
    assert_eq!(body["agent_id"], agent_id.to_string());

    // 2. Request grant
    let grant_body = factories::create_grant_request(agent_id, sp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/grants/request")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&grant_body).unwrap()))
        .unwrap();

    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "pending");
    let grant_id: uuid::Uuid = serde_json::from_value(body["id"].clone()).unwrap();

    // 3. Approve grant
    let approve_body = factories::create_approve_request(hp_id);
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/v1/grants/{grant_id}/approve"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&approve_body).unwrap()))
        .unwrap();

    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "approved");

    // 4. Issue token
    let issue_body = factories::create_issue_request(grant_id, agent_id, sp_id, hp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/issue")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&issue_body).unwrap()))
        .unwrap();

    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);
    let body = parse_json(resp).await;
    let jti: uuid::Uuid = serde_json::from_value(body["jti"].clone()).unwrap();

    // 5. Verify token via verifier
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
    assert_eq!(body["valid"], true);
    assert_eq!(body["outcome"], "allowed");
    assert_eq!(body["agent_id"], agent_id.to_string());
}

/// Register and retrieve an agent.
#[tokio::test]
async fn test_register_and_retrieve_agent() {
    let app = TestApp::new().await;
    let (register_body, agent_id, hp_id, _sp_id) = factories::create_signed_agent(&app.signer);

    seed_human_principal(&app.db_pool, hp_id).await;

    // Register
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();

    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);

    // Retrieve
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/agents/{agent_id}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;
    assert_eq!(body["id"], agent_id.to_string());
    assert_eq!(body["is_active"], true);
}

/// Grant lifecycle: request → get (pending) → approve → get (approved).
#[tokio::test]
async fn test_grant_lifecycle() {
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

    // Get grant — should be pending
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/grants/{grant_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.registry_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "pending");

    // Approve
    let approve_body = factories::create_approve_request(hp_id);
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/v1/grants/{grant_id}/approve"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&approve_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);

    // Get grant — should be approved
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/grants/{grant_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.registry_request(req).await;
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "approved");
}

/// Denial flow: request → deny → verify denied status.
#[tokio::test]
async fn test_denial_flow() {
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

    // Deny
    let req = Request::builder()
        .method("POST")
        .uri(&format!("/v1/grants/{grant_id}/deny"))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;
    assert_eq!(body["status"], "denied");
}
