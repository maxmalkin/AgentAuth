# AgentAuth

[![CI](https://github.com/maxmalkin/AgentAuth/actions/workflows/ci.yml/badge.svg)](https://github.com/maxmalkin/AgentAuth/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://www.rust-lang.org)

Human-in-the-loop authorization for AI agents. Every tool call is cryptographically signed, scoped to what a human approved, and written to an immutable audit log.

## Claude Desktop

The fastest way to see AgentAuth in action is with the included example MCP server.

**Prerequisites:** [Bun](https://bun.sh), [Rust](https://rustup.rs), [Docker](https://docs.docker.com/get-docker/)

```bash
# 1. Clone and start the full stack
git clone https://github.com/maxmalkin/AgentAuth.git
cd AgentAuth
./dev.sh
```

This starts:
- Registry on `http://localhost:8080`
- Verifier on `http://localhost:8081`
- Approval UI on `http://localhost:3001`
- Mock service on `http://localhost:9090`

```bash
# 2. Run the MCP server
cd services/agentauth-mcp
bun run index.ts
```

On first run it prints an approval URL:
```
[agentauth-mcp] Registered with registry
[agentauth-mcp] Approve this agent at:
[agentauth-mcp]   http://localhost:3001/approve/01966b3c-…
[agentauth-mcp] Waiting for approval…
```

Open the URL, review what capabilities the agent is requesting, click **Approve**. The MCP continues:
```
[agentauth-mcp] Grant approved!
[agentauth-mcp] Ready — grant 01966b3c-… is approved
```

```jsonc
// 3. Add to claude_desktop_config.json and restart Claude Desktop
// macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
// Windows: %APPDATA%\Claude\claude_desktop_config.json
{
  "mcpServers": {
    "agentauth": {
      "command": "bun",
      "args": ["run", "/absolute/path/to/AgentAuth/services/agentauth-mcp/index.ts"],
      "env": {
        "REGISTRY_URL": "http://localhost:8080",
        "SERVICE_URL": "http://localhost:9090"
      }
    }
  }
}
```

Now ask Claude to use the tools:
- **"Read my calendar"** → calls `read_calendar` (approved)
- **"Write a file called test.txt"** → calls `write_file` (approved)
- **"Send a payment of $100"** → 403 denied — `transact/payments` not in the grant

State is saved at `~/.config/agentauth-mcp/state.json`. Subsequent runs start immediately without re-approval.

---

## What AgentAuth enforces

| Guarantee | Mechanism |
|---|---|
| Agents only call what humans approved | Capability grants scoped per resource and action type |
| Stolen tokens can't be replayed | DPoP sender-constraint — token is bound to the agent's private key |
| Tokens expire | 15-minute lifetime; refresh requires the original grant to still be valid |
| Replay attacks blocked | Per-proof nonce checked in Redis before any verification |
| Rate limits enforced | Behavioral envelope (rpm, burst, time windows) checked at the verifier |
| Full audit trail | Append-only log with SHA-256 hash chain integrity |

---

## Architecture

```
Claude Desktop ──▶ agentauth-mcp ──▶ AgentAuth Registry :8080
                                             │
                        Authorization: AgentBearer <token>
                        AgentDPoP: <proof>
                                             │
                   Your Service ──▶ Verifier :8081 ──▶ allow / deny
```

- **Registry** — issues and manages agent access tokens (AATs), handles capability grants and human approvals
- **Verifier** — lightweight, read-only, horizontally-scalable token verification (sub-5ms p99 with Redis warm)
- **Approval UI** — React frontend for humans to review and approve capability requests
- **agentauth-mcp** — Claude Desktop MCP server that demonstrates the full AgentAuth flow

---

## Repository structure

```
AgentAuth/
├── services/
│   ├── agentauth-mcp/       # Claude Desktop MCP server (Bun + TypeScript)
│   ├── approval-ui/         # React approval frontend
│   ├── registry/            # Registry binary (Rust)
│   └── verifier/            # Verifier binary (Rust)
├── crates/
│   ├── core/                # Protocol types, crypto (no I/O)
│   ├── registry/            # Registry service logic
│   └── sdk/                 # Rust agent SDK
├── migrations/              # Database migrations
├── tests/integration/       # End-to-end tests
└── dev.sh                   # Start all services locally
```

---

## Integrating AgentAuth into your own agent

AgentAuth is designed to be embedded in any AI agent, not just Claude Desktop. The MCP server in `services/agentauth-mcp/` is a complete, working reference implementation you can adapt.

The core pattern:

1. Generate an Ed25519 keypair on first run
2. POST your signed manifest to `/v1/agents/register`
3. POST to `/v1/grants/request` and block until a human approves
4. On each tool call, POST to `/v1/tokens/issue` to get a short-lived token
5. Send `Authorization: AgentBearer <token>` and `AgentDPoP: <proof>` on every request to your service
6. Your service calls `/v1/tokens/verify` to check the token and capability

See [`services/agentauth-mcp/README.md`](services/agentauth-mcp/README.md) for the full implementation details.

---

## License

MIT License
