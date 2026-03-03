import index from "./index.html";

const PORT = process.env.PORT || 3000;

Bun.serve({
  port: PORT,
  routes: {
    "/": index,
    "/approve/:grant_id": index,
    "/agents": index,
    "/agents/:agent_id/activity": index,
    "/dashboard": index,
  },
  development: {
    hmr: true,
    console: true,
  },
});

console.log(`Approval UI running at http://localhost:${PORT}`);
