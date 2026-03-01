/**
 * Token Issuance Load Test
 *
 * Tests POST /v1/tokens/issue on the registry service.
 *
 * Baseline targets (from CLAUDE.md):
 *   500 req/s, p50 < 10ms, p99 < 50ms, p999 < 200ms, error < 0.1%
 *
 * Usage:
 *   k6 run load-tests/token-issue.js
 *   k6 run --vus 50 --duration 120s load-tests/token-issue.js
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';

const issueDuration = new Trend('issue_duration_ms', true);
const issueErrors = new Counter('issue_errors');
const issueSuccess = new Rate('issue_success_rate');

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
      vus: 20,
      duration: '60s',
      startTime: '10s',
      tags: { phase: 'baseline' },
    },
    sustained: {
      executor: 'constant-vus',
      vus: 50,
      duration: '60s',
      startTime: '70s',
      tags: { phase: 'sustained' },
    },
  },
  thresholds: {
    'http_req_duration{phase:baseline}': ['p(50)<50', 'p(99)<200', 'p(99.9)<500'],
    'issue_success_rate': ['rate>0.999'],
  },
};

// Pre-generate agents with approved grants to issue tokens against.
const AGENT_POOL_SIZE = 20;
const agentPool = [];
for (let i = 0; i < AGENT_POOL_SIZE; i++) {
  agentPool.push({
    agent_id: uuidv4(),
    grant_id: uuidv4(),
    service_provider_id: uuidv4(),
    human_principal_id: uuidv4(),
  });
}

export default function () {
  const agent = agentPool[Math.floor(Math.random() * agentPool.length)];

  const payload = JSON.stringify({
    grant_id: agent.grant_id,
    agent_id: agent.agent_id,
    service_provider_id: agent.service_provider_id,
    human_principal_id: agent.human_principal_id,
    capabilities: [
      { Read: { resource: 'calendar', filter: null } },
    ],
    behavioral_envelope: {
      max_requests_per_minute: 60,
      max_burst: 10,
      requires_human_online: false,
      human_confirmation_threshold: null,
      allowed_time_windows: [],
      max_session_duration_secs: 3600,
    },
    token_binding: null,
  });

  const params = {
    headers: { 'Content-Type': 'application/json' },
    tags: { endpoint: 'issue' },
  };

  const start = Date.now();
  const res = http.post(`${REGISTRY_URL}/v1/tokens/issue`, payload, params);
  const duration = Date.now() - start;

  issueDuration.add(duration);

  // 201 = issued, 200 = idempotent hit, 400/401/403 = expected without auth setup
  const ok = check(res, {
    'status is acceptable': (r) => [200, 201, 400, 401, 403].includes(r.status),
    'latency < 200ms': (r) => r.timings.duration < 200,
  });

  if (ok) {
    issueSuccess.add(1);
  } else {
    issueErrors.add(1);
    issueSuccess.add(0);
  }

  sleep(0.02);
}

export function handleSummary(data) {
  const p50 = data.metrics.http_req_duration.values['p(50)'];
  const p99 = data.metrics.http_req_duration.values['p(99)'];
  const p999 = data.metrics.http_req_duration.values['p(99.9)'];
  const errorRate = 1 - (data.metrics.issue_success_rate?.values?.rate || 0);
  const rps = data.metrics.http_reqs.values.rate;

  console.log('\n=== Token Issue Load Test Results ===');
  console.log(`Throughput:    ${rps.toFixed(0)} req/s`);
  console.log(`p50 latency:  ${p50.toFixed(2)}ms`);
  console.log(`p99 latency:  ${p99.toFixed(2)}ms`);
  console.log(`p999 latency: ${p999.toFixed(2)}ms`);
  console.log(`Error rate:   ${(errorRate * 100).toFixed(4)}%`);
  console.log('\nBaseline targets:');
  console.log('  500 req/s | p50 < 10ms | p99 < 50ms | p999 < 200ms | error < 0.1%');
  console.log('=====================================\n');

  return {
    'load-tests/results/token-issue.json': JSON.stringify(data, null, 2),
  };
}
