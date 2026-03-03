#!/usr/bin/env python3
"""Register a demo agent with the AgentAuth registry.

Generates an Ed25519 keypair, builds a signed manifest, registers
the agent, creates a service provider, and submits a grant request
so there's a pending approval visible in the UI.

Requires: pip install PyNaCl requests
"""

import json
import sys
import hashlib
from base64 import urlsafe_b64encode
from datetime import datetime, timezone, timedelta
from uuid import uuid4

try:
    from nacl.signing import SigningKey
    import requests
except ImportError:
    print("Install dependencies: pip install PyNaCl requests")
    sys.exit(1)

REGISTRY = "http://localhost:8080"
DB_URL = "postgres://agentauth:agentauth_dev@localhost:5434/agentauth"

# ── Generate Ed25519 keypair ────────────────────────────────────
signing_key = SigningKey.generate()
verify_key = signing_key.verify_key
public_key_b64 = urlsafe_b64encode(verify_key.encode()).decode().rstrip("=")

# ── Build manifest ──────────────────────────────────────────────
agent_id = str(uuid4())
hp_id = str(uuid4())
sp_id = str(uuid4())
now = datetime.now(timezone.utc)

manifest = {
    "id": agent_id,
    "public_key": public_key_b64,
    "key_id": "demo-key-001",
    "capabilities_requested": [
        {"type": "read", "resource": "calendar", "filter": None},
        {"type": "write", "resource": "files", "conditions": None},
    ],
    "human_principal_id": hp_id,
    "issued_at": now.strftime("%Y-%m-%dT%H:%M:%S.%fZ"),
    "expires_at": (now + timedelta(days=90)).strftime("%Y-%m-%dT%H:%M:%S.%fZ"),
    "name": "Claude Research Assistant",
    "description": "AI assistant that reads calendars and manages files",
    "model_origin": "anthropic.com",
}

# ── Sign the manifest (canonical JSON bytes) ────────────────────
# Rust's serde_json::to_value -> to_vec produces sorted keys with
# no whitespace, and skips None/null fields marked skip_serializing_if.
# We must produce the exact same bytes for the signature to verify.
def to_canonical(obj):
    """Mimic serde_json::to_value then to_vec: sorted keys, skip None values."""
    if obj is None:
        return "null"
    if isinstance(obj, bool):
        return "true" if obj else "false"
    if isinstance(obj, (int, float)):
        return json.dumps(obj)
    if isinstance(obj, str):
        return json.dumps(obj)
    if isinstance(obj, list):
        items = [to_canonical(v) for v in obj]
        return "[" + ",".join(items) + "]"
    if isinstance(obj, dict):
        # Sort keys, skip None values (matches serde skip_serializing_if)
        items = []
        for k in sorted(obj.keys()):
            v = obj[k]
            if v is None:
                continue
            items.append(json.dumps(k) + ":" + to_canonical(v))
        return "{" + ",".join(items) + "}"
    return json.dumps(obj)

canonical = to_canonical(manifest).encode()
signed = signing_key.sign(canonical)
signature_hex = signed.signature.hex()

# ── Seed human principal + service provider directly via psql ───
# (These tables have no API endpoints for creation in the registry)
import subprocess

sp_short = sp_id[:8]
seed_sql = f"""
INSERT INTO human_principals (id, email, email_verified)
VALUES ('{hp_id}', 'demo-{hp_id}@agentauth.dev', true)
ON CONFLICT (id) DO NOTHING;

INSERT INTO service_providers (id, name, description, verification_endpoint, public_key, allowed_capabilities, is_active)
VALUES (
    '{sp_id}',
    'Acme Calendar ({sp_short})',
    'Calendar management API',
    'http://localhost:9090/verify',
    '\\x{"0" * 64}',
    '[{{"type":"read","resource":"calendar"}},{{"type":"write","resource":"calendar"}}]'::jsonb,
    true
) ON CONFLICT (id) DO NOTHING;
"""

print("Seeding human principal and service provider...")
result = subprocess.run(
    ["psql", DB_URL, "-v", "ON_ERROR_STOP=1"],
    input=seed_sql,
    capture_output=True,
    text=True,
)
if result.returncode != 0:
    print(f"psql error: {result.stderr}")
    sys.exit(1)
print("  Done.")

# ── Register agent ──────────────────────────────────────────────
print(f"\nRegistering agent '{manifest['name']}'...")
resp = requests.post(
    f"{REGISTRY}/v1/agents/register",
    json={"manifest": manifest, "signature": signature_hex},
)
print(f"  Status: {resp.status_code}")
print(f"  Response: {json.dumps(resp.json(), indent=2)}")

if resp.status_code not in (200, 201):
    print("Registration failed!")
    sys.exit(1)

# ── Request a grant ─────────────────────────────────────────────
print(f"\nRequesting grant for calendar access...")
resp = requests.post(
    f"{REGISTRY}/v1/grants/request",
    json={
        "agent_id": agent_id,
        "service_provider_id": sp_id,
        "capabilities": [
            {"type": "read", "resource": "calendar"},
        ],
        "behavioral_envelope": {
            "max_requests_per_minute": 30,
            "max_burst": 5,
            "requires_human_online": False,
            "max_session_duration_secs": 3600,
        },
    },
)
print(f"  Status: {resp.status_code}")
body = resp.json()
print(f"  Response: {json.dumps(body, indent=2)}")

grant_id = body.get("id") or body.get("grant_id")

# ── Print summary ───────────────────────────────────────────────
print("\n" + "=" * 60)
print("Demo data created!")
print("=" * 60)
print(f"  Agent ID:            {agent_id}")
print(f"  Human Principal ID:  {hp_id}")
print(f"  Service Provider ID: {sp_id}")
print(f"  Grant ID:            {grant_id}")
print()
print("Open the approval UI:")
print(f"  http://localhost:3001/approve/{grant_id}")
print()
print("Or list agents:")
print(f"  http://localhost:3001/agents")
