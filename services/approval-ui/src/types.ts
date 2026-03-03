// AgentAuth Approval UI Types

/** Capability types matching the Rust enum (serde rename_all = "snake_case") */
export type Capability =
  | { type: 'read'; resource: string; filter: string | null }
  | { type: 'write'; resource: string; conditions: Record<string, string> | null }
  | { type: 'transact'; resource: string; max_value: number }
  | { type: 'delete'; resource: string; filter: string | null }
  | { type: 'custom'; namespace: string; name: string; params: Record<string, string> };

/** Behavioral envelope constraints */
export interface BehavioralEnvelope {
  max_requests_per_minute: number;
  max_burst: number;
  requires_human_online: boolean;
  human_confirmation_threshold: number | null;
  allowed_time_windows: TimeWindow[] | null;
  max_session_duration_secs: number;
}

/** Time window for allowed operations */
export interface TimeWindow {
  start_hour: number;
  start_minute: number;
  end_hour: number;
  end_minute: number;
  days_of_week: number[];
}

/** Grant request from the registry */
export interface GrantRequest {
  grant_id: string;
  agent_id: string;
  agent_name: string;
  service_provider_id: string;
  service_provider_name: string;
  human_principal_id: string;
  requested_capabilities: Capability[];
  requested_envelope: BehavioralEnvelope;
  created_at: string;
  expires_at: string;
  status: 'pending' | 'approved' | 'denied' | 'expired';
}

/** Agent manifest summary */
export interface AgentSummary {
  agent_id: string;
  name: string;
  registered_at: string;
  status: 'active' | 'suspended' | 'revoked';
  active_grants: number;
  pending_grant_id?: string;
}

/** Agent details */
export interface AgentDetails {
  agent_id: string;
  name: string;
  registered_at: string;
  status: 'active' | 'suspended' | 'revoked';
  public_key: string;
  capabilities: Capability[];
  grants: GrantSummary[];
}

/** Grant summary for agent details */
export interface GrantSummary {
  grant_id: string;
  service_provider_name: string;
  capabilities: Capability[];
  created_at: string;
  status: string;
}

/** Audit event */
export interface AuditEvent {
  event_id: string;
  agent_id: string;
  event_type: 'token_issued' | 'token_verified' | 'token_denied' | 'grant_approved' | 'grant_denied' | 'agent_registered' | 'agent_revoked';
  service_provider_id: string | null;
  capability: Capability | null;
  outcome: 'allowed' | 'denied' | 'rate_limited';
  created_at: string;
  details?: Record<string, string>;
}

/** Approval assertion to be signed */
export interface ApprovalAssertion {
  grant_id: string;
  agent_id: string;
  granted_capabilities: Capability[];
  behavioral_envelope: BehavioralEnvelope;
  approved_at: string;
  approval_nonce: string;
}

/** API error response */
export interface ApiError {
  error: string;
  code: string;
  details?: Record<string, string>;
}
