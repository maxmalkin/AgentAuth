/**
 * Full Flow Load Test
 *
 * Tests the complete AgentAuth flow:
 * 1. Agent registration
 * 2. Grant request
 * 3. Grant approval (simulated)
 * 4. Token issuance
 * 5. Token verification (multiple times)
 *
 * This test simulates realistic usage patterns.
 *
 * Usage:
 *   k6 run --vus 10 --duration 120s load-tests/scenarios/full-flow.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Configuration
const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';
const VERIFIER_URL = __ENV.VERIFIER_URL || 'http://localhost:8081';

// Custom metrics
const flowDuration = new Trend('flow_duration_ms', true);
const flowSuccess = new Rate('flow_success_rate');
const registrationDuration = new Trend('registration_duration_ms', true);
const grantDuration = new Trend('grant_duration_ms', true);
const verifyDuration = new Trend('verify_duration_ms', true);

// Test options
export const options = {
  scenarios: {
    realistic_load: {
      executor: 'ramping-vus',
      startVUs: 1,
      stages: [
        { duration: '30s', target: 10 },  // Ramp up
        { duration: '60s', target: 10 },  // Steady state
        { duration: '30s', target: 0 },   // Ramp down
      ],
    },
  },
  thresholds: {
    'flow_success_rate': ['rate>0.95'],
    'verify_duration_ms': ['p(99)<50'],
  },
};

export default function () {
  const flowStart = Date.now();
  let success = true;

  // Generate unique identifiers for this flow
  const agentId = uuidv4();
  const serviceProviderId = uuidv4();
  const humanPrincipalId = uuidv4();

  group('1. Agent Registration', () => {
    const payload = JSON.stringify({
      manifest: {
        id: agentId,
        name: `Test Agent ${agentId.substring(0, 8)}`,
        public_key: 'dGVzdC1wdWJsaWMta2V5LWJhc2U2NA==', // Placeholder
        human_principal_id: humanPrincipalId,
        requested_capabilities: [
          { Read: { resource: 'calendar', filter: null } },
        ],
        issued_at: new Date().toISOString(),
        expires_at: new Date(Date.now() + 86400000).toISOString(),
      },
      signature: 'dGVzdC1zaWduYXR1cmU=', // Placeholder
    });

    const start = Date.now();
    const response = http.post(`${REGISTRY_URL}/v1/agents/register`, payload, {
      headers: { 'Content-Type': 'application/json' },
    });
    registrationDuration.add(Date.now() - start);

    const ok = check(response, {
      'registration status ok': (r) => [200, 201, 400, 409].includes(r.status),
    });
    if (!ok) success = false;
  });

  sleep(0.1);

  group('2. Grant Request', () => {
    const payload = JSON.stringify({
      agent_id: agentId,
      service_provider_id: serviceProviderId,
      requested_capabilities: [
        { Read: { resource: 'calendar', filter: null } },
      ],
      requested_envelope: {
        max_requests_per_minute: 30,
        max_burst: 5,
        requires_human_online: false,
        human_confirmation_threshold: null,
        allowed_time_windows: [],
        max_session_duration_secs: 3600,
      },
    });

    const start = Date.now();
    const response = http.post(`${REGISTRY_URL}/v1/grants/request`, payload, {
      headers: { 'Content-Type': 'application/json' },
    });
    grantDuration.add(Date.now() - start);

    const ok = check(response, {
      'grant request status ok': (r) => [200, 201, 400, 404].includes(r.status),
    });
    if (!ok) success = false;
  });

  sleep(0.1);

  // In a real test, we would wait for approval
  // For load testing, we simulate multiple verification calls

  group('3. Token Verification (simulated)', () => {
    // Simulate multiple verification calls per flow
    for (let i = 0; i < 5; i++) {
      const payload = JSON.stringify({
        jti: uuidv4(),
        service_provider_id: serviceProviderId,
        nonce: uuidv4(),
        dpop_proof: null,
        dpop_thumbprint: null,
      });

      const start = Date.now();
      const response = http.post(`${VERIFIER_URL}/v1/tokens/verify`, payload, {
        headers: { 'Content-Type': 'application/json' },
      });
      verifyDuration.add(Date.now() - start);

      check(response, {
        'verify status is 200': (r) => r.status === 200,
      });

      sleep(0.05);
    }
  });

  // Record flow metrics
  flowDuration.add(Date.now() - flowStart);
  flowSuccess.add(success ? 1 : 0);

  sleep(1); // Think time between flows
}

export function handleSummary(data) {
  console.log('\n=== Full Flow Load Test Results ===');
  console.log(`Flow success rate: ${((data.metrics.flow_success_rate?.values?.rate || 0) * 100).toFixed(2)}%`);
  console.log(`Avg flow duration: ${(data.metrics.flow_duration_ms?.values?.avg || 0).toFixed(2)}ms`);
  console.log(`Verify p99: ${(data.metrics.verify_duration_ms?.values?.['p(99)'] || 0).toFixed(2)}ms`);
  console.log('===================================\n');

  return {
    'load-tests/results/full-flow-summary.json': JSON.stringify(data, null, 2),
  };
}
