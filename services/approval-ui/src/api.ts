// AgentAuth Registry API Client with CSRF Protection

import type {
  GrantRequest,
  AgentSummary,
  AgentDetails,
  AuditEvent,
  ApprovalAssertion,
  ApiError,
} from './types';

const REGISTRY_URL = process.env.REGISTRY_URL || 'http://localhost:8080';

/** CSRF token stored in memory and synced with cookie */
let csrfToken: string | null = null;

/** Generate a random CSRF token */
function generateCsrfToken(): string {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  return Array.from(array, (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Get or create CSRF token */
export function getCsrfToken(): string {
  if (!csrfToken) {
    // Try to read from cookie first
    const cookies = document.cookie.split(';');
    for (const cookie of cookies) {
      const [name, value] = cookie.trim().split('=');
      if (name === 'csrf_token' && value) {
        csrfToken = value;
        break;
      }
    }
    // Generate new token if not found
    if (!csrfToken) {
      csrfToken = generateCsrfToken();
      // Set as SameSite=Strict cookie
      document.cookie = `csrf_token=${csrfToken}; SameSite=Strict; Secure; Path=/`;
    }
  }
  return csrfToken;
}

/** Custom error class for API errors */
export class RegistryError extends Error {
  code: string;
  details?: Record<string, string>;

  constructor(apiError: ApiError) {
    super(apiError.error);
    this.name = 'RegistryError';
    this.code = apiError.code;
    this.details = apiError.details;
  }
}

/** Make an authenticated request to the registry */
async function request<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${REGISTRY_URL}${path}`;
  const headers = new Headers(options.headers);

  // Add CSRF token for state-changing requests
  if (options.method && ['POST', 'PUT', 'DELETE', 'PATCH'].includes(options.method)) {
    headers.set('X-CSRF-Token', getCsrfToken());
  }

  // Add content type for JSON bodies
  if (options.body && typeof options.body === 'string') {
    headers.set('Content-Type', 'application/json');
  }

  // Add credentials for cookie handling
  const response = await fetch(url, {
    ...options,
    headers,
    credentials: 'include',
  });

  if (!response.ok) {
    let apiError: ApiError;
    try {
      apiError = (await response.json()) as ApiError;
    } catch {
      apiError = {
        error: `Request failed with status ${response.status}`,
        code: 'REQUEST_FAILED',
      };
    }
    throw new RegistryError(apiError);
  }

  // Handle empty responses
  const text = await response.text();
  if (!text) {
    return {} as T;
  }

  return JSON.parse(text) as T;
}

/** Fetch a grant request by ID */
export async function getGrantRequest(grantId: string): Promise<GrantRequest> {
  return request<GrantRequest>(`/v1/grants/${grantId}`);
}

/** Submit approval for a grant */
export async function approveGrant(
  grantId: string,
  assertion: ApprovalAssertion,
  signature: string
): Promise<void> {
  await request(`/v1/grants/${grantId}/approve`, {
    method: 'POST',
    body: JSON.stringify({
      assertion,
      human_signature: signature,
    }),
  });
}

/** Deny a grant request */
export async function denyGrant(grantId: string, reason?: string): Promise<void> {
  await request(`/v1/grants/${grantId}/deny`, {
    method: 'POST',
    body: JSON.stringify({ reason }),
  });
}

/** List all agents for the current human principal */
export async function listAgents(): Promise<AgentSummary[]> {
  return request<AgentSummary[]>('/v1/agents');
}

/** Get agent details */
export async function getAgentDetails(agentId: string): Promise<AgentDetails> {
  return request<AgentDetails>(`/v1/agents/${agentId}`);
}

/** Get audit events for an agent */
export async function getAgentActivity(
  agentId: string,
  limit = 50,
  offset = 0
): Promise<AuditEvent[]> {
  return request<AuditEvent[]>(
    `/v1/audit/${agentId}?limit=${limit}&offset=${offset}`
  );
}

/** Revoke an agent */
export async function revokeAgent(agentId: string): Promise<void> {
  await request(`/v1/agents/${agentId}`, {
    method: 'DELETE',
  });
}

/** Revoke a specific grant */
export async function revokeGrant(grantId: string): Promise<void> {
  await request(`/v1/grants/${grantId}/revoke`, {
    method: 'POST',
  });
}

/** Check if registry is reachable */
export async function checkHealth(): Promise<boolean> {
  try {
    const response = await fetch(`${REGISTRY_URL}/health/ready`, {
      method: 'GET',
      credentials: 'include',
    });
    return response.ok;
  } catch {
    return false;
  }
}
