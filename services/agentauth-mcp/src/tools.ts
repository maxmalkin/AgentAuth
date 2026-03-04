import { z } from "zod/v4";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { authenticatedFetch } from "./agentauth.js";

const SERVICE_URL = process.env.SERVICE_URL ?? "http://localhost:9090";

interface AuthContext {
  grantId: string;
  privKey: Uint8Array;
  pubKey: Uint8Array;
}

async function callService(
  ctx: AuthContext,
  method: string,
  path: string,
  body?: unknown,
): Promise<{ status: number; data: unknown }> {
  const url = `${SERVICE_URL}${path}`;
  const res = await authenticatedFetch(method, url, body, ctx.grantId, ctx.privKey, ctx.pubKey);
  const data = await res.json().catch(() => ({ message: res.statusText }));
  return { status: res.status, data };
}

export function registerTools(server: McpServer, ctx: AuthContext): void {
  server.registerTool(
    "read_calendar",
    {
      title: "Read Calendar",
      description: "Read calendar events from the service. Requires the read/calendar capability.",
    },
    async () => {
      const { status, data } = await callService(ctx, "GET", "/calendar");
      if (status === 200) {
        return {
          content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
        };
      }
      return {
        content: [{ type: "text", text: `Error ${status}: ${JSON.stringify(data)}` }],
        isError: true,
      };
    },
  );

  server.registerTool(
    "write_file",
    {
      title: "Write File",
      description: "Write content to a file. Requires the write/files capability.",
      inputSchema: {
        filename: z.string().describe("Name of the file to write"),
        content: z.string().describe("Content to write to the file"),
      },
    },
    async ({ filename, content }) => {
      const { status, data } = await callService(ctx, "POST", "/files", { filename, content });
      if (status === 200) {
        return {
          content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
        };
      }
      return {
        content: [{ type: "text", text: `Error ${status}: ${JSON.stringify(data)}` }],
        isError: true,
      };
    },
  );

  server.registerTool(
    "delete_file",
    {
      title: "Delete File",
      description: "Delete a file by path. Requires the delete/files capability.",
      inputSchema: {
        path: z.string().describe("Path of the file to delete"),
      },
    },
    async ({ path }) => {
      const { status, data } = await callService(ctx, "DELETE", `/files/${encodeURIComponent(path)}`);
      if (status === 200) {
        return {
          content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
        };
      }
      return {
        content: [{ type: "text", text: `Error ${status}: ${JSON.stringify(data)}` }],
        isError: true,
      };
    },
  );

  server.registerTool(
    "make_payment",
    {
      title: "Make Payment",
      description:
        "Initiate a payment transaction. Requires the transact/payments capability. " +
        "This will be denied unless the grant explicitly includes the transact capability.",
      inputSchema: {
        amount: z.number().positive().describe("Payment amount"),
        recipient: z.string().describe("Payment recipient"),
        currency: z.string().default("USD").describe("Currency code (default: USD)"),
      },
    },
    async ({ amount, recipient, currency }) => {
      const { status, data } = await callService(ctx, "POST", "/payments", {
        amount,
        recipient,
        currency,
      });
      if (status === 200) {
        return {
          content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
        };
      }
      return {
        content: [
          {
            type: "text",
            text:
              status === 403
                ? `Payment denied (403): This agent was not granted the transact/payments capability.\n${JSON.stringify(data)}`
                : `Error ${status}: ${JSON.stringify(data)}`,
          },
        ],
        isError: true,
      };
    },
  );
}
