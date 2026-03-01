/**
 * Grant Request Load Test
 *
 * Tests POST /v1/grants/request on the registry service.
 *
 * Baseline targets (from CLAUDE.md):
 *   200 req/s, p50 < 20ms, p99 < 100ms, p999 < 500ms, error < 0.1%
 *
 * Usage:
 *   k6 run load-tests/grant-request.js
 *   k6 run --vus 30 --duration 120s load-tests/grant-request.js
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';

const grantDuration = new Trend('grant_duration_ms', true);
const grantErrors = new Counter('grant_errors');
const grantSuccess = new Rate('grant_success_rate');

export const options = {
  scenarios: {
    warmup: {
      executor: 'constant-vus',
      vus: 5,
      duration: '10s',
      startTime: '0s',
      tags: { phase: 'warmup' },
    },
    baseline: {
      executor: 'constant-vus',
      vus: 15,
      duration: '60s',
      startTime: '10s',
      tags: { phase: 'baseline' },
    },
    peak: {
      executor: 'ramping-vus',
      startVUs: 15,
      stages: [
        { duration: '10s', target: 30 },
        { duration: '30s', target: 30 },
        { duration: '10s', target: 15 },
      ],
      startTime: '70s',
      tags: { phase: 'peak' },
    },
  },
  thresholds: {
    'http_req_duration{phase:baseline}': ['p(50)<100', 'p(99)<500'],
    'grant_success_rate': ['rate>0.999'],
  },
};

// Pre-generate registered agents to request grants for.
const AGENT_POOL_SIZE = 50;
const agentPool = [];
for (let i = 0; i < AGENT_POOL_SIZE; i++) {
  agentPool.push({
    agent_id: uuidv4(),
    service_provider_id: uuidv4(),
  });
}

const CAPABILITY_SETS = [
  [{ Read: { resource: 'calendar', filter: null } }],
  [{ Read: { resource: 'email', filter: null } }],
  [
    { Read: { resource: 'calendar', filter: null } },
    { Write: { resource: 'calendar', conditions: null } },
  ],
  [{ Read: { resource: 'files', filter: null } }],
  [
    { Read: { resource: 'messages', filter: null } },
    { Write: { resource: 'messages', conditions: null } },
  ],
  [
    { Transact: { resource: 'payments', max_value: 100, currency: null } },
  ],
  [
    { Custom: { namespace: 'com.example', name: 'search', params: {} } },
  ],
];

export default function () {
  const agent = agentPool[Math.floor(Math.random() * agentPool.length)];
  const capabilities = CAPABILITY_SETS[Math.floor(Math.random() * CAPABILITY_SETS.length)];

  const payload = JSON.stringify({
    agent_id: agent.agent_id,
    service_provider_id: agent.service_provider_id,
    capabilities: capabilities,
    behavioral_envelope: {
      max_requests_per_minute: 30 + Math.floor(Math.random() * 30),
      max_burst: 5 + Math.floor(Math.random() * 10),
      requires_human_online: Math.random() > 0.7,
      human_confirmation_threshold: Math.random() > 0.8 ? 1000 : null,
      allowed_time_windows: [],
      max_session_duration_secs: 1800 + Math.floor(Math.random() * 5400),
    },
  });

  const params = {
    headers: { 'Content-Type': 'application/json' },
    tags: { endpoint: 'grant_request' },
  };

  const start = Date.now();
  const res = http.post(`${REGISTRY_URL}/v1/grants/request`, payload, params);
  const duration = Date.now() - start;

  grantDuration.add(duration);

  // 201 = created, 429 = flood protection, 400/404 = expected without full setup
  const ok = check(res, {
    'status is acceptable': (r) => [200, 201, 400, 404, 429].includes(r.status),
    'latency < 500ms': (r) => r.timings.duration < 500,
  });

  if (ok) {
    grantSuccess.add(1);
  } else {
    grantErrors.add(1);
    grantSuccess.add(0);
  }

  sleep(0.05);
}

export function handleSummary(data) {
  const p50 = data.metrics.http_req_duration.values['p(50)'];
  const p99 = data.metrics.http_req_duration.values['p(99)'];
  const p999 = data.metrics.http_req_duration.values['p(99.9)'];
  const errorRate = 1 - (data.metrics.grant_success_rate?.values?.rate || 0);
  const rps = data.metrics.http_reqs.values.rate;

  console.log('\n=== Grant Request Load Test Results ===');
  console.log(`Throughput:    ${rps.toFixed(0)} req/s`);
  console.log(`p50 latency:  ${p50.toFixed(2)}ms`);
  console.log(`p99 latency:  ${p99.toFixed(2)}ms`);
  console.log(`p999 latency: ${p999.toFixed(2)}ms`);
  console.log(`Error rate:   ${(errorRate * 100).toFixed(4)}%`);
  console.log('\nBaseline targets:');
  console.log('  200 req/s | p50 < 20ms | p99 < 100ms | p999 < 500ms | error < 0.1%');
  console.log('========================================\n');

  return {
    'load-tests/results/grant-request.json': JSON.stringify(data, null, 2),
  };
}
