# AgentAuth

[![CI](https://github.com/maxmalkin/AgentAuth/actions/workflows/ci.yml/badge.svg)](https://github.com/maxmalkin/AgentAuth/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://www.rust-lang.org)
[![Rust](https://img.shields.io/badge/Rust-stable-brightgreen.svg)](https://www.rust-lang.org)

A capability-based authentication system for AI agents.

AgentAuth enables AI agents to securely authenticate with third-party services while giving humans fine-grained control over what actions agents can perform. It implements a delegated authorization model where humans approve specific capabilities for their agents, and service providers can verify these grants in real-time.

## Features

- **Capability-based access control** - Define granular permissions (read, write, transact, custom) with resource-level scoping
- **Human-in-the-loop approvals** - Humans review and approve capability grants via WebAuthn/Passkey signing
- **Behavioral envelopes** - Rate limits, time windows, and transaction thresholds enforced at the protocol level
- **DPoP token binding** - Tokens are cryptographically bound to the agent's keypair, preventing theft and replay
- **Sub-5ms verification** - High-performance token verification via Redis caching
- **Audit trail** - Immutable, hash-chained audit log of all agent actions
- **HSM key storage** - Registry signing keys stored in AWS KMS, GCP Cloud KMS, or HashiCorp Vault

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────────┐
│   Agent     │────▶│  Registry   │◀────│  Approval UI    │
│   (SDK)     │     │  Service    │     │  (Human)        │
└─────────────┘     └─────────────┘     └─────────────────┘
       │                   │
       │                   ▼
       │            ┌─────────────┐
       │            │  Verifier   │
       │            │  Service    │
       │            └─────────────┘
       │                   ▲
       ▼                   │
┌─────────────────────────────────────┐
│         Service Provider            │
└─────────────────────────────────────┘
```

- **Registry Service** - Issues and manages agent access tokens (AATs), handles capability grants
- **Verifier Service** - Lightweight, horizontally-scalable token verification (read-only)
- **Approval UI** - React frontend for humans to review and approve capability requests
- **Agent SDK** - Rust library (with Python bindings) for agents to authenticate

## Quick Start

### Prerequisites

- Rust 1.85+
- Bun 1.0+
- Python 3.11+
- Docker & Docker Compose
- cargo-nextest (`cargo install cargo-nextest`)

### Setup

```bash
# Clone the repository
git clone https://github.com/maxmalkin/AgentAuth.git
cd agentauth

# Start dependencies (PostgreSQL, Redis, etc.)
docker-compose up -d

# Run database migrations
cargo install sqlx-cli
sqlx migrate run

# Build all crates
cargo build --workspace

# Run tests
cargo nextest run --workspace
```

### Running Services

The easiest way to run all services locally is with the dev runner script:

```bash
./dev.sh
```

This starts the registry, verifier, and approval UI in a single terminal with colored log output.

To run individually:

```bash
# Start the registry service
cargo run -p registry-bin

# Start the verifier service (separate terminal)
cargo run -p verifier-bin

# Start the approval UI (separate terminal)
cd services/approval-ui
bun install
bun run dev
```

## Repository Structure

```
agentauth/
├── crates/
│   ├── core/                # Protocol types, crypto (no I/O)
│   ├── registry/            # Registry service logic
│   ├── sdk/                 # Rust agent SDK
│   ├── py/                  # Python bindings (PyO3)
│   └── schema/              # JSON Schema validation
├── services/
│   ├── registry/            # Registry binary
│   ├── verifier/            # Verifier binary
│   ├── audit-archiver/      # Audit log archival
│   └── approval-ui/         # React approval frontend
├── migrations/              # SQLx database migrations
├── load-tests/              # k6 load test scripts
├── chaos/                   # Chaos engineering experiments
├── deploy/
    ├── helm/                # Kubernetes Helm charts
    └── grafana/             # Grafana dashboards
```

## SDK Usage

### Rust

```rust
use agentauth::{AgentAuthClient, AgentAuthConfig, Capability};

let config = AgentAuthConfig::new("https://registry.example.com")?;
let client = AgentAuthClient::new(config)?;

// Request capabilities
let grant = client.request_grant(
    "service-provider-id",
    vec![Capability::Read {
        resource: "calendar".to_string(),
        filter: None
    }],
).await?;

// After human approval, get a token
let token = client.get_token("service-provider-id").await?;

// Authenticate requests
client.authenticate_request("service-provider-id", &mut request).await?;
```

### Python

```python
from agentauth import AgentAuthClient, Capability

client = AgentAuthClient("https://registry.example.com")

# Request capabilities
grant = await client.request_grant(
    service_provider_id="service-provider-id",
    capabilities=[Capability.read("calendar")]
)

# After human approval, authenticate requests
headers = await client.authenticate_headers("service-provider-id", "POST", "/api/events")
```

## Security

- All signing keys stored in HSMs (AWS KMS, GCP Cloud KMS, Vault Transit)
- DPoP sender-constraint prevents token theft
- Nonce-based replay prevention
- Constant-time cryptographic comparisons
- Immutable audit log with hash chain integrity
- WebAuthn/Passkey for human approval signing

## License

MIT License
