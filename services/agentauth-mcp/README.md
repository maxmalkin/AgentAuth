# agentauth-mcp

A production-ready Claude Desktop MCP server that uses AgentAuth to authenticate its tool calls. Demonstrates the full AgentAuth flow: the MCP registers as an agent, a human approves its capability grant, and then every tool call carries a short-lived DPoP-bound token verified by AgentAuth.

**This is a complete working example you can adapt for your own AI agents and services.**

## Architecture

```
Claude Desktop → agentauth-mcp (stdio) → authenticatedFetch
                                             ↓
                                 Authorization: AgentBearer <token>
                                 AgentDPoP: <EdDSA proof>
                                             ↓
                                 Your Service / API
                                             ↓
                                 POST /v1/tokens/verify
                                             ↓
                                 AgentAuth Verifier → allow/deny
```

Every tool call made by Claude is:
- **Signed** with an EdDSA private key (one per agent)
- **Bound** to a DPoP proof (cryptographic proof the agent controls the private key)
- **Verified** against human-approved capability grants
- **Logged** to an immutable audit trail with hash chain integrity

## Features

- ✅ Ed25519 signing with deterministic canonical byte format
- ✅ DPoP (Demonstrating Proof of Possession) for sender-constraint
- ✅ Token caching with automatic refresh
- ✅ Behavioral envelope enforcement (rate limits, time windows)
- ✅ Stateless token verification at verifier service
- ✅ Human-in-the-loop approval before capabilities are granted
- ✅ Structured audit logging with chain hash integrity
- ✅ Graceful degradation and circuit breaker patterns

## Getting Started

### 1. Prerequisites

- **AgentAuth stack** running locally (registry, verifier, approval UI):
  ```bash
  cd /path/to/agentauth
  ./dev.sh
  ```
  This starts:
  - Registry on `http://localhost:8080`
  - Verifier on `http://localhost:8081`
  - Approval UI on `http://localhost:3001`

- **Bun** installed (https://bun.sh) — runs TypeScript directly
- **Claude Desktop** installed

### 2. Run the MCP server

```bash
cd services/agentauth-mcp
bun run index.ts
```

**First run:**
```
[agentauth-mcp] First run — fetching demo seed IDs…
[agentauth-mcp] Generated agent ID: 01966b3c-…
[agentauth-mcp] Registered with registry
[agentauth-mcp] Approve this agent at:
[agentauth-mcp]   http://localhost:3001/approve/01966b3c-…
[agentauth-mcp] Waiting for approval…
```

Open `http://localhost:3001/approve/...` in your browser and click **Approve**. The MCP will continue:

```
[agentauth-mcp] Grant approved!
[agentauth-mcp] Ready — grant 01966b3c-… is approved
```

**Subsequent runs:**
State (agent ID and private key) is stored at `~/.config/agentauth-mcp/state.json`. The MCP starts immediately without needing re-approval.

### 3. Connect Claude Desktop

Add to your Claude Desktop config:

**macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`

**Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "agentauth": {
      "command": "bun",
      "args": ["run", "/absolute/path/to/services/agentauth-mcp/index.ts"],
      "env": {
        "REGISTRY_URL": "http://localhost:8080",
        "SERVICE_URL": "http://localhost:9090"
      }
    }
  }
}
```

Restart Claude Desktop. You should see `agentauth` in the MCP panel at the bottom.

### 4. Use the tools

Ask Claude to use one of the tools:

- **"Read my calendar"** → calls `read_calendar` (requires `read/calendar` capability)
- **"Write a file called test.txt with content 'hello'"** → calls `write_file` (requires `write/files`)
- **"Delete the file test.txt"** → calls `delete_file` (requires `delete/files`)
- **"Send a payment of $100"** → calls `make_payment` (requires `transact/payments` — will be denied unless approved)

## Customization

### Add your own tools

Edit `src/tools.ts`:

```typescript
server.registerTool(
  "my_tool",
  {
    title: "My Tool",
    description: "What this tool does",
    inputSchema: {
      param_name: z.string().describe("What this parameter does"),
    },
  },
  async ({ param_name }) => {
    const { status, data } = await callService(
      ctx,
      "POST",
      "/my-endpoint",
      { param_name }
    );
    // ... handle response
  }
);
```

### Change capabilities

Edit `src/manifest.ts` in the `CAPABILITIES` array:

```typescript
const CAPABILITIES = [
  { type: "read", resource: "your-resource" },
  { type: "write", resource: "your-resource" },
  // Add more as needed
];
```

The human approver will see these capabilities and decide whether to grant them.

### Point to your own service

Set environment variables:

```bash
REGISTRY_URL=https://your-registry.example.com \
SERVICE_URL=https://your-service.example.com \
bun run index.ts
```

## Implementation Details

### Key Files

| File | Purpose |
|------|---------|
| `index.ts` | Entry point: registers agent, requests grant, starts MCP server |
| `src/state.ts` | Persistent state: agent ID, private key, grant status |
| `src/manifest.ts` | Agent manifest construction and EdDSA signing |
| `src/dpop.ts` | DPoP JWT proof generation per RFC 7636 |
| `src/agentauth.ts` | AgentAuth HTTP client (register, grant, token, verify) |
| `src/tools.ts` | MCP tool definitions |

### Critical Implementation Points

**1. Canonical Bytes Format**

The manifest is signed using deterministic JSON serialization:
- All object keys are **sorted alphabetically** (not insertion order)
- DateTime fields use **second precision** (`2026-03-04T00:57:21Z`, no milliseconds)
- Arrays maintain their order
- Optional fields are omitted if `None`/`null`

This matches Rust's `serde_json::to_value()` behavior (uses `BTreeMap` internally).

**2. EdDSA Signing**

Uses RFC 8032 Ed25519:
- 32-byte private key (seed)
- 32-byte public key
- 64-byte signature
- No pre-hashing by the application (SHA-512 is applied internally by the algorithm)

Compatible across `@noble/ed25519` (TypeScript) and `ed25519_dalek` (Rust).

**3. DPoP Proof**

Generated for every authenticated request. Proves the agent controls the private key:
```
Header: { alg: "EdDSA", typ: "dpop+jwt", jwk: {...} }
Payload: { jti: uuid(), htm: "GET", htu: url, iat: timestamp, ath?: sha256(token) }
Signature: EdDSA(private_key, header.payload)
```

The `ath` (access token hash) field binds the DPoP proof to the access token, preventing token substitution.

**4. Token Caching**

Tokens are cached in-memory and refreshed when within 2 minutes of expiry:
```typescript
// First call: network
const token = await getToken(grantId, privKey, pubKey); // ← slow

// Second call: cached
const token = await getToken(grantId, privKey, pubKey); // ← <1ms
```

**5. Graceful Error Handling**

- **Transient errors** (connection reset, 503): retry with exponential backoff
- **Non-transient errors** (401, 403, 400): fail immediately
- **Missing capabilities**: 403 with clear error message
- **Revoked token**: automatic refresh on next call
- **Registry unreachable**: fallback with clear error

## Tools

| Tool | Description | Capability |
|------|---|---|
| `read_calendar` | Read calendar events | `read/calendar` |
| `write_file` | Write content to a file | `write/files` |
| `delete_file` | Delete a file by path | `delete/files` |
| `make_payment` | Initiate a payment | `transact/payments` |

The demo grant includes `read/calendar`, `write/files`, and `delete/files`. Attempting `make_payment` will be denied (403) because the capability was not approved.

## State Management

Agent state is stored at `~/.config/agentauth-mcp/state.json`:

```json
{
  "agent_id": "01966b3c-...",
  "private_key_b64": "...",
  "human_principal_id": "...",
  "service_provider_id": "...",
  "grant_id": "...",
  "grant_status": "approved"
}
```

**To reset:** Delete this file and restart. The MCP will re-register and request a new grant.

## Environment Variables

| Variable | Default | Description |
|----------|---------|---|
| `REGISTRY_URL` | `http://localhost:8080` | AgentAuth registry endpoint |
| `SERVICE_URL` | `http://localhost:9090` | Service/backend endpoint |

## Production Deployment

For production use:

1. **Registry**: Use a cloud-based KMS for key signing (AWS KMS, GCP, Vault Transit) — never store private keys in plaintext
2. **Verifier**: Deploy multiple read-only verifier replicas behind a load balancer
3. **TLS**: All communication must use HTTPS with mTLS for service-to-service
4. **Audit**: Configure audit log archival to cloud storage for long-term retention
5. **Monitoring**: Wire up Prometheus metrics, distributed tracing, and alerting

See the AgentAuth main documentation for production deployment patterns.

## License

AgentAuth is under the [LICENSE](../../LICENSE) file in the repository root.
