/**
 * Mixed Traffic Load Test
 *
 * Simulates realistic production traffic distribution:
 *   - 80% token verification (high frequency, read-heavy)
 *   - 10% token issuance
 *   - 5%  grant requests
 *   - 3%  agent lookups
 *   - 2%  health checks
 *
 * Usage:
 *   k6 run load-tests/scenarios/mixed-traffic.js
 *   k6 run --vus 100 --duration 300s load-tests/scenarios/mixed-traffic.js
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';
const VERIFIER_URL = __ENV.VERIFIER_URL || 'http://localhost:8081';
const JSON_HEADERS = { headers: { 'Content-Type': 'application/json' } };

const requestSuccess = new Rate('request_success_rate');
const verifyLatency = new Trend('verify_latency_ms', true);
const issueLatency = new Trend('issue_latency_ms', true);
const grantLatency = new Trend('grant_latency_ms', true);
const endpointErrors = new Counter('endpoint_errors');

export const options = {
  scenarios: {
    ramp: {
      executor: 'ramping-vus',
      startVUs: 5,
      stages: [
        { duration: '30s', target: 50 },
        { duration: '120s', target: 50 },
        { duration: '30s', target: 100 },
        { duration: '60s', target: 100 },
        { duration: '30s', target: 0 },
      ],
    },
  },
  thresholds: {
    'request_success_rate': ['rate>0.99'],
    'verify_latency_ms': ['p(99)<50'],
    'issue_latency_ms': ['p(99)<200'],
    'grant_latency_ms': ['p(99)<500'],
  },
};

// Shared state pools
const TOKEN_POOL = [];
const AGENT_POOL = [];
for (let i = 0; i < 200; i++) {
  TOKEN_POOL.push({ jti: uuidv4(), sp: uuidv4() });
}
for (let i = 0; i < 50; i++) {
  AGENT_POOL.push({
    id: uuidv4(), sp: uuidv4(), hp: uuidv4(), grant: uuidv4(),
  });
}

function doVerify() {
  const t = TOKEN_POOL[Math.floor(Math.random() * TOKEN_POOL.length)];
  const payload = JSON.stringify({
    jti: t.jti,
    service_provider_id: t.sp,
    nonce: uuidv4(),
    dpop_proof: null,
    dpop_thumbprint: null,
  });

  const start = Date.now();
  const res = http.post(`${VERIFIER_URL}/v1/tokens/verify`, payload, JSON_HEADERS);
  verifyLatency.add(Date.now() - start);
  return res.status === 200 || res.status === 503;
}

function doIssue() {
  const a = AGENT_POOL[Math.floor(Math.random() * AGENT_POOL.length)];
  const payload = JSON.stringify({
    grant_id: a.grant,
    agent_id: a.id,
    service_provider_id: a.sp,
    human_principal_id: a.hp,
    capabilities: [{ Read: { resource: 'calendar', filter: null } }],
    behavioral_envelope: {
      max_requests_per_minute: 60, max_burst: 10,
      requires_human_online: false, human_confirmation_threshold: null,
      allowed_time_windows: [], max_session_duration_secs: 3600,
    },
    token_binding: null,
  });

  const start = Date.now();
  const res = http.post(`${REGISTRY_URL}/v1/tokens/issue`, payload, JSON_HEADERS);
  issueLatency.add(Date.now() - start);
  return [200, 201, 400, 401, 403].includes(res.status);
}

function doGrantRequest() {
  const a = AGENT_POOL[Math.floor(Math.random() * AGENT_POOL.length)];
  const payload = JSON.stringify({
    agent_id: a.id,
    service_provider_id: uuidv4(),
    capabilities: [{ Read: { resource: 'calendar', filter: null } }],
    behavioral_envelope: {
      max_requests_per_minute: 30, max_burst: 5,
      requires_human_online: false, human_confirmation_threshold: null,
      allowed_time_windows: [], max_session_duration_secs: 3600,
    },
  });

  const start = Date.now();
  const res = http.post(`${REGISTRY_URL}/v1/grants/request`, payload, JSON_HEADERS);
  grantLatency.add(Date.now() - start);
  return [200, 201, 400, 404, 429].includes(res.status);
}

function doAgentLookup() {
  const a = AGENT_POOL[Math.floor(Math.random() * AGENT_POOL.length)];
  const res = http.get(`${REGISTRY_URL}/v1/agents/${a.id}`);
  return [200, 404].includes(res.status);
}

function doHealthCheck() {
  const targets = [
    `${REGISTRY_URL}/health/ready`,
    `${VERIFIER_URL}/health/ready`,
  ];
  const url = targets[Math.floor(Math.random() * targets.length)];
  const res = http.get(url);
  return [200, 503].includes(res.status);
}

export default function () {
  const roll = Math.random();
  let ok;

  if (roll < 0.80) {
    ok = doVerify();
  } else if (roll < 0.90) {
    ok = doIssue();
  } else if (roll < 0.95) {
    ok = doGrantRequest();
  } else if (roll < 0.98) {
    ok = doAgentLookup();
  } else {
    ok = doHealthCheck();
  }

  if (ok) {
    requestSuccess.add(1);
  } else {
    endpointErrors.add(1);
    requestSuccess.add(0);
  }

  sleep(0.01);
}

export function handleSummary(data) {
  console.log('\n=== Mixed Traffic Load Test Results ===');
  console.log(`Overall success rate: ${((data.metrics.request_success_rate?.values?.rate || 0) * 100).toFixed(2)}%`);
  console.log(`Total requests:      ${data.metrics.http_reqs?.values?.count || 0}`);
  console.log(`Throughput:          ${(data.metrics.http_reqs?.values?.rate || 0).toFixed(0)} req/s`);
  console.log('');

  const latencies = [
    ['Verify', 'verify_latency_ms'],
    ['Issue',  'issue_latency_ms'],
    ['Grant',  'grant_latency_ms'],
  ];

  console.log('Endpoint latencies (p50 / p99):');
  for (const [name, key] of latencies) {
    const m = data.metrics[key];
    if (m) {
      console.log(`  ${name.padEnd(8)} ${m.values['p(50)'].toFixed(1)}ms / ${m.values['p(99)'].toFixed(1)}ms`);
    }
  }
  console.log('=======================================\n');

  return {
    'load-tests/results/mixed-traffic.json': JSON.stringify(data, null, 2),
  };
}
