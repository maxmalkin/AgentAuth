import { test, expect, type Page, type Route } from '@playwright/test';
import type { AgentSummary, AgentDetails, AuditEvent, GrantSummary } from '../src/types';

// Mock data
const mockAgents: AgentSummary[] = [
  {
    agent_id: 'agent_01JTEST123456789012345678',
    name: 'Personal Assistant',
    status: 'active',
    registered_at: '2024-01-15T10:00:00Z',
    active_grants: 3,
  },
  {
    agent_id: 'agent_01JTEST123456789012345679',
    name: 'Code Helper',
    status: 'active',
    registered_at: '2024-02-20T14:30:00Z',
    active_grants: 1,
  },
  {
    agent_id: 'agent_01JTEST123456789012345680',
    name: 'Old Agent',
    status: 'revoked',
    registered_at: '2023-06-01T08:00:00Z',
    active_grants: 0,
  },
];

const mockAgentDetails: AgentDetails = {
  agent_id: 'agent_01JTEST123456789012345678',
  name: 'Personal Assistant',
  status: 'active',
  registered_at: '2024-01-15T10:00:00Z',
  public_key: 'MCowBQYDK2VwAyEAz1234567890abcdefghijklmnopqrstuvwxyz12345678',
  capabilities: [
    { type: 'read', resource: 'calendar', filter: null },
    { type: 'write', resource: 'calendar', conditions: null },
  ],
  grants: [
    {
      grant_id: 'grant_01JTEST123456789012345678',
      service_provider_name: 'Calendar Service',
      capabilities: [
        { type: 'read', resource: 'calendar', filter: null },
        { type: 'write', resource: 'calendar', conditions: null },
      ],
      status: 'active',
      created_at: '2024-01-15T11:00:00Z',
    },
    {
      grant_id: 'grant_01JTEST123456789012345679',
      service_provider_name: 'Email Service',
      capabilities: [
        { type: 'read', resource: 'emails', filter: 'unread' },
      ],
      status: 'active',
      created_at: '2024-01-16T09:00:00Z',
    },
  ],
};

const mockAuditEvents: AuditEvent[] = [
  {
    event_id: 'evt_01JTEST123456789012345678',
    event_type: 'token_verified',
    agent_id: 'agent_01JTEST123456789012345678',
    service_provider_id: 'sp_calendar',
    capability: { type: 'read', resource: 'calendar', filter: null },
    outcome: 'allowed',
    created_at: '2024-03-15T10:30:00Z',
    details: {},
  },
  {
    event_id: 'evt_01JTEST123456789012345679',
    event_type: 'token_denied',
    agent_id: 'agent_01JTEST123456789012345678',
    service_provider_id: 'sp_calendar',
    capability: { type: 'delete', resource: 'calendar', filter: null },
    outcome: 'denied',
    created_at: '2024-03-15T10:25:00Z',
    details: { reason: 'Capability not granted' },
  },
  {
    event_id: 'evt_01JTEST123456789012345680',
    event_type: 'grant_approved',
    agent_id: 'agent_01JTEST123456789012345678',
    service_provider_id: 'sp_email',
    capability: null,
    outcome: 'allowed',
    created_at: '2024-01-16T09:00:00Z',
    details: {},
  },
];

async function setupMockApi(page: Page) {
  // Mock health check
  await page.route('**/health/ready', async (route: Route) => {
    await route.fulfill({ status: 200, body: '{}' });
  });

  // Mock agents list
  await page.route('**/v1/agents', async (route: Route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(mockAgents),
      });
    }
  });

  // Mock agent details
  await page.route('**/v1/agents/agent_01JTEST123456789012345678', async (route: Route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(mockAgentDetails),
      });
    } else if (route.request().method() === 'DELETE') {
      await route.fulfill({ status: 200, body: '{}' });
    }
  });

  // Mock audit events
  await page.route('**/v1/audit/agent_01JTEST123456789012345678*', async (route: Route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockAuditEvents),
    });
  });
}

test.describe('Agents Page', () => {
  test('displays list of agents', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents');

    await expect(page.locator('h1')).toHaveText('Your Agents');
    await expect(page.locator('.agent-card')).toHaveCount(3);
  });

  test('shows agent name and status', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents');

    // Check first agent
    const firstAgent = page.locator('.agent-card').first();
    await expect(firstAgent).toContainText('Personal Assistant');
    await expect(firstAgent.locator('.status-badge')).toContainText('active');
    await expect(firstAgent).toContainText('3 active grants');
  });

  test('shows revoked status correctly', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents');

    // Find the revoked agent
    const revokedAgent = page.locator('.agent-card', { hasText: 'Old Agent' });
    await expect(revokedAgent.locator('.status-badge')).toContainText('revoked');
    await expect(revokedAgent.locator('.status-badge')).toHaveClass(/status-revoked/);
  });

  test('can navigate to agent activity page', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents');

    // Click view activity on first agent
    await page.locator('.agent-card').first().locator('a:text("View Activity")').click();

    // Should navigate to activity page
    await expect(page).toHaveURL(/\/agents\/agent_01JTEST123456789012345678\/activity/);
    await expect(page.locator('h1')).toHaveText('Personal Assistant');
  });

  test('shows empty state when no agents', async ({ page }) => {
    // Mock empty agents list
    await page.route('**/health/ready', async (route: Route) => {
      await route.fulfill({ status: 200, body: '{}' });
    });
    await page.route('**/v1/agents', async (route: Route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: '[]',
      });
    });

    await page.goto('/agents');

    await expect(page.locator('.empty-state')).toBeVisible();
    await expect(page.locator('.empty-state')).toContainText('No Agents Yet');
  });

  test('shows error state when API fails', async ({ page }) => {
    await page.route('**/health/ready', async (route: Route) => {
      await route.fulfill({ status: 503 });
    });

    await page.goto('/agents');

    await expect(page.locator('.error-state')).toBeVisible();
    await expect(page.locator('.error-state')).toContainText('Connection Error');
  });
});

test.describe('Agent Activity Page', () => {
  test('displays agent details', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    await expect(page.locator('h1')).toHaveText('Personal Assistant');
    await expect(page.locator('.agent-id')).toContainText('agent_01JTEST123456789012345678');
    await expect(page.locator('.status-badge')).toContainText('active');
  });

  test('shows public key preview', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    await expect(page.locator('.agent-details')).toContainText('Public Key');
    await expect(page.locator('.agent-details code')).toContainText('MCowBQYDK2VwAyEAz123');
  });

  test('displays active grants', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    await expect(page.locator('.grants-section')).toBeVisible();
    await expect(page.locator('.grant-card')).toHaveCount(2);

    // Check grant details
    const firstGrant = page.locator('.grant-card').first();
    await expect(firstGrant).toContainText('Calendar Service');
    await expect(firstGrant).toContainText('Read calendar');
  });

  test('displays recent activity', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    await expect(page.locator('.activity-section')).toBeVisible();
    await expect(page.locator('.activity-item')).toHaveCount(3);

    // Check event types are displayed
    await expect(page.locator('.activity-list')).toContainText('Token Verified');
    await expect(page.locator('.activity-list')).toContainText('Token Denied');
    await expect(page.locator('.activity-list')).toContainText('Grant Approved');
  });

  test('shows outcome badges correctly', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    // Check allowed outcome
    await expect(page.locator('.outcome-badge.outcome-success')).toBeVisible();

    // Check denied outcome
    await expect(page.locator('.outcome-badge.outcome-error')).toBeVisible();
  });

  test('shows back link to agents page', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    await expect(page.locator('.back-link')).toBeVisible();
    await page.click('.back-link');

    await expect(page).toHaveURL('/agents');
  });

  test('shows danger zone for active agents', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    await expect(page.locator('.danger-zone')).toBeVisible();
    await expect(page.locator('.danger-zone')).toContainText('Revoke Agent');
  });

  test('can revoke an agent', async ({ page }) => {
    let revokeCalled = false;

    await setupMockApi(page);

    // Override the delete route to track the call
    await page.route('**/v1/agents/agent_01JTEST123456789012345678', async (route: Route) => {
      if (route.request().method() === 'DELETE') {
        revokeCalled = true;
        await route.fulfill({ status: 200, body: '{}' });
      } else {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(mockAgentDetails),
        });
      }
    });

    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    // Click revoke button
    await page.click('.danger-zone button:text("Revoke Agent")');

    // Confirmation dialog should appear
    await expect(page.locator('.confirmation-dialog')).toBeVisible();
    await expect(page.locator('.confirmation-dialog h3')).toHaveText('Revoke Agent');

    // Confirm revocation
    await page.click('.confirmation-dialog button:text("Revoke Agent")');

    // Should have called the API and navigated away
    expect(revokeCalled).toBe(true);
    await expect(page).toHaveURL('/agents');
  });

  test('can cancel agent revocation', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    // Click revoke button
    await page.click('.danger-zone button:text("Revoke Agent")');

    // Confirmation dialog should appear
    await expect(page.locator('.confirmation-dialog')).toBeVisible();

    // Cancel
    await page.click('.confirmation-dialog button:text("Cancel")');

    // Dialog should close
    await expect(page.locator('.confirmation-dialog')).not.toBeVisible();

    // Should still be on the same page
    await expect(page).toHaveURL(/\/agents\/agent_01JTEST123456789012345678\/activity/);
  });

  test('can revoke individual grant', async ({ page }) => {
    let revokeGrantCalled = false;

    await setupMockApi(page);

    // Mock grant revocation
    await page.route('**/v1/grants/*/revoke', async (route: Route) => {
      revokeGrantCalled = true;
      await route.fulfill({ status: 200, body: '{}' });
    });

    await page.goto('/agents/agent_01JTEST123456789012345678/activity');

    // Click revoke on first grant
    await page.locator('.grant-card').first().locator('button:text("Revoke")').click();

    // Confirmation dialog should appear
    await expect(page.locator('.confirmation-dialog h3')).toHaveText('Revoke Grant');

    // Confirm
    await page.click('.confirmation-dialog button:text("Revoke Grant")');

    expect(revokeGrantCalled).toBe(true);
  });

  test('shows error when agent not found', async ({ page }) => {
    await page.route('**/health/ready', async (route: Route) => {
      await route.fulfill({ status: 200, body: '{}' });
    });

    await page.route('**/v1/agents/nonexistent', async (route: Route) => {
      await route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Agent not found', code: 'NOT_FOUND' }),
      });
    });

    await page.goto('/agents/nonexistent/activity');

    await expect(page.locator('.error-state')).toBeVisible();
    await expect(page.locator('.error-state')).toContainText('Agent not found');
  });
});

test.describe('Navigation', () => {
  test('home page links to agents', async ({ page }) => {
    await setupMockApi(page);
    await page.goto('/');

    await expect(page.locator('h1')).toHaveText('AgentAuth');
    await page.click('a:text("View Your Agents")');

    await expect(page).toHaveURL('/agents');
  });

  test('404 page for unknown routes', async ({ page }) => {
    await page.goto('/unknown/route');

    await expect(page.locator('.not-found-page')).toBeVisible();
    await expect(page.locator('.not-found-page')).toContainText('404');
    await expect(page.locator('.not-found-page')).toContainText('Page Not Found');
  });

  test('can navigate home from 404', async ({ page }) => {
    await page.goto('/unknown/route');

    await page.click('a:text("Go Home")');

    await expect(page).toHaveURL('/');
    await expect(page.locator('h1')).toHaveText('AgentAuth');
  });
});
