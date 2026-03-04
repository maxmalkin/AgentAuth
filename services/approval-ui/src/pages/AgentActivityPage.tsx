import { useState, useEffect } from "react";
import { useParams, Link, useRouter } from "../Router";
import {
	getAgentDetails,
	getAgentActivity,
	revokeAgent,
	revokeGrant,
	checkHealth,
} from "../api";
import { capabilityToHumanReadable } from "../utils/capabilities";
import type { AgentDetails, AuditEvent, GrantSummary } from "../types";

type PageState =
	| { type: "loading" }
	| { type: "error"; message: string; isOffline: boolean }
	| { type: "loaded"; agent: AgentDetails; events: AuditEvent[] };

export function AgentActivityPage() {
	const { agent_id } = useParams<{ agent_id: string }>();
	const { navigate } = useRouter();
	const [state, setState] = useState<PageState>({ type: "loading" });
	const [showRevokeConfirm, setShowRevokeConfirm] = useState(false);
	const [revokeGrantId, setRevokeGrantId] = useState<string | null>(null);

	useEffect(() => {
		loadAgent();
	}, [agent_id]);

	async function loadAgent() {
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
			const [agent, events] = await Promise.all([
				getAgentDetails(agent_id),
				getAgentActivity(agent_id),
			]);
			setState({ type: "loaded", agent, events });
		} catch (err) {
			setState({
				type: "error",
				message:
					err instanceof Error
						? err.message
						: "Failed to load agent details",
				isOffline: false,
			});
		}
	}

	async function handleRevokeAgent() {
		try {
			await revokeAgent(agent_id);
			navigate("/agents");
		} catch (err) {
			setState({
				type: "error",
				message:
					err instanceof Error
						? err.message
						: "Failed to revoke agent",
				isOffline: false,
			});
		}
	}

	async function handleRevokeGrant(grantId: string) {
		try {
			await revokeGrant(grantId);
			setRevokeGrantId(null);
			loadAgent();
		} catch (err) {
			setState({
				type: "error",
				message:
					err instanceof Error
						? err.message
						: "Failed to revoke grant",
				isOffline: false,
			});
		}
	}

	const statusConfig = {
		active: { color: "bg-green", text: "text-green", label: "ACTIVE" },
		suspended: {
			color: "bg-amber",
			text: "text-amber",
			label: "SUSPENDED",
		},
		revoked: { color: "bg-red", text: "text-red", label: "REVOKED" },
	};

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
					<div className="flex items-center gap-2 font-mono text-xs text-text-muted">
						<Link
							to="/agents"
							className="hover:text-text-secondary transition-colors"
						>
							AGENTS
						</Link>
						<span>/</span>
						<span className="text-amber">DETAIL</span>
					</div>
				</div>
			</div>

			<div className="max-w-4xl mx-auto px-4 sm:px-6 py-8">
				{/* Loading */}
				{state.type === "loading" && (
					<div className="animate-fade-in space-y-4">
						<div className="skeleton h-6 w-48" />
						<div className="skeleton h-4 w-72" />
						<div className="mt-6 grid grid-cols-2 gap-3">
							<div className="skeleton h-20" />
							<div className="skeleton h-20" />
						</div>
						<div className="mt-4 skeleton h-40" />
					</div>
				)}

				{/* Error */}
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
							<div className="flex gap-3 mt-5">
								<button
									onClick={loadAgent}
									className="flex-1 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
								>
									RETRY
								</button>
								<Link
									to="/agents"
									className="flex-1 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-text-primary hover:text-text-primary transition-colors text-center"
								>
									BACK
								</Link>
							</div>
						</div>
					</div>
				)}

				{/* Loaded */}
				{state.type === "loaded" &&
					(() => {
						const { agent, events } = state;
						const status = statusConfig[agent.status];

						return (
							<div className="animate-fade-in">
								{/* Back link */}
								<Link
									to="/agents"
									className="inline-flex items-center gap-1.5 text-text-muted hover:text-amber font-mono text-xs tracking-wide transition-colors mb-6"
								>
									<svg
										width="12"
										height="12"
										viewBox="0 0 12 12"
										fill="none"
									>
										<path
											d="M8 2L4 6L8 10"
											stroke="currentColor"
											strokeWidth="1.5"
										/>
									</svg>
									AGENTS
								</Link>

								{/* Header */}
								<div className="mb-8">
									<div className="flex items-center gap-3 mb-1">
										<div
											className={`w-2 h-2 ${status.color}`}
										/>
										<h1 className="font-mono text-lg tracking-tight text-text-primary">
											{agent.name}
										</h1>
										<span
											className={`font-mono text-[10px] tracking-wide ${status.text}`}
										>
											{status.label}
										</span>
									</div>
									<p className="font-mono text-[11px] text-text-muted pl-5">
										{agent.agent_id}
									</p>
								</div>

								{/* Details grid */}
								<div className="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-8 stagger-children">
									<div className="border border-border bg-panel p-4">
										<div className="font-mono text-[10px] tracking-widest text-text-muted mb-1">
											REGISTERED
										</div>
										<div className="text-sm text-text-primary">
											{new Date(
												agent.registered_at,
											).toLocaleDateString()}
										</div>
									</div>
									<div className="border border-border bg-panel p-4">
										<div className="font-mono text-[10px] tracking-widest text-text-muted mb-1">
											ACTIVE GRANTS
										</div>
										<div className="text-sm text-text-primary font-mono">
											{
												agent.grants.filter(
													(g) =>
														g.status === "active",
												).length
											}
										</div>
									</div>
									<div className="border border-border bg-panel p-4">
										<div className="font-mono text-[10px] tracking-widest text-text-muted mb-1">
											PUBLIC KEY
										</div>
										<div className="text-sm text-text-primary font-mono truncate">
											{agent.public_key.slice(0, 24)}...
										</div>
									</div>
								</div>

								{/* Grants */}
								<div className="mb-8">
									<SectionLabel>
										GRANTS ({agent.grants.length})
									</SectionLabel>
									{agent.grants.length === 0 ? (
										<div className="border border-border bg-panel p-6 text-center">
											<p className="text-text-muted text-sm font-mono">
												NO ACTIVE GRANTS
											</p>
										</div>
									) : (
										<div className="space-y-2 stagger-children">
											{agent.grants.map((grant) => (
												<GrantRow
													key={grant.grant_id}
													grant={grant}
													onRevoke={() =>
														setRevokeGrantId(
															grant.grant_id,
														)
													}
												/>
											))}
										</div>
									)}
								</div>

								{/* Activity */}
								<div className="mb-8">
									<SectionLabel>
										RECENT ACTIVITY ({events.length})
									</SectionLabel>
									{events.length === 0 ? (
										<div className="border border-border bg-panel p-6 text-center">
											<p className="text-text-muted text-sm font-mono">
												NO RECENT ACTIVITY
											</p>
										</div>
									) : (
										<div className="border border-border divide-y divide-border stagger-children">
											{events.map((event) => (
												<ActivityRow
													key={event.event_id}
													event={event}
												/>
											))}
										</div>
									)}
								</div>

								{/* Danger zone */}
								{agent.status === "active" && (
									<div className="border border-red-dim bg-red-glow p-5">
										<div className="flex items-center gap-2 mb-2">
											<div className="w-2 h-2 bg-red" />
											<span className="font-mono text-xs tracking-wide text-red">
												DANGER ZONE
											</span>
										</div>
										<p className="text-text-secondary text-sm mb-4 pl-4">
											Revoking this agent will immediately
											terminate all access and invalidate
											all active tokens.
										</p>
										<button
											onClick={() =>
												setShowRevokeConfirm(true)
											}
											className="ml-4 px-4 py-2 border border-red text-red font-mono text-xs tracking-wide hover:bg-red hover:text-white transition-colors"
										>
											REVOKE AGENT
										</button>
									</div>
								)}
							</div>
						);
					})()}

				{/* Revoke agent dialog */}
				{showRevokeConfirm && state.type === "loaded" && (
					<ConfirmDialog
						title="REVOKE AGENT"
						message={`Are you sure you want to revoke ${state.agent.name}? This will immediately terminate all access.`}
						confirmLabel="REVOKE"
						onConfirm={handleRevokeAgent}
						onCancel={() => setShowRevokeConfirm(false)}
					/>
				)}

				{/* Revoke grant dialog */}
				{revokeGrantId && (
					<ConfirmDialog
						title="REVOKE GRANT"
						message="Are you sure you want to revoke this grant? The agent will lose access to this service."
						confirmLabel="REVOKE"
						onConfirm={() => handleRevokeGrant(revokeGrantId)}
						onCancel={() => setRevokeGrantId(null)}
					/>
				)}
			</div>
		</div>
	);
}

function SectionLabel({ children }: { children: React.ReactNode }) {
	return (
		<div className="font-mono text-[10px] tracking-widest text-text-muted mb-2 flex items-center gap-2">
			<span>{children}</span>
			<div className="flex-1 h-px bg-border" />
		</div>
	);
}

function GrantRow({
	grant,
	onRevoke,
}: {
	grant: GrantSummary;
	onRevoke: () => void;
}) {
	const grantStatus: Record<
		string,
		{ color: string; text: string; label: string }
	> = {
		active: { color: "bg-green", text: "text-green", label: "ACTIVE" },
		approved: { color: "bg-green", text: "text-green", label: "APPROVED" },
		pending: { color: "bg-amber", text: "text-amber", label: "PENDING" },
		denied: { color: "bg-red", text: "text-red", label: "DENIED" },
		revoked: { color: "bg-red", text: "text-red", label: "REVOKED" },
		expired: {
			color: "bg-text-muted",
			text: "text-text-muted",
			label: "EXPIRED",
		},
	};

	const fallback = {
		color: "bg-text-muted",
		text: "text-text-muted",
		label: grant.status.toUpperCase(),
	};
	const status = grantStatus[grant.status] ?? fallback;

	return (
		<div className="border border-border bg-panel hover:bg-panel-hover transition-colors">
			<div className="px-4 py-3 flex items-start justify-between gap-4">
				<div className="min-w-0 flex-1">
					<div className="flex items-center gap-2 mb-1">
						<div
							className={`w-1.5 h-1.5 ${status.color} shrink-0`}
						/>
						<span className="text-sm font-medium text-text-primary truncate">
							{grant.service_provider_name}
						</span>
						<span
							className={`font-mono text-[10px] tracking-wide ${status.text}`}
						>
							{status.label}
						</span>
					</div>
					<div className="pl-3.5 space-y-0.5">
						{grant.capabilities.map((cap, idx) => (
							<div
								key={idx}
								className="text-xs text-text-secondary"
							>
								{capabilityToHumanReadable(cap)}
							</div>
						))}
					</div>
					<div className="pl-3.5 mt-1 font-mono text-[10px] text-text-muted">
						{new Date(grant.created_at).toLocaleDateString()}
					</div>
				</div>
				<div className="flex items-center gap-2 shrink-0">
					{grant.status === "pending" && (
						<Link
							to={`/approve/${grant.grant_id}`}
							className="px-3 py-1.5 bg-amber text-surface font-mono text-[10px] tracking-wide hover:bg-amber-dim transition-colors"
						>
							APPROVE
						</Link>
					)}
					{(grant.status === "active" ||
						grant.status === "approved") && (
						<button
							onClick={(e) => {
								e.stopPropagation();
								onRevoke();
							}}
							className="px-3 py-1.5 border border-border text-text-muted font-mono text-[10px] tracking-wide hover:border-red hover:text-red transition-colors"
						>
							REVOKE
						</button>
					)}
				</div>
			</div>
		</div>
	);
}

function ActivityRow({ event }: { event: AuditEvent }) {
	const eventLabels: Record<string, string> = {
		token_issued: "Token Issued",
		token_verified: "Token Verified",
		token_denied: "Token Denied",
		grant_approved: "Grant Approved",
		grant_denied: "Grant Denied",
		agent_registered: "Agent Registered",
		agent_revoked: "Agent Revoked",
	};

	const outcomeConfig: Record<string, { color: string; label: string }> = {
		allowed: { color: "text-green", label: "OK" },
		denied: { color: "text-red", label: "DENIED" },
		rate_limited: { color: "text-amber", label: "THROTTLED" },
	};

	const outcome = outcomeConfig[event.outcome] || {
		color: "text-text-muted",
		label: event.outcome,
	};

	return (
		<div className="px-4 py-3 flex items-center gap-4 bg-panel hover:bg-panel-hover transition-colors">
			<div className="font-mono text-[11px] text-text-muted w-36 shrink-0">
				{new Date(event.created_at).toLocaleString()}
			</div>
			<div className="flex-1 min-w-0 flex items-center gap-2 flex-wrap">
				<span className="text-sm text-text-primary">
					{eventLabels[event.event_type] || event.event_type}
				</span>
				{event.capability && (
					<span className="font-mono text-[11px] text-text-muted bg-panel-raised border border-border px-1.5 py-0.5 truncate max-w-48">
						{capabilityToHumanReadable(event.capability)}
					</span>
				)}
			</div>
			<span
				className={`font-mono text-[10px] tracking-wide ${outcome.color} shrink-0`}
			>
				{outcome.label}
			</span>
		</div>
	);
}

function ConfirmDialog({
	title,
	message,
	confirmLabel,
	onConfirm,
	onCancel,
}: {
	title: string;
	message: string;
	confirmLabel: string;
	onConfirm: () => void;
	onCancel: () => void;
}) {
	return (
		<div className="fixed inset-0 bg-surface/80 backdrop-blur-sm flex items-center justify-center p-4 z-50 animate-fade-in">
			<div className="border border-red-dim bg-panel-raised max-w-md w-full p-6 animate-slide-up">
				<div className="flex items-center gap-2 mb-4">
					<div className="w-2 h-2 bg-red" />
					<h3 className="font-mono text-sm tracking-wide text-text-primary">
						{title}
					</h3>
				</div>
				<p className="text-text-secondary text-sm mb-6 leading-relaxed">
					{message}
				</p>
				<div className="flex gap-3 justify-end">
					<button
						onClick={onCancel}
						className="px-4 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-text-primary hover:text-text-primary transition-colors"
					>
						CANCEL
					</button>
					<button
						onClick={onConfirm}
						className="px-4 py-2.5 bg-red-dim border border-red text-red font-mono text-xs tracking-wide hover:bg-red hover:text-white transition-colors"
					>
						{confirmLabel}
					</button>
				</div>
			</div>
		</div>
	);
}
