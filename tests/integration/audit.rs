//! Audit log integrity tests.

use crate::helpers::assertions::{assert_status, parse_json};
use crate::helpers::factories;
use crate::helpers::setup::{seed_human_principal, seed_service_provider, Body, Request, TestApp};
use hyper::StatusCode;

/// Audit event is written on agent registration.
#[tokio::test]
async fn test_audit_written_on_registration() {
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

    // Check audit log
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/audit/{agent_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;

    // Response is a flat array of audit events
    let events = body.as_array().expect("response should be a JSON array");
    let has_registration = events.iter().any(|e| e["event_type"] == "agent_registered");
    assert!(
        has_registration,
        "audit log should contain agent_registered event"
    );
}

/// Audit events are written for the full grant lifecycle.
#[tokio::test]
async fn test_audit_written_on_grant_lifecycle() {
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
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);

    // Request grant
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
    let grant_id: uuid::Uuid = serde_json::from_value(body["id"].clone()).unwrap();

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

    // Issue token
    let issue_body = factories::create_issue_request(grant_id, agent_id, sp_id, hp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/tokens/issue")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&issue_body).unwrap()))
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);

    // Check audit log for all expected events
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/audit/{agent_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;

    let events = body.as_array().expect("response should be a JSON array");
    let event_types: Vec<&str> = events
        .iter()
        .filter_map(|e| e["event_type"].as_str())
        .collect();

    assert!(
        event_types.contains(&"agent_registered"),
        "should have agent_registered"
    );
    assert!(
        event_types.contains(&"grant_requested"),
        "should have grant_requested"
    );
    assert!(
        event_types.contains(&"grant_approved"),
        "should have grant_approved"
    );
    assert!(
        event_types.contains(&"token_issued"),
        "should have token_issued"
    );
}

/// Audit event is written on grant denial.
#[tokio::test]
async fn test_audit_written_on_denial() {
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
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::CREATED);

    // Request grant
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

    // Check audit
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/audit/{agent_id}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;

    let events = body.as_array().expect("response should be a JSON array");
    let has_denial = events.iter().any(|e| e["event_type"] == "grant_denied");
    assert!(has_denial, "audit log should contain grant_denied event");
}

/// Audit hash chain integrity can be verified.
#[tokio::test]
async fn test_audit_chain_integrity() {
    let app = TestApp::new().await;
    let (register_body, agent_id, hp_id, sp_id) = factories::create_signed_agent(&app.signer);
    seed_human_principal(&app.db_pool, hp_id).await;
    seed_service_provider(&app.db_pool, sp_id).await;

    // Register (creates audit event)
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&register_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    // Request grant (creates audit event)
    let grant_body = factories::create_grant_request(agent_id, sp_id);
    let req = Request::builder()
        .method("POST")
        .uri("/v1/grants/request")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&grant_body).unwrap()))
        .unwrap();
    let _ = app.registry_request(req).await;

    // Verify chain integrity
    let req = Request::builder()
        .method("GET")
        .uri(&format!("/v1/audit/{agent_id}/verify"))
        .body(Body::empty())
        .unwrap();
    let resp = app.registry_request(req).await;
    assert_status(&resp, StatusCode::OK);
    let body = parse_json(resp).await;
    assert_eq!(body["valid"], true, "audit chain should be valid");
}
