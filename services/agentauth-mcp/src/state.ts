import { join } from "node:path";
import { mkdir } from "node:fs/promises";
import { uuidv7 } from "uuidv7";

export interface AgentState {
  agent_id: string;
  private_key_b64: string;
  human_principal_id: string;
  service_provider_id: string;
  grant_id: string | null;
  grant_status: "new" | "pending" | "approved";
}

const STATE_DIR = join(
  process.env.HOME ?? "~",
  ".config",
  "agentauth-mcp",
);
const STATE_FILE = join(STATE_DIR, "state.json");

export async function loadState(): Promise<AgentState | null> {
  try {
    const file = Bun.file(STATE_FILE);
    if (!(await file.exists())) return null;
    return (await file.json()) as AgentState;
  } catch {
    return null;
  }
}

export async function saveState(state: AgentState): Promise<void> {
  await mkdir(STATE_DIR, { recursive: true });
  await Bun.write(STATE_FILE, JSON.stringify(state, null, 2));
}

export async function initState(
  humanPrincipalId: string,
  serviceProviderId: string,
): Promise<AgentState> {
  // Generate a fresh 32-byte Ed25519 private key seed.
  const privKey = crypto.getRandomValues(new Uint8Array(32));
  const state: AgentState = {
    agent_id: uuidv7(),
    private_key_b64: Buffer.from(privKey).toString("base64url"),
    human_principal_id: humanPrincipalId,
    service_provider_id: serviceProviderId,
    grant_id: null,
    grant_status: "new",
  };
  await saveState(state);
  return state;
}

export function decodePrivKey(state: AgentState): Uint8Array {
  return Buffer.from(state.private_key_b64, "base64url");
}
