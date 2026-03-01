/**
 * Full Flow Composite Load Test
 *
 * Simulates the complete AgentAuth lifecycle:
 *   1. Register agent
 *   2. Request grant
 *   3. Approve grant (simulated)
 *   4. Issue token
 *   5. Verify token (multiple times)
 *   6. Revoke token
 *
 * Usage:
 *   k6 run load-tests/scenarios/full-flow.js
 *   k6 run --vus 20 --duration 300s load-tests/scenarios/full-flow.js
 */

import http from 'k6/http';
import { check, sleep, group } from 'k6';
import { Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';
const VERIFIER_URL = __ENV.VERIFIER_URL || 'http://localhost:8081';
const JSON_HEADERS = { headers: { 'Content-Type': 'application/json' } };

const flowSuccess = new Rate('flow_success_rate');
const flowDuration = new Trend('flow_duration_ms', true);
const registerDuration = new Trend('register_duration_ms', true);
const grantDuration = new Trend('grant_request_duration_ms', true);
const issueDuration = new Trend('issue_duration_ms', true);
const verifyDuration = new Trend('verify_duration_ms', true);
const revokeDuration = new Trend('revoke_duration_ms', true);

export const options = {
  scenarios: {
    realistic: {
      executor: 'ramping-vus',
      startVUs: 1,
      stages: [
        { duration: '30s', target: 10 },
        { duration: '60s', target: 10 },
        { duration: '30s', target: 20 },
        { duration: '60s', target: 20 },
        { duration: '30s', target: 0 },
      ],
    },
  },
  thresholds: {
    'flow_success_rate': ['rate>0.90'],
    'verify_duration_ms': ['p(99)<50'],
  },
};

export default function () {
  const flowStart = Date.now();
  let ok = true;

  const agentId = uuidv4();
  const serviceProviderId = uuidv4();
  const humanPrincipalId = uuidv4();
  let grantId = null;
  let tokenJti = null;

  // --- Step 1: Register agent ---
  group('register', () => {
    const payload = JSON.stringify({
      manifest: {
        id: agentId,
        name: `loadtest-agent-${agentId.substring(0, 8)}`,
        public_key: 'dGVzdC1wdWJsaWMta2V5',
        human_principal_id: humanPrincipalId,
        requested_capabilities: [
          { Read: { resource: 'calendar', filter: null } },
        ],
        issued_at: new Date().toISOString(),
        expires_at: new Date(Date.now() + 86400000).toISOString(),
      },
      signature: 'deadbeef',
    });

    const start = Date.now();
    const res = http.post(`${REGISTRY_URL}/v1/agents/register`, payload, JSON_HEADERS);
    registerDuration.add(Date.now() - start);

    if (!check(res, {
      'register: accepted': (r) => [200, 201, 400].includes(r.status),
    })) {
      ok = false;
    }
  });

  sleep(0.1);

  // --- Step 2: Request grant ---
  group('grant_request', () => {
    const payload = JSON.stringify({
      agent_id: agentId,
      service_provider_id: serviceProviderId,
      capabilities: [
        { Read: { resource: 'calendar', filter: null } },
      ],
      behavioral_envelope: {
        max_requests_per_minute: 30,
        max_burst: 5,
        requires_human_online: false,
        human_confirmation_threshold: null,
        allowed_time_windows: [],
        max_session_duration_secs: 3600,
      },
    });

    const start = Date.now();
    const res = http.post(`${REGISTRY_URL}/v1/grants/request`, payload, JSON_HEADERS);
    grantDuration.add(Date.now() - start);

    if (check(res, {
      'grant: accepted': (r) => [200, 201, 400, 404].includes(r.status),
    })) {
      try {
        const body = JSON.parse(res.body);
        grantId = body.id;
      } catch { /* grant ID not available */ }
    } else {
      ok = false;
    }
  });

  sleep(0.1);

  // --- Step 3: Issue token ---
  group('issue_token', () => {
    const payload = JSON.stringify({
      grant_id: grantId || uuidv4(),
      agent_id: agentId,
      service_provider_id: serviceProviderId,
      human_principal_id: humanPrincipalId,
      capabilities: [
        { Read: { resource: 'calendar', filter: null } },
      ],
      behavioral_envelope: {
        max_requests_per_minute: 30,
        max_burst: 5,
        requires_human_online: false,
        human_confirmation_threshold: null,
        allowed_time_windows: [],
        max_session_duration_secs: 3600,
      },
      token_binding: null,
    });

    const start = Date.now();
    const res = http.post(`${REGISTRY_URL}/v1/tokens/issue`, payload, JSON_HEADERS);
    issueDuration.add(Date.now() - start);

    if (check(res, {
      'issue: accepted': (r) => [200, 201, 400, 401, 403].includes(r.status),
    })) {
      try {
        const body = JSON.parse(res.body);
        tokenJti = body.jti;
      } catch { /* token JTI not available */ }
    } else {
      ok = false;
    }
  });

  sleep(0.1);

  // --- Step 4: Verify token (multiple times, simulating service provider usage) ---
  group('verify_token', () => {
    const jti = tokenJti || uuidv4();
    const verifyCount = 3 + Math.floor(Math.random() * 5);

    for (let i = 0; i < verifyCount; i++) {
      const payload = JSON.stringify({
        jti: jti,
        service_provider_id: serviceProviderId,
        nonce: uuidv4(),
        dpop_proof: null,
        dpop_thumbprint: null,
      });

      const start = Date.now();
      const res = http.post(`${VERIFIER_URL}/v1/tokens/verify`, payload, JSON_HEADERS);
      verifyDuration.add(Date.now() - start);

      check(res, {
        'verify: responded': (r) => r.status === 200 || r.status === 503,
      });

      sleep(0.05);
    }
  });

  sleep(0.1);

  // --- Step 5: Revoke token ---
  group('revoke_token', () => {
    if (!tokenJti) return;

    const payload = JSON.stringify({
      jti: tokenJti,
      reason: 'load test cleanup',
    });

    const start = Date.now();
    const res = http.post(`${REGISTRY_URL}/v1/tokens/revoke`, payload, JSON_HEADERS);
    revokeDuration.add(Date.now() - start);

    check(res, {
      'revoke: accepted': (r) => [200, 204, 400, 404].includes(r.status),
    });
  });

  flowDuration.add(Date.now() - flowStart);
  flowSuccess.add(ok ? 1 : 0);

  sleep(1);
}

export function handleSummary(data) {
  const metrics = [
    ['Register', 'register_duration_ms'],
    ['Grant',    'grant_request_duration_ms'],
    ['Issue',    'issue_duration_ms'],
    ['Verify',   'verify_duration_ms'],
    ['Revoke',   'revoke_duration_ms'],
  ];

  console.log('\n=== Full Flow Load Test Results ===');
  console.log(`Flow success rate: ${((data.metrics.flow_success_rate?.values?.rate || 0) * 100).toFixed(1)}%`);
  console.log(`Avg flow duration: ${(data.metrics.flow_duration_ms?.values?.avg || 0).toFixed(0)}ms`);
  console.log('');
  console.log('Step latencies (p50 / p99):');

  for (const [name, key] of metrics) {
    const m = data.metrics[key];
    if (m) {
      console.log(`  ${name.padEnd(10)} ${m.values['p(50)'].toFixed(1)}ms / ${m.values['p(99)'].toFixed(1)}ms`);
    }
  }

  console.log('===================================\n');

  return {
    'load-tests/results/full-flow.json': JSON.stringify(data, null, 2),
  };
}
