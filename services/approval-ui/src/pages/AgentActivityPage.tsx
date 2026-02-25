// Agent activity page - shows audit trail for a specific agent

import { useState, useEffect } from 'react';
import { useParams, Link, useRouter } from '../Router';
import {
  getAgentDetails,
  getAgentActivity,
  revokeAgent,
  revokeGrant,
  checkHealth,
} from '../api';
import { capabilityToHumanReadable } from '../utils/capabilities';
import type { AgentDetails, AuditEvent, GrantSummary } from '../types';

type PageState =
  | { type: 'loading' }
  | { type: 'error'; message: string; isOffline: boolean }
  | { type: 'loaded'; agent: AgentDetails; events: AuditEvent[] };

export function AgentActivityPage() {
  const { agent_id } = useParams<{ agent_id: string }>();
  const { navigate } = useRouter();
  const [state, setState] = useState<PageState>({ type: 'loading' });
  const [showRevokeConfirm, setShowRevokeConfirm] = useState(false);
  const [revokeGrantId, setRevokeGrantId] = useState<string | null>(null);

  useEffect(() => {
    loadAgent();
  }, [agent_id]);

  async function loadAgent() {
    setState({ type: 'loading' });

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
      const [agent, events] = await Promise.all([
        getAgentDetails(agent_id),
        getAgentActivity(agent_id),
      ]);
      setState({ type: 'loaded', agent, events });
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to load agent details',
        isOffline: false,
      });
    }
  }

  async function handleRevokeAgent() {
    try {
      await revokeAgent(agent_id);
      navigate('/agents');
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to revoke agent',
        isOffline: false,
      });
    }
  }

  async function handleRevokeGrant(grantId: string) {
    try {
      await revokeGrant(grantId);
      setRevokeGrantId(null);
      loadAgent(); // Refresh the page
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to revoke grant',
        isOffline: false,
      });
    }
  }

  if (state.type === 'loading') {
    return (
      <div className="page activity-page">
        <div className="loading">
          <div className="spinner"></div>
          <p>Loading agent details...</p>
        </div>
      </div>
    );
  }

  if (state.type === 'error') {
    return (
      <div className="page activity-page">
        <div className={`error-state ${state.isOffline ? 'offline' : ''}`}>
          <h2>{state.isOffline ? 'Connection Error' : 'Error'}</h2>
          <p>{state.message}</p>
          <button onClick={loadAgent} className="btn btn-primary">
            Try Again
          </button>
          <Link to="/agents" className="btn btn-secondary">
            Back to Agents
          </Link>
        </div>
      </div>
    );
  }

  const { agent, events } = state;

  return (
    <div className="page activity-page">
      <header className="page-header">
        <Link to="/agents" className="back-link">&larr; Back to Agents</Link>
        <h1>{agent.name}</h1>
        <p className="agent-id">{agent.agent_id}</p>
        <span className={`status-badge status-${agent.status}`}>{agent.status}</span>
      </header>

      <section className="agent-details">
        <h2>Agent Details</h2>
        <div className="info-card">
          <p><strong>Registered:</strong> {new Date(agent.registered_at).toLocaleString()}</p>
          <p><strong>Public Key:</strong> <code>{agent.public_key.slice(0, 20)}...</code></p>
        </div>
      </section>

      <section className="grants-section">
        <h2>Active Grants</h2>
        {agent.grants.length === 0 ? (
          <p className="empty-message">No active grants</p>
        ) : (
          <ul className="grant-list">
            {agent.grants.map((grant) => (
              <GrantCard
                key={grant.grant_id}
                grant={grant}
                onRevoke={() => setRevokeGrantId(grant.grant_id)}
              />
            ))}
          </ul>
        )}
      </section>

      <section className="activity-section">
        <h2>Recent Activity</h2>
        {events.length === 0 ? (
          <p className="empty-message">No recent activity</p>
        ) : (
          <ul className="activity-list">
            {events.map((event) => (
              <ActivityItem key={event.event_id} event={event} />
            ))}
          </ul>
        )}
      </section>

      {agent.status === 'active' && (
        <section className="danger-zone">
          <h2>Danger Zone</h2>
          <p>Revoking this agent will immediately terminate all its access.</p>
          <button
            onClick={() => setShowRevokeConfirm(true)}
            className="btn btn-danger"
          >
            Revoke Agent
          </button>
        </section>
      )}

      {/* Revoke agent confirmation dialog */}
      {showRevokeConfirm && (
        <div className="confirmation-overlay">
          <div className="confirmation-dialog">
            <h3>Revoke Agent</h3>
            <p>
              Are you sure you want to revoke <strong>{agent.name}</strong>?
              This will immediately terminate all access for this agent.
            </p>
            <div className="dialog-actions">
              <button
                onClick={() => setShowRevokeConfirm(false)}
                className="btn btn-secondary"
              >
                Cancel
              </button>
              <button
                onClick={handleRevokeAgent}
                className="btn btn-danger"
              >
                Revoke Agent
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Revoke grant confirmation dialog */}
      {revokeGrantId && (
        <div className="confirmation-overlay">
          <div className="confirmation-dialog">
            <h3>Revoke Grant</h3>
            <p>
              Are you sure you want to revoke this grant? The agent will no
              longer have access to this service provider.
            </p>
            <div className="dialog-actions">
              <button
                onClick={() => setRevokeGrantId(null)}
                className="btn btn-secondary"
              >
                Cancel
              </button>
              <button
                onClick={() => handleRevokeGrant(revokeGrantId)}
                className="btn btn-danger"
              >
                Revoke Grant
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function GrantCard({
  grant,
  onRevoke,
}: {
  grant: GrantSummary;
  onRevoke: () => void;
}) {
  return (
    <li className={`grant-card status-${grant.status}`}>
      <div className="grant-info">
        <h3>{grant.service_provider_name}</h3>
        <p className="grant-date">
          Granted: {new Date(grant.created_at).toLocaleDateString()}
        </p>
        <ul className="capability-list compact">
          {grant.capabilities.map((cap, idx) => (
            <li key={idx}>{capabilityToHumanReadable(cap)}</li>
          ))}
        </ul>
      </div>
      <div className="grant-actions">
        <span className={`status-badge status-${grant.status}`}>{grant.status}</span>
        {grant.status === 'active' && (
          <button onClick={onRevoke} className="btn btn-sm btn-danger">
            Revoke
          </button>
        )}
      </div>
    </li>
  );
}

function ActivityItem({ event }: { event: AuditEvent }) {
  const eventLabels: Record<string, string> = {
    token_issued: 'Token Issued',
    token_verified: 'Token Verified',
    token_denied: 'Token Denied',
    grant_approved: 'Grant Approved',
    grant_denied: 'Grant Denied',
    agent_registered: 'Agent Registered',
    agent_revoked: 'Agent Revoked',
  };

  const outcomeClasses: Record<string, string> = {
    allowed: 'outcome-success',
    denied: 'outcome-error',
    rate_limited: 'outcome-warning',
  };

  return (
    <li className="activity-item">
      <div className="activity-time">
        {new Date(event.created_at).toLocaleString()}
      </div>
      <div className="activity-info">
        <span className="event-type">{eventLabels[event.event_type] || event.event_type}</span>
        {event.capability && (
          <span className="event-capability">
            {capabilityToHumanReadable(event.capability)}
          </span>
        )}
      </div>
      <span className={`outcome-badge ${outcomeClasses[event.outcome]}`}>
        {event.outcome}
      </span>
    </li>
  );
}
