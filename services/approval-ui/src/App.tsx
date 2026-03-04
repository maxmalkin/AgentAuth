import { Router, Link } from "./Router";
import {
	ApprovalPage,
	AgentsPage,
	AgentActivityPage,
	DashboardPage,
} from "./pages";

const routes = [
	{ pattern: "/", component: HomePage },
	{ pattern: "/approve/:grant_id", component: ApprovalPage },
	{ pattern: "/agents", component: AgentsPage },
	{ pattern: "/agents/:agent_id/activity", component: AgentActivityPage },
	{ pattern: "/dashboard", component: DashboardPage },
];

function LogoMark({ size = 20 }: { size?: number }) {
	return (
		<svg width={size} height={size} viewBox="0 0 20 20" fill="none">
			<path
				d="M10 2L18 6V14L10 18L2 14V6L10 2Z"
				stroke="currentColor"
				strokeWidth="1.5"
				className="text-amber"
			/>
			<circle
				cx="10"
				cy="10"
				r="3"
				stroke="currentColor"
				strokeWidth="1.5"
				className="text-amber"
			/>
		</svg>
	);
}

function SectionTag({ children }: { children: React.ReactNode }) {
	return (
		<div className="inline-flex items-center gap-2 mb-8">
			<div className="w-4 h-px bg-amber" />
			<span className="font-mono text-xs text-amber tracking-widest">{children}</span>
		</div>
	);
}

function CodeBlock({ children }: { children: React.ReactNode }) {
	return (
		<div className="bg-panel border border-border p-4 font-mono text-xs leading-relaxed">
			{children}
		</div>
	);
}

function C({ children }: { children: React.ReactNode }) {
	return <span className="text-text-muted">{children}</span>;
}

function Prompt({ children }: { children: React.ReactNode }) {
	return (
		<div>
			<span className="text-amber select-none">$ </span>
			<span className="text-text-secondary">{children}</span>
		</div>
	);
}

function HomePage() {
	return (
		<div className="min-h-screen">

			{/* ── Hero ─────────────────────────────────────────────── */}
			<div className="border-b border-border animate-fade-in">
				<div className="max-w-5xl mx-auto px-6 py-14">
					{/* Logo */}
					<div className="flex items-center gap-3 mb-5">
						<div className="w-10 h-10 border border-amber bg-amber-glow flex items-center justify-center shrink-0">
							<LogoMark />
						</div>
						<span className="font-mono text-2xl font-medium tracking-tight text-text-primary">
							AGENTAUTH
						</span>
					</div>

					<p className="text-text-primary text-base mb-2 max-w-xl">
						Human-in-the-loop authorization for AI agents.
					</p>
					<p className="text-text-secondary text-sm mb-8 max-w-xl leading-relaxed">
						Every tool call your agent makes is cryptographically signed, human-approved, and verified in real time — with a full audit trail.
					</p>

					{/* Primary actions */}
					<div className="flex flex-col sm:flex-row gap-3">
						<Link
							to="/agents"
							className="inline-flex items-center gap-2 px-6 py-3 bg-amber text-surface font-mono text-sm font-medium tracking-wide hover:bg-amber-dim transition-colors"
						>
							<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
								<rect x="2" y="2" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
								<rect x="9" y="2" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
								<rect x="2" y="9" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
								<rect x="9" y="9" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
							</svg>
							VIEW AGENTS
						</Link>
						<Link
							to="/dashboard"
							className="inline-flex items-center gap-2 px-6 py-3 border border-border text-text-secondary font-mono text-sm font-medium tracking-wide hover:border-amber hover:text-amber transition-colors"
						>
							<svg width="16" height="16" viewBox="0 0 16 16" fill="none">
								<path d="M2 12L5.5 6L8.5 9L14 3" stroke="currentColor" strokeWidth="1.5" />
								<path d="M2 14H14" stroke="currentColor" strokeWidth="1.5" />
							</svg>
							DASHBOARD
						</Link>
					</div>
				</div>
			</div>

			{/* ── How it works ─────────────────────────────────────── */}
			<div className="border-b border-border">
				<div className="max-w-5xl mx-auto px-6 py-14">
					<SectionTag>HOW IT WORKS</SectionTag>
					<div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-px bg-border">
						{[
							{
								n: "01",
								title: "REGISTER",
								body: "An agent generates an Ed25519 keypair and registers with the AgentAuth registry, declaring the capabilities it wants.",
							},
							{
								n: "02",
								title: "APPROVE",
								body: "A human reviews the capability request here and explicitly approves it. Nothing runs without this step.",
							},
							{
								n: "03",
								title: "AUTHENTICATE",
								body: "The agent issues short-lived tokens (15 min) bound to its private key via DPoP — stolen tokens can't be replayed.",
							},
							{
								n: "04",
								title: "VERIFY",
								body: "Your service verifies the token and capability in real time. Every outcome is appended to an immutable audit log.",
							},
						].map(({ n, title, body }) => (
							<div key={n} className="bg-surface p-6">
								<div className="font-mono text-xs text-amber mb-3">{n}</div>
								<div className="font-mono text-sm text-text-primary mb-2">{title}</div>
								<p className="text-text-secondary text-xs leading-relaxed">{body}</p>
							</div>
						))}
					</div>

					{/* Flow diagram */}
					<div className="mt-6 bg-panel border border-border p-5 font-mono text-xs text-text-muted overflow-x-auto">
						<div className="flex items-center gap-0 flex-wrap">
							{[
								{ label: "Claude Desktop", color: "text-text-secondary" },
								{ arrow: true },
								{ label: "agentauth-mcp", color: "text-amber" },
								{ arrow: true },
								{ label: "AgentAuth Registry :8080", color: "text-text-secondary" },
							].map((item, i) =>
								"arrow" in item ? (
									<span key={i} className="mx-3 text-border">──▶</span>
								) : (
									<span key={i} className={item.color}>{item.label}</span>
								)
							)}
						</div>
						<div className="mt-2 ml-[calc(50%-2px)] border-l border-border pl-4 py-1 text-text-muted">
							Authorization: AgentBearer &lt;token&gt;  ·  AgentDPoP: &lt;proof&gt;
						</div>
						<div className="flex items-center gap-0 mt-2 flex-wrap">
							{[
								{ label: "Your Service", color: "text-text-secondary" },
								{ arrow: true },
								{ label: "Verifier :8081", color: "text-text-secondary" },
								{ arrow: true },
								{ label: "allow / deny", color: "text-green" },
							].map((item, i) =>
								"arrow" in item ? (
									<span key={i} className="mx-3 text-border">──▶</span>
								) : (
									<span key={i} className={item.color}>{item.label}</span>
								)
							)}
						</div>
					</div>
				</div>
			</div>

			{/* ── Quick start ──────────────────────────────────────── */}
			<div className="border-b border-border">
				<div className="max-w-5xl mx-auto px-6 py-14">
					<SectionTag>QUICK START</SectionTag>
					<div className="grid grid-cols-1 md:grid-cols-2 gap-6">

						<div>
							<div className="font-mono text-sm text-text-primary mb-3">1 — Start the stack</div>
							<CodeBlock>
								<Prompt>git clone https://github.com/maxmalkin/AgentAuth</Prompt>
								<Prompt>cd AgentAuth && ./dev.sh</Prompt>
								<div className="mt-2">
									<C># registry      http://localhost:8080</C><br />
									<C># verifier      http://localhost:8081</C><br />
									<C># approval UI   http://localhost:3001</C><br />
									<C># mock service  http://localhost:9090</C>
								</div>
							</CodeBlock>
						</div>

						<div>
							<div className="font-mono text-sm text-text-primary mb-3">2 — Run the MCP agent</div>
							<CodeBlock>
								<Prompt>cd services/agentauth-mcp</Prompt>
								<Prompt>bun run index.ts</Prompt>
								<div className="mt-2 text-text-secondary">
									[agentauth-mcp] Registered with registry<br />
									[agentauth-mcp] Approve this agent at:<br />
									[agentauth-mcp]   <span className="text-amber">http://localhost:3001/approve/…</span><br />
									[agentauth-mcp] Waiting for approval…
								</div>
							</CodeBlock>
						</div>

						<div>
							<div className="font-mono text-sm text-text-primary mb-3">3 — Approve the grant</div>
							<p className="text-text-secondary text-xs leading-relaxed mb-3">
								Open the approval URL printed above. Review the capabilities the agent is requesting, then click <strong className="text-text-primary font-mono">APPROVE GRANT</strong>. The agent will start immediately.
							</p>
							<Link
								to="/agents"
								className="inline-flex items-center gap-2 px-4 py-2 border border-border text-text-secondary font-mono text-xs hover:border-amber hover:text-amber transition-colors"
							>
								<svg width="12" height="12" viewBox="0 0 16 16" fill="none">
									<rect x="2" y="2" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
									<rect x="9" y="2" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
									<rect x="2" y="9" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
									<rect x="9" y="9" width="5" height="5" stroke="currentColor" strokeWidth="1.5" />
								</svg>
								GO TO AGENTS
							</Link>
						</div>

						<div>
							<div className="font-mono text-sm text-text-primary mb-3">4 — Connect Claude Desktop</div>
							<p className="text-text-secondary text-xs leading-relaxed mb-3">
								Add to <span className="text-text-primary font-mono">claude_desktop_config.json</span> and restart Claude:
							</p>
							<CodeBlock>
								<span className="text-text-muted">{"{"}</span><br />
								&nbsp;&nbsp;<span className="text-blue">"mcpServers"</span>: {"{"}<br />
								&nbsp;&nbsp;&nbsp;&nbsp;<span className="text-blue">"agentauth"</span>: {"{"}<br />
								&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span className="text-blue">"command"</span>: <span className="text-green">"bun"</span>,<br />
								&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span className="text-blue">"args"</span>: [<span className="text-green">"/path/to/agentauth-mcp/index.ts"</span>],<br />
								&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span className="text-blue">"env"</span>: {"{"} <span className="text-blue">"REGISTRY_URL"</span>: <span className="text-green">"http://localhost:8080"</span> {"}"}<br />
								&nbsp;&nbsp;&nbsp;&nbsp;{"}"}<br />
								&nbsp;&nbsp;{"}"}<br />
								<span className="text-text-muted">{"}"}</span>
							</CodeBlock>
						</div>
					</div>
				</div>
			</div>

			{/* ── What's enforced ──────────────────────────────────── */}
			<div className="border-b border-border">
				<div className="max-w-5xl mx-auto px-6 py-14">
					<SectionTag>WHAT'S ENFORCED</SectionTag>
					<div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
						{[
							{
								title: "Capability grants",
								body: "Agents can only call what a human approved. read, write, delete, transact — each resource scoped separately.",
							},
							{
								title: "DPoP sender-constraint",
								body: "Tokens are cryptographically bound to the agent's private key. A stolen token is useless without the matching key.",
							},
							{
								title: "Short-lived tokens",
								body: "Tokens expire after 15 minutes. Refresh requires the original approved grant to still be valid.",
							},
							{
								title: "Nonce replay prevention",
								body: "Every DPoP proof carries a unique nonce checked against Redis. Replaying an old proof is rejected immediately.",
							},
							{
								title: "Behavioral envelope",
								body: "Per-grant rate limits, burst caps, time-of-day windows, and session duration are all enforced at the verifier.",
							},
							{
								title: "Immutable audit log",
								body: "Every token issue, verify, deny, and revoke is written to an append-only log with SHA-256 hash chain integrity.",
							},
						].map(({ title, body }) => (
							<div key={title} className="border border-border p-5">
								<div className="flex items-center gap-2 mb-2">
									<div className="w-1.5 h-1.5 bg-amber shrink-0" />
									<span className="font-mono text-sm text-text-primary">{title}</span>
								</div>
								<p className="text-text-secondary text-xs leading-relaxed">{body}</p>
							</div>
						))}
					</div>
				</div>
			</div>

			{/* ── Demo tools ───────────────────────────────────────── */}
			<div className="border-b border-border">
				<div className="max-w-5xl mx-auto px-6 py-14">
					<SectionTag>DEMO TOOLS</SectionTag>
					<p className="text-text-secondary text-sm mb-6 max-w-xl">
						The included MCP server exposes four tools. Ask Claude to use them after connecting.
					</p>
					<div className="border border-border divide-y divide-border">
						{[
							{ tool: "read_calendar", cap: "read / calendar", desc: "Read calendar events from the mock service.", allowed: true },
							{ tool: "write_file", cap: "write / files", desc: "Write content to a file on the mock service.", allowed: true },
							{ tool: "delete_file", cap: "delete / files", desc: "Delete a file by path on the mock service.", allowed: true },
							{ tool: "make_payment", cap: "transact / payments", desc: "Initiate a payment — denied unless the grant includes transact.", allowed: false },
						].map(({ tool, cap, desc, allowed }) => (
							<div key={tool} className="flex items-start gap-4 px-5 py-4">
								<div className={`mt-0.5 w-1.5 h-1.5 shrink-0 ${allowed ? "bg-green" : "bg-red"}`} />
								<div className="flex-1 min-w-0">
									<div className="flex items-center gap-3 mb-1 flex-wrap">
										<span className="font-mono text-sm text-text-primary">{tool}</span>
										<span className="font-mono text-xs text-text-muted border border-border px-1.5 py-0.5">{cap}</span>
										{!allowed && (
											<span className="font-mono text-xs text-red border border-red/30 px-1.5 py-0.5">DENIED BY DEFAULT</span>
										)}
									</div>
									<p className="text-text-secondary text-xs">{desc}</p>
								</div>
							</div>
						))}
					</div>
				</div>
			</div>

			{/* ── Footer ───────────────────────────────────────────── */}
			<div className="max-w-5xl mx-auto px-6 py-8 flex items-center justify-between flex-wrap gap-4">
				<div className="flex items-center gap-2">
					<div className="w-5 h-5 border border-border bg-amber-glow flex items-center justify-center">
						<LogoMark size={10} />
					</div>
					<span className="font-mono text-xs text-text-muted">AGENTAUTH</span>
				</div>
				<div className="flex items-center gap-6">
					<a
						href="https://github.com/maxmalkin/AgentAuth"
						target="_blank"
						rel="noopener noreferrer"
						className="font-mono text-xs text-text-muted hover:text-amber transition-colors"
					>
						GITHUB
					</a>
					<Link to="/agents" className="font-mono text-xs text-text-muted hover:text-amber transition-colors">
						AGENTS
					</Link>
					<Link to="/dashboard" className="font-mono text-xs text-text-muted hover:text-amber transition-colors">
						DASHBOARD
					</Link>
				</div>
			</div>

			{/* Background decoration */}
			<div className="fixed inset-0 pointer-events-none -z-10 overflow-hidden">
				<div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-150 h-150 border border-border/20 rotate-45 opacity-20" />
				<div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-100 h-100 border border-border/20 rotate-45 opacity-10" />
			</div>
		</div>
	);
}

function NotFound() {
	return (
		<div className="min-h-screen flex flex-col items-center justify-center px-4 text-center">
			<div className="font-mono text-6xl font-bold text-border mb-4">
				404
			</div>
			<p className="text-text-secondary mb-6 text-sm font-mono">
				SECTOR NOT FOUND
			</p>
			<Link
				to="/"
				className="inline-flex items-center gap-2 px-5 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
			>
				RETURN TO BASE
			</Link>
		</div>
	);
}

function App() {
	return (
		<div className="min-h-screen">
			<Router routes={routes} notFound={NotFound} />
		</div>
	);
}

export default App;
