import { useState, useEffect } from 'react';
import { useParams, useRouter, Link } from '../Router';
import { getGrantRequest, approveGrant, denyGrant, checkHealth } from '../api';
import { signApprovalAssertion, isWebAuthnSupported } from '../utils/webauthn';
import {
  capabilityToHumanReadable,
  envelopeToHumanReadable,
  requiresTwoStep,
  getCapabilityRiskLevel,
  getCapabilitySummary,
} from '../utils/capabilities';
import type { GrantRequest, Capability, ApprovalAssertion } from '../types';

type PageState =
  | { type: 'loading' }
  | { type: 'error'; message: string; isOffline: boolean }
  | { type: 'loaded'; grant: GrantRequest }
  | { type: 'confirming'; grant: GrantRequest; step: 1 | 2 }
  | { type: 'signing'; grant: GrantRequest }
  | { type: 'success'; action: 'approved' | 'denied' }
  | { type: 'expired' };

export function ApprovalPage() {
  const { grant_id } = useParams<{ grant_id: string }>();
  const { navigate } = useRouter();
  const [state, setState] = useState<PageState>({ type: 'loading' });
  const [denyReason, setDenyReason] = useState('');

  useEffect(() => {
    loadGrant();
  }, [grant_id]);

  async function loadGrant() {
    setState({ type: 'loading' });
    const isHealthy = await checkHealth();
    if (!isHealthy) {
      setState({
        type: 'error',
        message: 'Unable to establish connection with AgentAuth registry. Service may be offline.',
        isOffline: true,
      });
      return;
    }
    try {
      const grant = await getGrantRequest(grant_id);
      if (grant.status === 'expired') {
        setState({ type: 'expired' });
        return;
      }
      if (grant.status !== 'pending') {
        setState({
          type: 'error',
          message: `This grant request has already been ${grant.status}.`,
          isOffline: false,
        });
        return;
      }
      setState({ type: 'loaded', grant });
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to load grant request',
        isOffline: false,
      });
    }
  }

  function handleApproveClick(grant: GrantRequest) {
    const summary = getCapabilitySummary(grant.requested_capabilities);
    if (summary.hasHighRisk) {
      setState({ type: 'confirming', grant, step: 1 });
    } else {
      startSigning(grant);
    }
  }

  function handleConfirmStep1(grant: GrantRequest) {
    setState({ type: 'confirming', grant, step: 2 });
  }

  async function startSigning(grant: GrantRequest) {
    if (!isWebAuthnSupported()) {
      setState({
        type: 'error',
        message: 'WebAuthn/Passkeys not supported. Use a compatible browser.',
        isOffline: false,
      });
      return;
    }
    setState({ type: 'signing', grant });
    try {
      const assertion: ApprovalAssertion = {
        grant_id: grant.grant_id,
        agent_id: grant.agent_id,
        granted_capabilities: grant.requested_capabilities,
        behavioral_envelope: grant.requested_envelope,
        approved_at: new Date().toISOString(),
        approval_nonce: crypto.randomUUID(),
      };
      const signature = await signApprovalAssertion(assertion);
      await approveGrant(grant.grant_id, assertion, signature);
      setState({ type: 'success', action: 'approved' });
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Signing failed',
        isOffline: false,
      });
    }
  }

  async function handleDeny(grant: GrantRequest) {
    try {
      await denyGrant(grant.grant_id, denyReason || undefined);
      setState({ type: 'success', action: 'denied' });
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to deny request',
        isOffline: false,
      });
    }
  }

  // --- Loading ---
  if (state.type === 'loading') {
    return (
      <Shell>
        <div className="space-y-4 animate-fade-in">
          <div className="skeleton h-6 w-48" />
          <div className="skeleton h-4 w-72" />
          <div className="mt-8 space-y-3">
            <div className="skeleton h-20 w-full" />
            <div className="skeleton h-20 w-full" />
            <div className="skeleton h-32 w-full" />
          </div>
        </div>
      </Shell>
    );
  }

  // --- Error ---
  if (state.type === 'error') {
    return (
      <Shell>
        <div className="max-w-md mx-auto mt-16 animate-fade-in">
          <div className={`border ${state.isOffline ? 'border-amber-dim bg-amber-glow' : 'border-red-dim bg-red-glow'} p-6`}>
            <div className="flex items-start gap-3">
              <div className={`w-2 h-2 mt-1.5 ${state.isOffline ? 'bg-amber' : 'bg-red'} animate-pulse`} />
              <div>
                <h2 className="font-mono text-sm font-medium tracking-wide text-text-primary mb-2">
                  {state.isOffline ? 'CONNECTION LOST' : 'ERROR'}
                </h2>
                <p className="text-text-secondary text-sm leading-relaxed">{state.message}</p>
              </div>
            </div>
            <button
              onClick={loadGrant}
              className="mt-5 w-full py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
            >
              RETRY
            </button>
          </div>
        </div>
      </Shell>
    );
  }

  // --- Expired ---
  if (state.type === 'expired') {
    return (
      <Shell>
        <div className="max-w-md mx-auto mt-16 text-center animate-fade-in">
          <div className="border border-border bg-panel p-8">
            <div className="w-3 h-3 bg-text-muted mx-auto mb-4" />
            <h2 className="font-mono text-sm tracking-wide text-text-secondary mb-2">REQUEST EXPIRED</h2>
            <p className="text-text-muted text-sm mb-6">This grant request has expired and can no longer be processed.</p>
            <button
              onClick={() => navigate('/agents')}
              className="px-5 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
            >
              VIEW AGENTS
            </button>
          </div>
        </div>
      </Shell>
    );
  }

  // --- Success ---
  if (state.type === 'success') {
    const approved = state.action === 'approved';
    return (
      <Shell>
        <div className="max-w-md mx-auto mt-16 text-center animate-slide-up">
          <div className={`border ${approved ? 'border-green-dim bg-green-glow' : 'border-border bg-panel'} p-8`}>
            <div className={`w-3 h-3 ${approved ? 'bg-green' : 'bg-text-muted'} mx-auto mb-4`} />
            <h2 className="font-mono text-sm tracking-wide text-text-primary mb-2">
              {approved ? 'GRANT AUTHORIZED' : 'REQUEST DENIED'}
            </h2>
            <p className="text-text-secondary text-sm mb-6">
              {approved
                ? 'Agent access has been granted. Token issuance is now active.'
                : 'The request has been denied. The agent will be notified.'}
            </p>
            <button
              onClick={() => navigate('/agents')}
              className="px-5 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-amber hover:text-amber transition-colors"
            >
              VIEW AGENTS
            </button>
          </div>
        </div>
      </Shell>
    );
  }

  // --- Signing ---
  if (state.type === 'signing') {
    return (
      <Shell>
        <div className="max-w-md mx-auto mt-16 text-center animate-fade-in">
          <div className="border border-blue-dim bg-panel p-8">
            <div className="flex items-center justify-center gap-1.5 mb-4">
              <div className="w-1.5 h-1.5 bg-blue rounded-full" style={{ animation: 'pulse-glow 1s ease-in-out infinite' }} />
              <div className="w-1.5 h-1.5 bg-blue rounded-full" style={{ animation: 'pulse-glow 1s ease-in-out 0.2s infinite' }} />
              <div className="w-1.5 h-1.5 bg-blue rounded-full" style={{ animation: 'pulse-glow 1s ease-in-out 0.4s infinite' }} />
            </div>
            <h2 className="font-mono text-sm tracking-wide text-text-primary mb-2">AUTHENTICATING</h2>
            <p className="text-text-secondary text-sm">Complete verification with your passkey.</p>
          </div>
        </div>
      </Shell>
    );
  }

  // --- Loaded / Confirming ---
  const grant = state.grant;
  const isConfirming = state.type === 'confirming';
  const confirmStep = isConfirming ? state.step : 0;
  const summary = getCapabilitySummary(grant.requested_capabilities);

  return (
    <Shell>
      <div className="animate-fade-in">
        {/* Header */}
        <div className="mb-8">
          <div className="flex items-center gap-2 mb-1">
            <div className={`w-2 h-2 ${summary.hasHighRisk ? 'bg-red' : 'bg-amber'}`} />
            <h1 className="font-mono text-lg tracking-tight text-text-primary">
              GRANT REQUEST
            </h1>
          </div>
          <p className="font-mono text-xs text-text-muted pl-4">{grant.grant_id}</p>
        </div>

        {/* Agent + Service grid */}
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 mb-6 stagger-children">
          <InfoBlock label="REQUESTING AGENT" value={grant.agent_name} sub={grant.agent_id} />
          <InfoBlock label="TARGET SERVICE" value={grant.service_provider_name} sub={grant.service_provider_id} />
        </div>

        {/* Capabilities */}
        <div className="mb-6">
          <SectionLabel>REQUESTED PERMISSIONS ({grant.requested_capabilities.length})</SectionLabel>
          <div className="border border-border divide-y divide-border stagger-children">
            {grant.requested_capabilities.map((cap, idx) => (
              <CapabilityRow key={idx} capability={cap} />
            ))}
          </div>
        </div>

        {/* Behavioral constraints */}
        <div className="mb-6">
          <SectionLabel>BEHAVIORAL CONSTRAINTS</SectionLabel>
          <div className="border border-border bg-panel p-4 space-y-2">
            {envelopeToHumanReadable(grant.requested_envelope).map((desc, idx) => (
              <div key={idx} className="flex items-start gap-2">
                <span className="text-text-muted mt-0.5 text-xs">{'>'}</span>
                <span className="text-sm text-text-secondary">{desc}</span>
              </div>
            ))}
          </div>
        </div>

        {/* Expiry */}
        <div className="mb-8 border border-amber-dim/50 bg-amber-glow px-4 py-3 flex items-center gap-3">
          <div className="w-1.5 h-1.5 bg-amber" />
          <span className="text-sm text-amber font-mono">
            EXPIRES {new Date(grant.expires_at).toLocaleString().toUpperCase()}
          </span>
        </div>

        {/* Actions */}
        {!isConfirming && (
          <div className="border-t border-border pt-6 flex flex-col sm:flex-row items-stretch sm:items-center justify-between gap-4">
            <div className="flex items-center gap-2">
              <input
                type="text"
                placeholder="Denial reason (optional)"
                value={denyReason}
                onChange={(e) => setDenyReason(e.target.value)}
                className="bg-panel border border-border px-3 py-2 text-sm text-text-primary placeholder:text-text-muted font-mono focus:outline-none focus:border-amber w-56"
              />
              <button
                onClick={() => handleDeny(grant)}
                className="px-4 py-2 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-red hover:text-red transition-colors whitespace-nowrap"
              >
                DENY
              </button>
            </div>
            <button
              onClick={() => handleApproveClick(grant)}
              className="px-6 py-3 bg-amber text-surface font-mono text-sm font-medium tracking-wide hover:bg-amber-dim transition-colors"
            >
              APPROVE WITH PASSKEY
            </button>
          </div>
        )}
      </div>

      {/* Confirmation overlay */}
      {isConfirming && (
        <div className="fixed inset-0 bg-surface/80 backdrop-blur-sm flex items-center justify-center p-4 z-50 animate-fade-in">
          <div className="border border-border bg-panel-raised max-w-lg w-full p-6 animate-slide-up">
            {confirmStep === 1 ? (
              <>
                <div className="flex items-center gap-2 mb-4">
                  <div className="w-2 h-2 bg-red animate-pulse" />
                  <h3 className="font-mono text-sm tracking-wide text-text-primary">HIGH-RISK PERMISSIONS</h3>
                </div>
                <p className="text-text-secondary text-sm mb-4 leading-relaxed">
                  This request includes permissions that could modify or delete your data, or make financial transactions.
                </p>
                <p className="text-text-primary text-sm font-medium mb-6">Proceed with caution.</p>
                <div className="flex gap-3 justify-end">
                  <button
                    onClick={() => setState({ type: 'loaded', grant })}
                    className="px-4 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-text-primary hover:text-text-primary transition-colors"
                  >
                    CANCEL
                  </button>
                  <button
                    onClick={() => handleConfirmStep1(grant)}
                    className="px-4 py-2.5 bg-amber-dim border border-amber text-amber font-mono text-xs tracking-wide hover:bg-amber hover:text-surface transition-colors"
                  >
                    CONTINUE
                  </button>
                </div>
              </>
            ) : (
              <>
                <div className="flex items-center gap-2 mb-4">
                  <div className="w-2 h-2 bg-red" />
                  <h3 className="font-mono text-sm tracking-wide text-text-primary">FINAL CONFIRMATION</h3>
                </div>
                <p className="text-text-secondary text-sm mb-3">You are granting access to:</p>
                <div className="border border-red-dim bg-red-glow divide-y divide-red-dim/50 mb-4">
                  {grant.requested_capabilities
                    .filter(requiresTwoStep)
                    .map((cap, idx) => (
                      <div key={idx} className="px-3 py-2 text-sm text-red">
                        {capabilityToHumanReadable(cap)}
                      </div>
                    ))}
                </div>
                <p className="text-text-muted text-xs mb-6 font-mono">
                  This cannot be undone without revoking the entire grant.
                </p>
                <div className="flex gap-3 justify-end">
                  <button
                    onClick={() => setState({ type: 'loaded', grant })}
                    className="px-4 py-2.5 border border-border text-text-secondary font-mono text-xs tracking-wide hover:border-text-primary hover:text-text-primary transition-colors"
                  >
                    CANCEL
                  </button>
                  <button
                    onClick={() => startSigning(grant)}
                    className="px-4 py-2.5 bg-red-dim border border-red text-red font-mono text-xs tracking-wide hover:bg-red hover:text-white transition-colors"
                  >
                    APPROVE WITH PASSKEY
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </Shell>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen">
      {/* Top bar */}
      <div className="border-b border-border bg-panel">
        <div className="max-w-3xl mx-auto px-4 sm:px-6 h-12 flex items-center justify-between">
          <Link to="/" className="flex items-center gap-2 text-text-secondary hover:text-amber transition-colors">
            <div className="w-4 h-4 border border-current flex items-center justify-center">
              <div className="w-1.5 h-1.5 bg-current" />
            </div>
            <span className="font-mono text-xs tracking-wide">AGENTAUTH</span>
          </Link>
          <Link to="/agents" className="font-mono text-xs text-text-muted hover:text-text-secondary transition-colors tracking-wide">
            AGENTS
          </Link>
        </div>
      </div>
      {/* Content */}
      <div className="max-w-3xl mx-auto px-4 sm:px-6 py-8">
        {children}
      </div>
    </div>
  );
}

function InfoBlock({ label, value, sub }: { label: string; value: string; sub: string }) {
  return (
    <div className="border border-border bg-panel p-4">
      <div className="font-mono text-[10px] tracking-widest text-text-muted mb-2">{label}</div>
      <div className="text-text-primary font-medium text-sm mb-1">{value}</div>
      <div className="font-mono text-[11px] text-text-muted truncate">{sub}</div>
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

function CapabilityRow({ capability }: { capability: Capability }) {
  const risk = getCapabilityRiskLevel(capability);
  const needsTwoStep = requiresTwoStep(capability);

  const riskColors = {
    low: 'bg-green',
    medium: 'bg-amber',
    high: 'bg-red',
  };

  return (
    <div className="flex items-center gap-3 px-4 py-3 bg-panel hover:bg-panel-hover transition-colors">
      <div className={`w-1.5 h-1.5 ${riskColors[risk]} shrink-0`} />
      <span className="text-sm text-text-primary flex-1">
        {capabilityToHumanReadable(capability)}
      </span>
      {needsTwoStep && (
        <span className="font-mono text-[10px] tracking-wide text-red border border-red-dim px-2 py-0.5 bg-red-glow">
          2-STEP
        </span>
      )}
    </div>
  );
}
