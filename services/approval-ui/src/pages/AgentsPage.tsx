import { useState, useEffect } from "react";
import { Link, useRouter } from "../Router";
import { listAgents, checkHealth } from "../api";
import type { AgentSummary } from "../types";

type PageState =
	| { type: "loading" }
	| { type: "error"; message: string; isOffline: boolean }
	| { type: "loaded"; agents: AgentSummary[] };

export function AgentsPage() {
	const [state, setState] = useState<PageState>({ type: "loading" });

	useEffect(() => {
		loadAgents();
	}, []);

	async function loadAgents() {
		setState({ type: "loading" });
		const isHealthy = await checkHealth();
		if (!isHealthy) {
			setState({
				type: "error",
				message:
					"Unable to establish connection with AgentAuth registry.",
				isOffline: true,
			});
			return;
		}
		try {
			const agents = await listAgents();
			setState({ type: "loaded", agents });
		} catch (err) {
			setState({
				type: "error",
				message:
					err instanceof Error
						? err.message
						: "Failed to load agents",
				isOffline: false,
			});
		}
	}

	return (
		<div className="min-h-screen">
			{/* Top bar */}
			<div className="border-b border-border bg-panel">
				<div className="max-w-4xl mx-auto px-4 sm:px-6 h-12 flex items-center justify-between">
					<Link
						to="/"
						className="flex items-center gap-2 text-text-secondary hover:text-amber transition-colors"
					>
						<div className="w-4 h-4 border border-current flex items-center justify-center">
							<div className="w-1.5 h-1.5 bg-current" />
						</div>
						<span className="font-mono text-xs tracking-wide">
							AGENTAUTH
						</span>
					</Link>
					<span className="font-mono text-xs text-amber tracking-wide">
						AGENTS
					</span>
				</div>
			</div>

			<div className="max-w-4xl mx-auto px-4 sm:px-6 py-8">
				{state.type === "loading" && (
					<div className="animate-fade-in space-y-4">
						<div className="skeleton h-6 w-36" />
						<div className="skeleton h-4 w-64" />
						<div className="mt-6 space-y-3">
							{[1, 2, 3].map((i) => (
								<div key={i} className="skeleton h-24 w-full" />
							))}
						</div>
					</div>
				)}

				{state.type === "error" && (
					<div className="max-w-md mx-auto mt-16 animate-fade-in">
						<div
							className={`border ${state.isOffline ? "border-amber-dim bg-amber-glow" : "border-red-dim bg-red-glow"} p-6`}
						>
							<div className="flex items-start gap-3">
								<div
									className={`w-2 h-2 mt-1.5 ${state.isOffline ? "bg-amber" : "bg-red"} animate-pulse`}
								/>
								<div>
									<h2 className="font-mono text-sm font-medium tracking-wide text-text-primary mb-2">
										{state.isOffline
											? "CONNECTION LOST"
											: "ERROR"}
									</h2>
									<p className="text-text-secondary text-sm">
										{state.message}
									</p>
								</div>
							</div>
							<button
								onClick={loadAgents}
								className="mt-5 w-full py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
							>
								RETRY
							</button>
						</div>
					</div>
				)}

				{state.type === "loaded" && (
					<div className="animate-fade-in">
						{/* Header */}
						<div className="mb-6">
							<div className="flex items-center gap-2 mb-1">
								<div className="w-2 h-2 bg-amber" />
								<h1 className="font-mono text-lg tracking-tight text-text-primary">
									AGENTS
								</h1>
							</div>
							<p className="text-text-muted text-sm pl-4">
								{state.agents.length} registered agent
								{state.agents.length !== 1 ? "s" : ""}
							</p>
						</div>

						{state.agents.length === 0 ? (
							<div className="border border-border bg-panel p-12 text-center">
								<div className="w-3 h-3 bg-text-muted mx-auto mb-4" />
								<p className="font-mono text-sm text-text-secondary mb-1">
									NO AGENTS REGISTERED
								</p>
								<p className="text-text-muted text-sm">
									No agents have been authorized to act on
									your behalf.
								</p>
							</div>
						) : (
							<div className="space-y-2 stagger-children">
								{state.agents.map((agent) => (
									<AgentRow
										key={agent.agent_id}
										agent={agent}
									/>
								))}
							</div>
						)}
					</div>
				)}
			</div>
		</div>
	);
}

function AgentRow({ agent }: { agent: AgentSummary }) {
	const { navigate } = useRouter();
	const statusConfig = {
		active: { color: "bg-green", text: "text-green", label: "ACTIVE" },
		suspended: {
			color: "bg-amber",
			text: "text-amber",
			label: "SUSPENDED",
		},
		revoked: { color: "bg-red", text: "text-red", label: "REVOKED" },
	};

	const status = statusConfig[agent.status];

	return (
		<div
			className="border border-border bg-panel hover:bg-panel-hover hover:border-border-bright transition-all group"
			onClick={() => navigate(`/agents/${agent.agent_id}/activity`)}
		>
			<div className="px-4 py-4 flex items-center gap-4">
				{/* Status indicator */}
				<div className={`w-2 h-2 ${status.color} shrink-0`} />

				{/* Agent info — clickable area navigates to detail */}
				<button className="flex-1 min-w-0 text-left">
					<div className="flex items-center gap-3 mb-1">
						<span className="text-sm font-medium text-text-primary group-hover:text-amber transition-colors truncate">
							{agent.name}
						</span>
						<span
							className={`font-mono text-[10px] tracking-wide ${status.text}`}
						>
							{status.label}
						</span>
						{agent.pending_grant_id && (
							<span className="font-mono text-[10px] tracking-wide text-amber border border-amber-dim px-1.5 py-0.5 bg-amber-glow animate-pulse">
								PENDING APPROVAL
							</span>
						)}
					</div>
					<div className="font-mono text-[11px] text-text-muted truncate">
						{agent.agent_id}
					</div>
				</button>

				{/* Grants count */}
				<div className="text-right shrink-0">
					<div className="font-mono text-sm text-text-primary">
						{agent.active_grants}
					</div>
					<div className="font-mono text-[10px] text-text-muted">
						GRANTS
					</div>
				</div>

				{/* Approve button (pending) or arrow (normal) */}
				{agent.pending_grant_id ? (
					<Link
						to={`/approve/${agent.pending_grant_id}`}
						className="px-3 py-1.5 bg-amber text-surface font-mono text-[10px] tracking-wide hover:bg-amber-dim transition-colors shrink-0"
					>
						APPROVE
					</Link>
				) : (
					<svg
						width="16"
						height="16"
						viewBox="0 0 16 16"
						fill="none"
						className="text-text-muted group-hover:text-amber transition-colors shrink-0"
					>
						<path
							d="M6 4L10 8L6 12"
							stroke="currentColor"
							strokeWidth="1.5"
						/>
					</svg>
				)}
			</div>
		</div>
	);
}
