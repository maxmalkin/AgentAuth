# AgentAuth

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
- Node.js 20 LTS
- Python 3.11+
- Docker & Docker Compose

### Setup

```bash
# Clone the repository
git clone https://github.com/agentauth/agentauth.git
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

### Running the Services

```bash
# Start the registry service
cargo run -p agentauth-registry-bin

# Start the verifier service (separate terminal)
cargo run -p agentauth-verifier-bin

# Start the approval UI (separate terminal)
cd services/approval-ui
npm install
npm run dev
```

## Repository Structure

```
agentauth/
├── crates/
│   ├── agentauth-core/      # Protocol types, crypto (no I/O)
│   ├── agentauth-registry/  # Registry service logic
│   ├── agentauth-sdk/       # Rust agent SDK
│   ├── agentauth-py/        # Python bindings (PyO3)
│   └── agentauth-schema/    # JSON Schema validation
├── services/
│   ├── registry/            # Registry binary
│   ├── verifier/            # Verifier binary
│   ├── audit-archiver/      # Audit log archival
│   └── approval-ui/         # React approval frontend
├── migrations/              # SQLx database migrations
├── load-tests/              # k6 load test scripts
├── chaos/                   # Chaos engineering experiments
├── deploy/
│   ├── helm/                # Kubernetes Helm charts
│   └── grafana/             # Grafana dashboards
└── docs/
    ├── threat-model.md      # Security threat model
    ├── runbook.md           # Operations runbook
    └── capacity-planning.md # Sizing guidelines
```

## SDK Usage

### Rust

```rust
use agentauth_sdk::{AgentAuthClient, AgentAuthConfig, Capability};

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

AgentAuth is designed with security as a primary concern:

- All signing keys stored in HSMs (AWS KMS, GCP Cloud KMS, Vault Transit)
- DPoP sender-constraint prevents token theft
- Nonce-based replay prevention
- Constant-time cryptographic comparisons
- Immutable audit log with hash chain integrity
- WebAuthn/Passkey for human approval signing

See [docs/threat-model.md](docs/threat-model.md) for the full threat model.

## Performance

Target performance characteristics:

| Operation | Throughput | p99 Latency |
|-----------|------------|-------------|
| Token verification (warm) | 10,000 req/s | < 5ms |
| Token verification (cold) | 1,000 req/s | < 20ms |
| Token issuance | 500 req/s | < 50ms |

## Documentation

- [Threat Model](docs/threat-model.md) - Security analysis and mitigations
- [Operations Runbook](docs/runbook.md) - Alert response procedures
- [Capacity Planning](docs/capacity-planning.md) - Sizing and scaling guidelines

## License

MIT License - see [LICENSE](LICENSE) for details.
