import * as ed from "@noble/ed25519";
import type { AgentState } from "./state.js";

export interface Capability {
  type: "read" | "write" | "delete" | "transact" | "custom";
  resource: string;
  max_value?: number;
  currency?: string;
  filter?: string;
}

export interface SignedManifest {
  manifest: Record<string, unknown>;
  signature: string;
  signing_key_id: string;
}

const KEY_ID = "mcp-key-001";

/** Format a Date as RFC 3339 with second precision (no milliseconds).
 *  Matches Rust's chrono serde output (SecondsFormat::AutoSi with use_z=true). */
function toRfc3339Secs(d: Date): string {
  // Truncate to whole seconds, then strip the ".000Z" that toISOString always adds.
  const secs = new Date(Math.floor(d.getTime() / 1000) * 1000);
  return secs.toISOString().replace(".000Z", "Z");
}

/** Sort all object keys alphabetically, recursively.
 *  serde_json::to_value() uses BTreeMap internally so canonical bytes have sorted keys. */
function sortKeysDeep(val: unknown): unknown {
  if (Array.isArray(val)) return val.map(sortKeysDeep);
  if (val !== null && typeof val === "object") {
    const obj = val as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(obj)
        .sort()
        .map((k) => [k, sortKeysDeep(obj[k])]),
    );
  }
  return val;
}

const CAPABILITIES: Capability[] = [
  { type: "read", resource: "calendar" },
  { type: "write", resource: "files" },
  { type: "delete", resource: "files" },
  { type: "transact", resource: "payments", max_value: 10000, currency: "USD" },
];

export async function buildSignedManifest(
  state: AgentState,
  privKey: Uint8Array,
): Promise<SignedManifest> {
  const pubKey = await ed.getPublicKeyAsync(privKey);
  const pubKeyB64 = Buffer.from(pubKey).toString("base64url");

  const now = new Date();
  const expiresAt = new Date(now.getTime() + 90 * 86400 * 1000); // 90 days

  const manifest = {
    id: state.agent_id,
    public_key: pubKeyB64,
    key_id: KEY_ID,
    capabilities_requested: CAPABILITIES,
    human_principal_id: state.human_principal_id,
    issued_at: toRfc3339Secs(now),
    expires_at: toRfc3339Secs(expiresAt),
    name: "AgentAuth MCP",
    description: "Claude Desktop MCP server authenticated via AgentAuth",
    model_origin: "anthropic.com",
  };

  // Sign the alphabetically-sorted JSON to match Rust's serde_json canonical bytes.
  const manifestBytes = new TextEncoder().encode(JSON.stringify(sortKeysDeep(manifest)));
  const signature = await ed.signAsync(manifestBytes, privKey);

  return {
    manifest,
    signature: Buffer.from(signature).toString("base64url"),
    signing_key_id: KEY_ID,
  };
}

export async function getPublicKey(privKey: Uint8Array): Promise<Uint8Array> {
  return ed.getPublicKeyAsync(privKey);
}

export { CAPABILITIES };
