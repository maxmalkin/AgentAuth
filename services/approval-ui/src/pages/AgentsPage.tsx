// Agents list page - shows all agents for the current human principal

import { useState, useEffect } from 'react';
import { Link } from '../Router';
import { listAgents, checkHealth } from '../api';
import type { AgentSummary } from '../types';

type PageState =
  | { type: 'loading' }
  | { type: 'error'; message: string; isOffline: boolean }
  | { type: 'loaded'; agents: AgentSummary[] };

export function AgentsPage() {
  const [state, setState] = useState<PageState>({ type: 'loading' });

  useEffect(() => {
    loadAgents();
  }, []);

  async function loadAgents() {
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
      const agents = await listAgents();
      setState({ type: 'loaded', agents });
    } catch (err) {
      setState({
        type: 'error',
        message: err instanceof Error ? err.message : 'Failed to load agents',
        isOffline: false,
      });
    }
  }

  if (state.type === 'loading') {
    return (
      <div className="page agents-page">
        <div className="loading">
          <div className="spinner"></div>
          <p>Loading your agents...</p>
        </div>
      </div>
    );
  }

  if (state.type === 'error') {
    return (
      <div className="page agents-page">
        <div className={`error-state ${state.isOffline ? 'offline' : ''}`}>
          <h2>{state.isOffline ? 'Connection Error' : 'Error'}</h2>
          <p>{state.message}</p>
          <button onClick={loadAgents} className="btn btn-primary">
            Try Again
          </button>
        </div>
      </div>
    );
  }

  const { agents } = state;

  return (
    <div className="page agents-page">
      <header className="page-header">
        <h1>Your Agents</h1>
        <p>Manage agents that have access to your accounts</p>
      </header>

      {agents.length === 0 ? (
        <div className="empty-state">
          <h2>No Agents Yet</h2>
          <p>You haven't authorized any agents to act on your behalf.</p>
        </div>
      ) : (
        <ul className="agent-list">
          {agents.map((agent) => (
            <AgentCard key={agent.agent_id} agent={agent} />
          ))}
        </ul>
      )}
    </div>
  );
}

function AgentCard({ agent }: { agent: AgentSummary }) {
  const statusClass = `status-${agent.status}`;

  return (
    <li className="agent-card">
      <div className="agent-info">
        <h3>{agent.name}</h3>
        <p className="agent-id">{agent.agent_id}</p>
        <p className="registered-at">
          Registered: {new Date(agent.registered_at).toLocaleDateString()}
        </p>
      </div>
      <div className="agent-meta">
        <span className={`status-badge ${statusClass}`}>{agent.status}</span>
        <span className="grant-count">{agent.active_grants} active grants</span>
      </div>
      <div className="agent-actions">
        <Link to={`/agents/${agent.agent_id}/activity`} className="btn btn-secondary">
          View Activity
        </Link>
      </div>
    </li>
  );
}
