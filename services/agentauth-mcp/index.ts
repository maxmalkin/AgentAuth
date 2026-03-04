import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { loadState, initState, saveState, decodePrivKey } from "./src/state.js";
import { buildSignedManifest, getPublicKey, CAPABILITIES } from "./src/manifest.js";
import { register, requestOrLoadGrant, fetchDiscoveryIds } from "./src/agentauth.js";
import { registerTools } from "./src/tools.js";

async function main() {
  // All startup logs go to stderr — stdout is reserved for MCP JSON-RPC.
  let state = await loadState();

  if (!state) {
    console.error("[agentauth-mcp] First run — fetching demo seed IDs…");
    const { humanPrincipalId, serviceProviderId } = await fetchDiscoveryIds();
    state = await initState(humanPrincipalId, serviceProviderId);
    console.error(`[agentauth-mcp] Generated agent ID: ${state.agent_id}`);
  } else {
    console.error(`[agentauth-mcp] Loaded state for agent: ${state.agent_id}`);
  }

  const privKey = decodePrivKey(state);
  const pubKey = await getPublicKey(privKey);

  // Register (idempotent — safe on every startup).
  const signedManifest = await buildSignedManifest(state, privKey);
  await register(signedManifest);
  console.error(`[agentauth-mcp] Registered with registry`);

  // Request or resume grant, block until approved.
  const grantId = await requestOrLoadGrant(state, privKey, pubKey, CAPABILITIES);
  state.grant_id = grantId;
  state.grant_status = "approved";
  await saveState(state);

  console.error(`[agentauth-mcp] Ready — grant ${grantId} is approved`);

  // Start MCP server.
  const server = new McpServer({
    name: "agentauth-mcp",
    version: "0.1.0",
  });

  registerTools(server, { grantId, privKey, pubKey });

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("[agentauth-mcp] Fatal error:", err);
  process.exit(1);
});