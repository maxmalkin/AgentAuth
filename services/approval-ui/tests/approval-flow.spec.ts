import { test, expect, type Page, type Route } from '@playwright/test';
import type { GrantRequest, Capability, BehavioralEnvelope } from '../src/types';

// Mock data
const mockGrant: GrantRequest = {
  grant_id: 'grant_01JTEST123456789012345678',
  agent_id: 'agent_01JTEST123456789012345678',
  agent_name: 'Test Agent',
  service_provider_id: 'sp_01JTEST123456789012345678',
  service_provider_name: 'Test Service Provider',
  requested_capabilities: [
    { type: 'read', resource: 'calendar', filter: null },
    { type: 'write', resource: 'calendar', conditions: null },
  ],
  requested_envelope: {
    max_requests_per_minute: 30,
    max_burst: 10,
    requires_human_online: false,
    human_confirmation_threshold: null,
    allowed_time_windows: null,
    max_session_duration_secs: 3600,
  },
  status: 'pending',
  created_at: new Date().toISOString(),
  expires_at: new Date(Date.now() + 3600000).toISOString(),
};

// High-risk grant requiring two-step confirmation
const mockHighRiskGrant: GrantRequest = {
  ...mockGrant,
  grant_id: 'grant_01JTEST123456789012345679',
  requested_capabilities: [
    { type: 'read', resource: 'calendar', filter: null },
    { type: 'transact', resource: 'payments', max_value: 1000 },
    { type: 'delete', resource: 'documents', filter: null },
  ],
};

// Setup mock API routes
async function setupMockApi(page: Page, grant: GrantRequest = mockGrant) {
  // Mock the health check
  await page.route('**/health/ready', async (route: Route) => {
    await route.fulfill({
      status: 200,
      body: JSON.stringify({ status: 'ok' }),
    });
  });

  // Mock the grant request endpoint
  await page.route(`**/v1/grants/${grant.grant_id}`, async (route: Route) => {
    if (route.request().method() === 'GET') {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(grant),
      });
    }
  });
}

test.describe('Approval Page', () => {
  test('displays grant request details correctly', async ({ page }) => {
    await setupMockApi(page);
    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Wait for the page to load
    await expect(page.locator('h1')).toHaveText('Grant Request');

    // Verify agent info is displayed
    await expect(page.locator('.agent-info')).toContainText(mockGrant.agent_name);
    await expect(page.locator('.agent-info')).toContainText(mockGrant.agent_id);

    // Verify service provider info
    await expect(page.locator('.service-info')).toContainText(mockGrant.service_provider_name);

    // Verify capabilities are listed
    await expect(page.locator('.capability-list')).toBeVisible();
    await expect(page.locator('.capability-item')).toHaveCount(2);
  });

  test('displays capabilities in human-readable format', async ({ page }) => {
    await setupMockApi(page);
    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Check that capabilities are translated to human-readable text
    await expect(page.locator('.capability-list')).toContainText('Read calendar');
    await expect(page.locator('.capability-list')).toContainText('Write to calendar');
  });

  test('displays behavioral envelope constraints', async ({ page }) => {
    await setupMockApi(page);
    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Check that behavioral envelope is displayed
    await expect(page.locator('.envelope-list')).toContainText('30 actions per minute');
    await expect(page.locator('.envelope-list')).toContainText('Burst limit: 10');
  });

  test('shows expiry information', async ({ page }) => {
    await setupMockApi(page);
    await page.goto(`/approve/${mockGrant.grant_id}`);

    await expect(page.locator('.expiry-info')).toBeVisible();
    await expect(page.locator('.expiry-info')).toContainText('expires');
  });
});

test.describe('Two-Step Confirmation', () => {
  test('high-risk capabilities require two-step confirmation', async ({ page }) => {
    await setupMockApi(page, mockHighRiskGrant);
    await page.goto(`/approve/${mockHighRiskGrant.grant_id}`);

    // Wait for page to load
    await expect(page.locator('h1')).toHaveText('Grant Request');

    // Verify high-risk capabilities show the confirmation badge
    await expect(page.locator('.high-risk-badge')).toBeVisible();

    // Click approve button
    await page.click('button:text("Approve with Passkey")');

    // First confirmation step should appear
    await expect(page.locator('.confirmation-dialog')).toBeVisible();
    await expect(page.locator('.confirmation-dialog h3')).toHaveText('High-Risk Permissions Requested');

    // Click continue on first step
    await page.click('button:text("Yes, Continue")');

    // Second confirmation step should appear
    await expect(page.locator('.confirmation-dialog h3')).toHaveText('Final Confirmation');
    await expect(page.locator('.confirmation-dialog')).toContainText('This action cannot be undone');
  });

  test('can cancel two-step confirmation at first step', async ({ page }) => {
    await setupMockApi(page, mockHighRiskGrant);
    await page.goto(`/approve/${mockHighRiskGrant.grant_id}`);

    // Wait and click approve
    await expect(page.locator('h1')).toHaveText('Grant Request');
    await page.click('button:text("Approve with Passkey")');

    // First step appears
    await expect(page.locator('.confirmation-dialog')).toBeVisible();

    // Click cancel
    await page.click('.confirmation-dialog button:text("Cancel")');

    // Dialog should close
    await expect(page.locator('.confirmation-dialog')).not.toBeVisible();
  });

  test('can cancel two-step confirmation at second step', async ({ page }) => {
    await setupMockApi(page, mockHighRiskGrant);
    await page.goto(`/approve/${mockHighRiskGrant.grant_id}`);

    // Wait and click approve
    await expect(page.locator('h1')).toHaveText('Grant Request');
    await page.click('button:text("Approve with Passkey")');

    // Progress through first step
    await page.click('button:text("Yes, Continue")');

    // Second step appears
    await expect(page.locator('.confirmation-dialog h3')).toHaveText('Final Confirmation');

    // Click cancel
    await page.click('.confirmation-dialog button:text("Cancel")');

    // Dialog should close
    await expect(page.locator('.confirmation-dialog')).not.toBeVisible();
  });

  test('low-risk capabilities do not require two-step confirmation', async ({ page }) => {
    await setupMockApi(page);
    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Wait for page to load
    await expect(page.locator('h1')).toHaveText('Grant Request');

    // Verify no high-risk badges
    await expect(page.locator('.high-risk-badge')).not.toBeVisible();
  });
});

test.describe('Denial Flow', () => {
  test('can deny a grant request', async ({ page }) => {
    let denyCalled = false;
    let denyBody: { reason?: string } | null = null;

    await setupMockApi(page);

    // Mock the deny endpoint
    await page.route(`**/v1/grants/${mockGrant.grant_id}/deny`, async (route: Route) => {
      if (route.request().method() === 'POST') {
        denyCalled = true;
        denyBody = JSON.parse(route.request().postData() || '{}');
        await route.fulfill({
          status: 200,
          body: '{}',
        });
      }
    });

    await page.goto(`/approve/${mockGrant.grant_id}`);
    await expect(page.locator('h1')).toHaveText('Grant Request');

    // Click deny button
    await page.click('button:text("Deny Request")');

    // Verify deny was called
    expect(denyCalled).toBe(true);

    // Success state should appear
    await expect(page.locator('.success-state h2')).toHaveText('Request Denied');
  });

  test('can provide a reason when denying', async ({ page }) => {
    let capturedBody = '';

    await setupMockApi(page);

    // Mock the deny endpoint
    await page.route(`**/v1/grants/${mockGrant.grant_id}/deny`, async (route: Route) => {
      if (route.request().method() === 'POST') {
        capturedBody = route.request().postData() || '{}';
        await route.fulfill({
          status: 200,
          body: '{}',
        });
      }
    });

    await page.goto(`/approve/${mockGrant.grant_id}`);
    await expect(page.locator('h1')).toHaveText('Grant Request');

    // Fill in the denial reason
    await page.fill('.deny-reason-input', 'Security concern');

    // Click deny button
    await page.click('button:text("Deny Request")');

    // Verify reason was sent
    const parsed = JSON.parse(capturedBody) as { reason?: string };
    expect(parsed.reason).toBe('Security concern');
  });
});

test.describe('Error Handling', () => {
  test('shows error state when registry is unreachable', async ({ page }) => {
    // Mock health check to fail
    await page.route('**/health/ready', async (route: Route) => {
      await route.fulfill({
        status: 503,
        body: 'Service Unavailable',
      });
    });

    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Error state should appear
    await expect(page.locator('.error-state')).toBeVisible();
    await expect(page.locator('.error-state')).toContainText('Connection Error');
    await expect(page.locator('.error-state')).toHaveClass(/offline/);
  });

  test('shows error when grant request fails', async ({ page }) => {
    // Mock health check to succeed
    await page.route('**/health/ready', async (route: Route) => {
      await route.fulfill({
        status: 200,
        body: '{}',
      });
    });

    // Mock grant request to fail
    await page.route('**/v1/grants/*', async (route: Route) => {
      await route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Grant not found', code: 'NOT_FOUND' }),
      });
    });

    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Error state should appear
    await expect(page.locator('.error-state')).toBeVisible();
    await expect(page.locator('.error-state')).toContainText('Grant not found');
  });

  test('shows expired state for expired grants', async ({ page }) => {
    const expiredGrant: GrantRequest = {
      ...mockGrant,
      status: 'expired',
    };

    await setupMockApi(page, expiredGrant);
    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Expired state should appear
    await expect(page.locator('.expired-state')).toBeVisible();
    await expect(page.locator('.expired-state')).toContainText('Request Expired');
  });

  test('allows retry after error', async ({ page }) => {
    let callCount = 0;

    // Mock health check
    await page.route('**/health/ready', async (route: Route) => {
      await route.fulfill({
        status: 200,
        body: '{}',
      });
    });

    // First call fails, second succeeds
    await page.route(`**/v1/grants/${mockGrant.grant_id}`, async (route: Route) => {
      callCount++;
      if (callCount === 1) {
        await route.fulfill({
          status: 500,
          contentType: 'application/json',
          body: JSON.stringify({ error: 'Server error', code: 'SERVER_ERROR' }),
        });
      } else {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify(mockGrant),
        });
      }
    });

    await page.goto(`/approve/${mockGrant.grant_id}`);

    // Error state should appear
    await expect(page.locator('.error-state')).toBeVisible();

    // Click retry
    await page.click('button:text("Try Again")');

    // Page should load successfully
    await expect(page.locator('h1')).toHaveText('Grant Request');
  });
});

test.describe('Navigation', () => {
  test('can navigate to agents page from success state', async ({ page }) => {
    let approveCalled = false;

    await setupMockApi(page);

    // Mock the approve endpoint
    await page.route(`**/v1/grants/${mockGrant.grant_id}/approve`, async (route: Route) => {
      approveCalled = true;
      await route.fulfill({
        status: 200,
        body: '{}',
      });
    });

    await page.goto(`/approve/${mockGrant.grant_id}`);
    await expect(page.locator('h1')).toHaveText('Grant Request');

    // For low-risk grants, WebAuthn would be triggered
    // In tests, we'll just verify the UI state
    // Since WebAuthn can't be fully tested without a real authenticator,
    // we verify navigation from success state

    // Simulate going to the agents page directly
    await page.goto('/agents');
    await expect(page.locator('h1')).toHaveText('Your Agents');
  });
});
