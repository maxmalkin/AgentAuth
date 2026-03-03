//! AgentAuth Demo Agent
//!
//! Showcases the complete AgentAuth flow:
//! 1. Registers as an agent with the registry
//! 2. Requests a capability grant (pending human approval)
//! 3. Waits for approval via the UI
//! 4. Issues a token and makes authenticated requests
//! 5. Demonstrates capability enforcement (allowed vs denied)

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used)]
#![allow(clippy::doc_markdown)]

use anyhow::{bail, Context, Result};
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::{delete, get, post};
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use registry::demo;
use sdk::{
    AgentAuthClient, AgentId, AgentManifest, BehavioralEnvelope, Capability, HumanPrincipalId,
    SdkConfig, ServiceProviderId, SignedManifest,
};
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

const REGISTRY_URL: &str = "http://localhost:8080";
const VERIFIER_URL: &str = "http://localhost:8081";
const MOCK_SP_PORT: u16 = 9095;

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("demo_agent=info")
        .compact()
        .init();

    println!();
    println!("  ╔═══════════════════════════════════════════╗");
    println!("  ║  AgentAuth Demo Agent                     ║");
    println!("  ╚═══════════════════════════════════════════╝");
    println!();

    // Start mock service provider in background
    let mock_sp = tokio::spawn(run_mock_service_provider());

    // Wait for registry to be healthy
    wait_for_registry().await?;

    // Create the agent
    let (client, agent_id, sp_id) = create_agent_client()?;

    // Register agent
    info!("Registering agent with registry...");
    match client.register().await {
        Ok(()) => info!("Agent registered successfully"),
        Err(e) => {
            // Might already be registered (idempotent)
            info!("Registration result: {e}");
        }
    }

    // Request grant (will be pending)
    let grant_id = request_grant(&client, sp_id).await?;

    println!();
    println!("  ┌─────────────────────────────────────────────────────────┐");
    println!("  │  Grant pending! Approve it in the UI:                   │");
    println!("  │  http://localhost:3001/approve/{}   │", &grant_id[..36]);
    println!("  └─────────────────────────────────────────────────────────┘");
    println!();

    // Poll for approval
    info!("Waiting for grant approval...");
    wait_for_approval(&grant_id).await?;
    info!("Grant approved!");

    // Store grant in client (re-request to get approved status)
    let sp_id_typed = ServiceProviderId::from_uuid(sp_id);
    match client
        .request_grant(sp_id_typed, demo_capabilities(), demo_envelope())
        .await
    {
        Ok(_grant) => info!("Grant loaded into SDK"),
        Err(sdk::SdkError::GrantPending { .. }) => {
            // Still pending? Shouldn't happen after we waited
            bail!("Grant still pending after approval detected");
        }
        Err(e) => {
            // The grant was already created; the SDK may error.
            // Try get_token directly since the grant is approved.
            warn!("Re-request returned: {e}; trying direct token issuance");
        }
    }

    // Get token
    info!("Issuing token...");
    let token = match client.get_token(&sp_id_typed).await {
        Ok(t) => t,
        Err(e) => {
            warn!("SDK get_token failed: {e}; issuing token directly");
            issue_token_directly(&grant_id, agent_id, sp_id).await?
        }
    };
    info!("Token issued: {}...", &token[..8.min(token.len())]);

    // Run demo requests against mock service provider
    println!();
    println!("  Running capability enforcement demo...");
    println!();

    let results = run_demo_requests(&token, sp_id).await;

    // Print results
    print_results(&results);

    // Keep running so user can inspect
    println!();
    println!("  Demo complete! Press Ctrl+C to exit.");
    println!();

    mock_sp.await?;
    Ok(())
}

// ============================================================================
// Agent Setup
// ============================================================================

fn create_agent_client() -> Result<(AgentAuthClient, uuid::Uuid, uuid::Uuid)> {
    let signing_key = SigningKey::from_bytes(&demo::DEMO_AGENT_KEY_SEED);
    let public_key_bytes = signing_key.verifying_key().to_bytes();
    let public_key_b64 = URL_SAFE_NO_PAD.encode(public_key_bytes);

    let agent_id = demo::demo_agent_id();
    let hp_id = demo::demo_human_principal_id();
    let sp_id = demo::demo_service_provider_id();
    let now = Utc::now();

    let manifest = AgentManifest {
        id: AgentId::from_uuid(agent_id),
        public_key: public_key_b64,
        key_id: "demo-key-001".to_string(),
        capabilities_requested: demo_all_capabilities(),
        human_principal_id: HumanPrincipalId::from_uuid(hp_id),
        issued_at: now,
        expires_at: now + Duration::days(90),
        name: "Claude Research Assistant".to_string(),
        description: Some("AI assistant that manages calendars, files, and payments".to_string()),
        model_origin: Some("anthropic.com".to_string()),
    };

    // Sign the manifest
    let canonical_bytes = manifest
        .to_canonical_bytes()
        .context("Failed to serialize manifest")?;
    let signature = signing_key.sign(&canonical_bytes);
    let signature_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    let signed = SignedManifest {
        manifest,
        signature: signature_b64,
        signing_key_id: "demo-key-001".to_string(),
    };

    let config = SdkConfig::new(REGISTRY_URL).context("Invalid registry URL")?;
    let client = AgentAuthClient::new(config, signed, &demo::DEMO_AGENT_KEY_SEED)
        .context("Failed to create SDK client")?;

    Ok((client, agent_id, sp_id))
}

/// Capabilities the agent will request in its grant (subset of all).
fn demo_capabilities() -> Vec<Capability> {
    vec![
        Capability::Read {
            resource: "calendar".to_string(),
            filter: None,
        },
        Capability::Write {
            resource: "files".to_string(),
            conditions: None,
        },
        Capability::Delete {
            resource: "files".to_string(),
            filter: None,
        },
    ]
}

/// All capabilities declared in the manifest.
fn demo_all_capabilities() -> Vec<Capability> {
    let mut caps = demo_capabilities();
    caps.push(Capability::Transact {
        resource: "payments".to_string(),
        max_value: 1000,
        currency: Some("USD".to_string()),
    });
    caps
}

fn demo_envelope() -> BehavioralEnvelope {
    BehavioralEnvelope {
        max_requests_per_minute: 30,
        max_burst: 5,
        requires_human_online: false,
        human_confirmation_threshold: None,
        allowed_time_windows: vec![],
        max_session_duration_secs: 3600,
    }
}

// ============================================================================
// Grant Handling
// ============================================================================

async fn request_grant(client: &AgentAuthClient, sp_id: uuid::Uuid) -> Result<String> {
    let sp_id_typed = ServiceProviderId::from_uuid(sp_id);

    match client
        .request_grant(sp_id_typed, demo_capabilities(), demo_envelope())
        .await
    {
        Ok(grant) => Ok(grant.grant_id),
        Err(sdk::SdkError::GrantPending { grant_id }) => Ok(grant_id),
        Err(e) => bail!("Grant request failed: {e}"),
    }
}

async fn wait_for_approval(grant_id: &str) -> Result<()> {
    let http = reqwest::Client::new();
    let url = format!("{REGISTRY_URL}/v1/grants/{grant_id}");

    for _ in 0..300 {
        // 10 min timeout
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let resp = http.get(&url).send().await;
        if let Ok(r) = resp {
            if let Ok(body) = r.json::<serde_json::Value>().await {
                if body.get("status").and_then(|s| s.as_str()) == Some("approved") {
                    return Ok(());
                }
            }
        }
    }

    bail!("Timed out waiting for grant approval")
}

#[derive(Deserialize)]
struct TokenResp {
    jti: uuid::Uuid,
}

async fn issue_token_directly(
    grant_id: &str,
    _agent_id: uuid::Uuid,
    _sp_id: uuid::Uuid,
) -> Result<String> {
    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{REGISTRY_URL}/v1/tokens/issue"))
        .json(&serde_json::json!({
            "grant_id": grant_id,
        }))
        .send()
        .await
        .context("Token issue request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Token issue failed ({status}): {body}");
    }

    let token: TokenResp = resp
        .json()
        .await
        .context("Failed to parse token response")?;
    Ok(token.jti.to_string())
}

// ============================================================================
// Demo Requests
// ============================================================================

struct DemoResult {
    action: &'static str,
    status: u16,
    allowed: bool,
    reason: String,
}

async fn run_demo_requests(token: &str, sp_id: uuid::Uuid) -> Vec<DemoResult> {
    let http = reqwest::Client::new();
    let base = format!("http://localhost:{MOCK_SP_PORT}");
    let mut results = Vec::new();

    let actions: Vec<(&str, &str, &str)> = vec![
        ("GET", "/calendar", "Read calendar"),
        ("POST", "/files", "Write to files"),
        ("DELETE", "/files/doc.txt", "Delete file"),
        ("POST", "/payments", "Make payment"),
    ];

    for (method, path, action) in actions {
        let url = format!("{base}{path}");
        let req = match method {
            "POST" => http.post(&url),
            "DELETE" => http.delete(&url),
            _ => http.get(&url),
        };

        let resp = req
            .header("Authorization", format!("AgentBearer {token}"))
            .header("X-Token-JTI", token)
            .header("X-Service-Provider-ID", sp_id.to_string())
            .send()
            .await;

        match resp {
            Ok(r) => {
                let status = r.status().as_u16();
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                let reason = body
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(if status == 200 {
                        "Capability granted"
                    } else {
                        "Unknown"
                    })
                    .to_string();

                results.push(DemoResult {
                    action,
                    status,
                    allowed: status == 200,
                    reason,
                });
            }
            Err(e) => {
                results.push(DemoResult {
                    action,
                    status: 0,
                    allowed: false,
                    reason: format!("Connection error: {e}"),
                });
            }
        }
    }

    results
}

fn print_results(results: &[DemoResult]) {
    println!("  ┌──────────────────────┬──────────┬────────────────────────────────┐");
    println!("  │  Action              │  Result  │  Reason                        │");
    println!("  ├──────────────────────┼──────────┼────────────────────────────────┤");

    for r in results {
        let icon = if r.allowed { "✅" } else { "❌" };
        let result_str = format!("{} {}", icon, r.status);
        let reason = if r.reason.len() > 28 {
            format!("{}...", &r.reason[..25])
        } else {
            r.reason.clone()
        };
        println!(
            "  │  {:<18} │  {:<6}  │  {:<28}  │",
            r.action, result_str, reason
        );
    }

    println!("  └──────────────────────┴──────────┴────────────────────────────────┘");

    let allowed = results.iter().filter(|r| r.allowed).count();
    let denied = results.iter().filter(|r| !r.allowed).count();
    println!();
    println!("  {allowed} allowed, {denied} denied");
}

// ============================================================================
// Registry Health Check
// ============================================================================

async fn wait_for_registry() -> Result<()> {
    let http = reqwest::Client::new();
    let url = format!("{REGISTRY_URL}/health/ready");

    for i in 0..30 {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        if let Ok(r) = http.get(&url).send().await {
            if r.status().is_success() {
                info!("Registry is ready");
                return Ok(());
            }
        }
        if i % 5 == 0 && i > 0 {
            info!("Waiting for registry to be ready...");
        }
    }

    bail!("Registry not ready after 30 seconds")
}

// ============================================================================
// Mock Service Provider (port 9090)
// ============================================================================

async fn run_mock_service_provider() {
    let app = Router::new()
        .route("/calendar", get(handle_calendar))
        .route("/files", post(handle_files_write))
        .route("/files/{path}", delete(handle_files_delete))
        .route("/payments", post(handle_payments));

    let addr = SocketAddr::from(([0, 0, 0, 0], MOCK_SP_PORT));
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind mock service provider on :{MOCK_SP_PORT}: {e}");
            return;
        }
    };

    info!("Mock service provider listening on :{MOCK_SP_PORT}");
    if let Err(e) = axum::serve(listener, app).await {
        error!("Mock service provider error: {e}");
    }
}

#[derive(Deserialize)]
struct VerifyResponse {
    valid: bool,
    outcome: String,
    granted_capabilities: Option<serde_json::Value>,
}

async fn verify_token(headers: &HeaderMap) -> Result<VerifyResponse, (u16, String)> {
    let jti = headers
        .get("X-Token-JTI")
        .and_then(|v| v.to_str().ok())
        .ok_or((401, "Missing X-Token-JTI header".to_string()))?;

    let sp_id = headers
        .get("X-Service-Provider-ID")
        .and_then(|v| v.to_str().ok())
        .ok_or((401, "Missing X-Service-Provider-ID header".to_string()))?;

    let nonce = uuid::Uuid::now_v7().to_string();

    let http = reqwest::Client::new();
    let resp = http
        .post(format!("{VERIFIER_URL}/v1/tokens/verify"))
        .json(&serde_json::json!({
            "jti": jti,
            "service_provider_id": sp_id,
            "nonce": nonce,
        }))
        .send()
        .await
        .map_err(|e| (502, format!("Verifier unreachable: {e}")))?;

    let verify: VerifyResponse = resp
        .json()
        .await
        .map_err(|e| (502, format!("Invalid verifier response: {e}")))?;

    if !verify.valid {
        return Err((403, format!("Token invalid: {}", verify.outcome)));
    }

    Ok(verify)
}

fn has_capability(
    caps: Option<&serde_json::Value>,
    required_type: &str,
    required_resource: &str,
) -> bool {
    let Some(caps) = caps else { return false };
    let Some(arr) = caps.as_array() else {
        return false;
    };

    arr.iter().any(|c| {
        c.get("type").and_then(|t| t.as_str()) == Some(required_type)
            && c.get("resource").and_then(|r| r.as_str()) == Some(required_resource)
    })
}

async fn handle_calendar(headers: HeaderMap) -> axum::response::Response {
    match verify_token(&headers).await {
        Ok(v) if has_capability(v.granted_capabilities.as_ref(), "read", "calendar") => {
            axum::Json(serde_json::json!({
                "message": "Capability granted",
                "data": {
                    "events": [
                        {"title": "Team standup", "time": "09:00"},
                        {"title": "Design review", "time": "14:00"},
                    ]
                }
            }))
            .into_response()
        }
        Ok(_) => (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "message": "Capability not granted: read:calendar"
            })),
        )
            .into_response(),
        Err((code, msg)) => (
            axum::http::StatusCode::from_u16(code).unwrap_or(axum::http::StatusCode::FORBIDDEN),
            axum::Json(serde_json::json!({ "message": msg })),
        )
            .into_response(),
    }
}

async fn handle_files_write(headers: HeaderMap) -> axum::response::Response {
    match verify_token(&headers).await {
        Ok(v) if has_capability(v.granted_capabilities.as_ref(), "write", "files") => {
            axum::Json(serde_json::json!({
                "message": "Capability granted",
                "data": {"file_id": "f-001", "status": "uploaded"}
            }))
            .into_response()
        }
        Ok(_) => (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "message": "Capability not granted: write:files"
            })),
        )
            .into_response(),
        Err((code, msg)) => (
            axum::http::StatusCode::from_u16(code).unwrap_or(axum::http::StatusCode::FORBIDDEN),
            axum::Json(serde_json::json!({ "message": msg })),
        )
            .into_response(),
    }
}

async fn handle_files_delete(
    headers: HeaderMap,
    Path(path): Path<String>,
) -> axum::response::Response {
    match verify_token(&headers).await {
        Ok(v) if has_capability(v.granted_capabilities.as_ref(), "delete", "files") => {
            axum::Json(serde_json::json!({
                "message": "Capability granted",
                "data": {"deleted": path}
            }))
            .into_response()
        }
        Ok(_) => (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "message": "Capability not granted: delete:files"
            })),
        )
            .into_response(),
        Err((code, msg)) => (
            axum::http::StatusCode::from_u16(code).unwrap_or(axum::http::StatusCode::FORBIDDEN),
            axum::Json(serde_json::json!({ "message": msg })),
        )
            .into_response(),
    }
}

async fn handle_payments(headers: HeaderMap) -> axum::response::Response {
    match verify_token(&headers).await {
        Ok(v) if has_capability(v.granted_capabilities.as_ref(), "transact", "payments") => {
            axum::Json(serde_json::json!({
                "message": "Capability granted",
                "data": {"transaction_id": "tx-001", "status": "completed"}
            }))
            .into_response()
        }
        Ok(_) => (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "message": "Capability not granted: transact:payments"
            })),
        )
            .into_response(),
        Err((code, msg)) => (
            axum::http::StatusCode::from_u16(code).unwrap_or(axum::http::StatusCode::FORBIDDEN),
            axum::Json(serde_json::json!({ "message": msg })),
        )
            .into_response(),
    }
}

use axum::response::IntoResponse;
