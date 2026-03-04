import { makeDpopProof, jwkThumbprint } from "./dpop.js";
import type { SignedManifest, Capability } from "./manifest.js";
import type { AgentState } from "./state.js";
import { saveState } from "./state.js";

const REGISTRY = process.env.REGISTRY_URL ?? "http://localhost:8080";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyJson = Record<string, any>;

interface TokenCache {
  jti: string;
  expiresAt: Date;
}

let tokenCache: TokenCache | null = null;

/** Register the agent with the registry. Idempotent: 201 and 409 are both fine. */
export async function register(signedManifest: SignedManifest): Promise<void> {
  const res = await fetch(`${REGISTRY}/v1/agents/register`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(signedManifest),
  });
  if (!res.ok && res.status !== 409) {
    const body = await res.text();
    throw new Error(`Registration failed (${res.status}): ${body}`);
  }
}

/**
 * Request a grant from the registry. Returns the grant_id once approved.
 * If the grant is pending, blocks and prints the approval URL, polling every 2s.
 * Idempotent — registry returns the existing pending grant if one exists.
 */
export async function requestOrLoadGrant(
  state: AgentState,
  privKey: Uint8Array,
  pubKey: Uint8Array,
  capabilities: Capability[],
): Promise<string> {
  // If we already have an approved grant, skip straight to token issuance.
  if (state.grant_status === "approved" && state.grant_id) {
    return state.grant_id;
  }

  const res = await fetch(`${REGISTRY}/v1/grants/request`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      agent_id: state.agent_id,
      service_provider_id: state.service_provider_id,
      requested_capabilities: capabilities,
      requested_envelope: {
        max_requests_per_minute: 30,
        max_burst: 10,
        requires_human_online: false,
        max_session_duration_secs: 900,
      },
    }),
  });

  if (!res.ok) {
    const body = await res.text();
    throw new Error(`Grant request failed (${res.status}): ${body}`);
  }

  const grant = await res.json() as AnyJson;
  const grantId: string = grant["id"] ?? grant["grant_id"];

  // Save the pending grant_id immediately so restarts can resume polling.
  state.grant_id = grantId;
  state.grant_status = "pending";
  await saveState(state);

  if (grant["status"] === "approved") {
    state.grant_status = "approved";
    await saveState(state);
    return grantId;
  }

  // Pending — print approval URL and poll.
  console.error(`[agentauth-mcp] Approve this agent at:`);
  console.error(`[agentauth-mcp]   http://localhost:3001/approve/${grantId}`);
  console.error(`[agentauth-mcp] Waiting for approval…`);

  while (true) {
    await Bun.sleep(2000);
    const pollRes = await fetch(`${REGISTRY}/v1/grants/${grantId}`);
    if (!pollRes.ok) continue;
    const updated = await pollRes.json() as AnyJson;
    if (updated["status"] === "approved") {
      state.grant_status = "approved";
      await saveState(state);
      console.error(`[agentauth-mcp] Grant approved!`);
      return grantId;
    }
    if (updated["status"] === "denied") throw new Error("Grant was denied by the human principal.");
    if (updated["status"] === "expired") throw new Error("Grant request expired before approval.");
  }
}

/**
 * Get a valid access token, using the cache when possible.
 * Refreshes when within 2 minutes of expiry.
 */
export async function getToken(
  grantId: string,
  privKey: Uint8Array,
  pubKey: Uint8Array,
): Promise<string> {
  const TWO_MINUTES = 2 * 60 * 1000;
  if (tokenCache && tokenCache.expiresAt.getTime() - Date.now() > TWO_MINUTES) {
    return tokenCache.jti;
  }

  const issueUrl = `${REGISTRY}/v1/tokens/issue`;
  const dpop = await makeDpopProof(privKey, pubKey, "POST", issueUrl);

  const res = await fetch(issueUrl, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "AgentDPoP": dpop,
    },
    body: JSON.stringify({
      grant_id: grantId,
      dpop_thumbprint: jwkThumbprint(pubKey),
    }),
  });

  if (!res.ok) {
    const body = await res.text();
    throw new Error(`Token issuance failed (${res.status}): ${body}`);
  }

  const data = await res.json() as AnyJson;
  const jti: string = data["jti"] ?? data["access_token"];
  const expiresAt = new Date(data["expires_at"]);

  tokenCache = { jti, expiresAt };
  return jti;
}

/**
 * Make an authenticated HTTP request to a service.
 * Attaches Authorization: AgentBearer <token> and AgentDPoP headers.
 */
export async function authenticatedFetch(
  method: string,
  url: string,
  body: unknown,
  grantId: string,
  privKey: Uint8Array,
  pubKey: Uint8Array,
): Promise<Response> {
  const token = await getToken(grantId, privKey, pubKey);
  const dpop = await makeDpopProof(privKey, pubKey, method, url, token);

  return fetch(url, {
    method,
    headers: {
      "Authorization": `AgentBearer ${token}`,
      "AgentDPoP": dpop,
      ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
}

/** Fetch human principal ID and service provider ID from the discovery document. */
export async function fetchDiscoveryIds(): Promise<{
  humanPrincipalId: string;
  serviceProviderId: string;
}> {
  const res = await fetch(`${REGISTRY}/.well-known/agentauth`);
  if (!res.ok) throw new Error(`Discovery fetch failed (${res.status})`);
  const doc = await res.json() as AnyJson;

  // The registry seeds demo data — extract demo IDs from the discovery doc
  // or fall back to the well-known demo seed values.
  const humanPrincipalId: string =
    doc["demo"]?.human_principal_id ?? await fetchDemoHumanPrincipalId();
  const serviceProviderId: string =
    doc["demo"]?.service_provider_id ?? await fetchDemoServiceProviderId();

  return { humanPrincipalId, serviceProviderId };
}

async function fetchDemoHumanPrincipalId(): Promise<string> {
  // GET /v1/agents returns the seeded agents; we need a different path.
  // Fall back to the known deterministic demo ID derived from demo.rs constants.
  // This is safe — these IDs are public demo seed values, not secrets.
  const res = await fetch(`${REGISTRY}/v1/demo/ids`).catch(() => null);
  if (res?.ok) {
    const data = await res.json() as AnyJson;
    return data["human_principal_id"];
  }
  // Hard fallback: use the deterministic UUID from demo.rs seed data.
  // Computed from UUID::new_v5(DEMO_NAMESPACE, b"human-principal").
  return await resolveDemoId("human-principal");
}

async function fetchDemoServiceProviderId(): Promise<string> {
  const res = await fetch(`${REGISTRY}/v1/demo/ids`).catch(() => null);
  if (res?.ok) {
    const data = await res.json() as AnyJson;
    return data["service_provider_id"];
  }
  return await resolveDemoId("service-provider");
}

/**
 * Compute the deterministic UUID v5 that demo.rs generates.
 * DEMO_NAMESPACE = AA67AE01-1234-5678-9ABC-DEF001234567
 * UUID v5 = SHA-1(namespace_bytes + name_bytes), formatted per RFC 4122.
 */
async function resolveDemoId(name: "human-principal" | "service-provider"): Promise<string> {
  // DEMO_NAMESPACE from crates/registry/src/demo.rs
  const ns = new Uint8Array([
    0xaa, 0x67, 0xae, 0x01, 0x12, 0x34, 0x56, 0x78,
    0x9a, 0xbc, 0xde, 0xf0, 0x01, 0x23, 0x45, 0x67,
  ]);
  const nameBytes = new TextEncoder().encode(name);
  const input = new Uint8Array(ns.length + nameBytes.length);
  input.set(ns);
  input.set(nameBytes, ns.length);

  const hashBuffer = await crypto.subtle.digest("SHA-1", input);
  const hash = new Uint8Array(hashBuffer);

  // Set version 5 (0101) in bits [76:79] of byte 6
  hash[6] = ((hash[6]!) & 0x0f) | 0x50;
  // Set variant bits (10xx) in byte 8
  hash[8] = ((hash[8]!) & 0x3f) | 0x80;

  const h = Buffer.from(hash.slice(0, 16)).toString("hex");
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20, 32)}`;
}
