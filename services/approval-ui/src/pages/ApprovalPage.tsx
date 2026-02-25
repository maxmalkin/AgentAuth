// Grant approval page with two-step confirmation for high-risk capabilities

import { useState, useEffect } from 'react';
import { useParams, useRouter } from '../Router';
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

    // Check if registry is reachable
    const isHealthy = await checkHealth();
    if (!isHealthy) {
      setState({
        type: 'error',
        message: 'Unable to connect to the AgentAuth registry. Please try again later.',
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
      // Requires two-step confirmation
      setState({ type: 'confirming', grant, step: 1 });
    } else {
      // Direct approval
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
        message: 'Your browser does not support WebAuthn/Passkeys. Please use a modern browser.',
        isOffline: false,
      });
      return;
    }

    setState({ type: 'signing', grant });

    try {
      // Create the approval assertion
      const assertion: ApprovalAssertion = {
        grant_id: grant.grant_id,
        agent_id: grant.agent_id,
        granted_capabilities: grant.requested_capabilities,
        behavioral_envelope: grant.requested_envelope,
        approved_at: new Date().toISOString(),
        approval_nonce: crypto.randomUUID(),
      };

      // Sign with WebAuthn
      const signature = await signApprovalAssertion(assertion);

      // Submit to registry
      await approveGrant(grant.grant_id, assertion, signature);

      setState({ type: 'success', action: 'approved' });
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to sign approval',
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

  // Render based on state
  if (state.type === 'loading') {
    return (
      <div className="page approval-page">
        <div className="loading">
          <div className="spinner"></div>
          <p>Loading grant request...</p>
        </div>
      </div>
    );
  }

  if (state.type === 'error') {
    return (
      <div className="page approval-page">
        <div className={`error-state ${state.isOffline ? 'offline' : ''}`}>
          <h2>{state.isOffline ? 'Connection Error' : 'Error'}</h2>
          <p>{state.message}</p>
          <button onClick={loadGrant} className="btn btn-primary">
            Try Again
          </button>
        </div>
      </div>
    );
  }

  if (state.type === 'expired') {
    return (
      <div className="page approval-page">
        <div className="expired-state">
          <h2>Request Expired</h2>
          <p>This grant request has expired and can no longer be approved.</p>
          <button onClick={() => navigate('/agents')} className="btn btn-secondary">
            View Your Agents
          </button>
        </div>
      </div>
    );
  }

  if (state.type === 'success') {
    return (
      <div className="page approval-page">
        <div className="success-state">
          <h2>
            {state.action === 'approved' ? 'Request Approved' : 'Request Denied'}
          </h2>
          <p>
            {state.action === 'approved'
              ? 'The agent has been granted access.'
              : 'The request has been denied.'}
          </p>
          <button onClick={() => navigate('/agents')} className="btn btn-primary">
            View Your Agents
          </button>
        </div>
      </div>
    );
  }

  if (state.type === 'signing') {
    return (
      <div className="page approval-page">
        <div className="signing-state">
          <h2>Authenticating...</h2>
          <p>Please complete authentication with your passkey.</p>
          <div className="spinner"></div>
        </div>
      </div>
    );
  }

  // Loaded or confirming state - show the grant details
  const grant = state.type === 'loaded' ? state.grant : state.grant;
  const isConfirming = state.type === 'confirming';
  const confirmStep = isConfirming ? state.step : 0;

  return (
    <div className="page approval-page">
      <header className="approval-header">
        <h1>Grant Request</h1>
        <p className="grant-id">ID: {grant.grant_id}</p>
      </header>

      <section className="agent-info">
        <h2>Agent Requesting Access</h2>
        <div className="info-card">
          <p><strong>Name:</strong> {grant.agent_name}</p>
          <p><strong>Agent ID:</strong> {grant.agent_id}</p>
        </div>
      </section>

      <section className="service-info">
        <h2>Service Provider</h2>
        <div className="info-card">
          <p><strong>Name:</strong> {grant.service_provider_name}</p>
          <p><strong>ID:</strong> {grant.service_provider_id}</p>
        </div>
      </section>

      <section className="capabilities">
        <h2>Requested Permissions</h2>
        <ul className="capability-list">
          {grant.requested_capabilities.map((cap, idx) => (
            <CapabilityItem key={idx} capability={cap} />
          ))}
        </ul>
      </section>

      <section className="behavioral-envelope">
        <h2>Behavioral Constraints</h2>
        <ul className="envelope-list">
          {envelopeToHumanReadable(grant.requested_envelope).map((desc, idx) => (
            <li key={idx}>{desc}</li>
          ))}
        </ul>
      </section>

      <section className="expiry-info">
        <p>This request expires: {new Date(grant.expires_at).toLocaleString()}</p>
      </section>

      {isConfirming && (
        <div className="confirmation-overlay">
          <div className="confirmation-dialog">
            {confirmStep === 1 ? (
              <>
                <h3>High-Risk Permissions Requested</h3>
                <p>
                  This request includes permissions that could modify or delete
                  your data, or make financial transactions on your behalf.
                </p>
                <p><strong>Are you sure you want to proceed?</strong></p>
                <div className="dialog-actions">
                  <button
                    onClick={() => setState({ type: 'loaded', grant })}
                    className="btn btn-secondary"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={() => handleConfirmStep1(grant)}
                    className="btn btn-warning"
                  >
                    Yes, Continue
                  </button>
                </div>
              </>
            ) : (
              <>
                <h3>Final Confirmation</h3>
                <p>
                  You are about to grant access to:
                </p>
                <ul>
                  {grant.requested_capabilities
                    .filter(requiresTwoStep)
                    .map((cap, idx) => (
                      <li key={idx} className="high-risk">
                        {capabilityToHumanReadable(cap)}
                      </li>
                    ))}
                </ul>
                <p><strong>This action cannot be undone without revoking the grant.</strong></p>
                <div className="dialog-actions">
                  <button
                    onClick={() => setState({ type: 'loaded', grant })}
                    className="btn btn-secondary"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={() => startSigning(grant)}
                    className="btn btn-danger"
                  >
                    Approve with Passkey
                  </button>
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {!isConfirming && (
        <section className="actions">
          <div className="deny-section">
            <input
              type="text"
              placeholder="Reason for denial (optional)"
              value={denyReason}
              onChange={(e) => setDenyReason(e.target.value)}
              className="deny-reason-input"
            />
            <button
              onClick={() => handleDeny(grant)}
              className="btn btn-secondary"
            >
              Deny Request
            </button>
          </div>
          <button
            onClick={() => handleApproveClick(grant)}
            className="btn btn-primary btn-large"
          >
            Approve with Passkey
          </button>
        </section>
      )}
    </div>
  );
}

function CapabilityItem({ capability }: { capability: Capability }) {
  const riskLevel = getCapabilityRiskLevel(capability);
  const needsTwoStep = requiresTwoStep(capability);

  return (
    <li className={`capability-item risk-${riskLevel}`}>
      <span className="capability-text">
        {capabilityToHumanReadable(capability)}
      </span>
      {needsTwoStep && (
        <span className="high-risk-badge">Requires confirmation</span>
      )}
    </li>
  );
}
