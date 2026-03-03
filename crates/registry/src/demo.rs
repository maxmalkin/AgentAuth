//! Demo mode constants and seed data.
//!
//! When `[demo] enabled = true` in config, the registry seeds a human principal
//! and service provider on startup so the demo agent can register against them.

use tracing::info;
use uuid::Uuid;

// Fixed namespace for deterministic UUID v5 generation.
const DEMO_NAMESPACE: Uuid = Uuid::from_bytes([
    0xAA, 0x67, 0xAE, 0x01, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x01, 0x23, 0x45,
    0x67,
]);

/// Deterministic human principal ID for demo mode.
pub fn demo_human_principal_id() -> Uuid {
    Uuid::new_v5(&DEMO_NAMESPACE, b"human-principal")
}

/// Deterministic service provider ID for demo mode.
pub fn demo_service_provider_id() -> Uuid {
    Uuid::new_v5(&DEMO_NAMESPACE, b"service-provider")
}

/// Deterministic agent ID for demo mode.
pub fn demo_agent_id() -> Uuid {
    Uuid::new_v5(&DEMO_NAMESPACE, b"demo-agent")
}

/// Deterministic 32-byte seed for the demo agent's Ed25519 keypair.
/// SHA-256("agentauth-demo-agent-key-v1") truncated to 32 bytes.
pub const DEMO_AGENT_KEY_SEED: [u8; 32] = [
    0x7a, 0x1b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7, 0xf8,
    0x09, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5, 0xd6, 0xe7,
    0xf8, 0x09,
];

/// Seed demo data into the database (idempotent).
pub async fn seed_demo_data(pool: &sqlx::PgPool) {
    let hp_id = demo_human_principal_id();
    let sp_id = demo_service_provider_id();

    // Seed human principal
    match sqlx::query(
        r#"INSERT INTO human_principals (id, email, email_verified)
           VALUES ($1, $2, true)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(hp_id)
    .bind("demo@agentauth.dev")
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => info!("Seeded demo human principal: {hp_id}"),
        Ok(_) => info!("Demo human principal already exists: {hp_id}"),
        Err(e) => tracing::warn!(error = %e, "Failed to seed demo human principal"),
    }

    // Seed service provider
    let allowed_caps = serde_json::json!([
        {"type": "read", "resource": "calendar"},
        {"type": "write", "resource": "files"},
        {"type": "delete", "resource": "files"},
        {"type": "transact", "resource": "payments", "max_value": 10000},
    ]);

    match sqlx::query(
        r#"INSERT INTO service_providers (id, name, description, verification_endpoint, public_key, allowed_capabilities, is_active)
           VALUES ($1, $2, $3, $4, $5, $6, true)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(sp_id)
    .bind("Acme Cloud Services")
    .bind("Demo service provider for calendar, files, and payments")
    .bind("http://localhost:9090/verify")
    .bind(vec![0u8; 32]) // dummy public key
    .bind(allowed_caps)
    .execute(pool)
    .await
    {
        Ok(r) if r.rows_affected() > 0 => info!("Seeded demo service provider: {sp_id}"),
        Ok(_) => info!("Demo service provider already exists: {sp_id}"),
        Err(e) => tracing::warn!(error = %e, "Failed to seed demo service provider"),
    }

    info!(
        human_principal_id = %hp_id,
        service_provider_id = %sp_id,
        "Demo seed data ready"
    );
}
