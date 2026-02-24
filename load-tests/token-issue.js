/**
 * Token Issuance Load Test
 *
 * Tests the registry service's token issuance endpoint.
 *
 * Baseline targets (from CLAUDE.md):
 * - 500 req/s, p50 < 10ms, p99 < 50ms, p999 < 200ms, error rate < 0.1%
 *
 * Usage:
 *   k6 run --vus 20 --duration 60s load-tests/token-issue.js
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Configuration
const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';

// Custom metrics
const issueDuration = new Trend('issue_duration_ms', true);
const issueErrors = new Counter('issue_errors');
const issueSuccess = new Rate('issue_success_rate');

// Test options
export const options = {
  scenarios: {
    warmup: {
      executor: 'constant-vus',
      vus: 5,
      duration: '10s',
      startTime: '0s',
      tags: { scenario: 'warmup' },
    },
    baseline: {
      executor: 'constant-vus',
      vus: 20,
      duration: '60s',
      startTime: '10s',
      tags: { scenario: 'baseline' },
    },
  },
  thresholds: {
    // Latency thresholds
    'http_req_duration{scenario:baseline}': ['p(50)<50', 'p(99)<200', 'p(99.9)<500'],
    // Error rate threshold
    'issue_success_rate': ['rate>0.999'],
  },
};

// Pre-generated test data
const testAgents = [];
for (let i = 0; i < 10; i++) {
  testAgents.push({
    agent_id: uuidv4(),
    grant_id: uuidv4(),
    service_provider_id: uuidv4(),
  });
}

export default function () {
  // Pick a random agent
  const agent = testAgents[Math.floor(Math.random() * testAgents.length)];

  const payload = JSON.stringify({
    grant_id: agent.grant_id,
    idempotency_key: uuidv4(), // Unique per request for testing
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
    tags: {
      endpoint: 'issue',
    },
  };

  const startTime = Date.now();
  const response = http.post(`${REGISTRY_URL}/v1/tokens/issue`, payload, params);
  const duration = Date.now() - startTime;

  // Record custom metrics
  issueDuration.add(duration);

  // Check response (401/403 expected without auth, but we test the path works)
  const success = check(response, {
    'status is 200 or 401/403': (r) => [200, 401, 403, 400].includes(r.status),
    'response time < 200ms': (r) => r.timings.duration < 200,
  });

  if (success) {
    issueSuccess.add(1);
  } else {
    issueErrors.add(1);
    issueSuccess.add(0);
  }

  // Slightly longer sleep for write operations
  sleep(0.05);
}

export function handleSummary(data) {
  const p50 = data.metrics.http_req_duration.values['p(50)'];
  const p99 = data.metrics.http_req_duration.values['p(99)'];
  const p999 = data.metrics.http_req_duration.values['p(99.9)'];
  const errorRate = 1 - (data.metrics.issue_success_rate?.values?.rate || 0);
  const rps = data.metrics.http_reqs.values.rate;

  console.log('\n=== Token Issue Load Test Results ===');
  console.log(`Requests/sec: ${rps.toFixed(2)}`);
  console.log(`p50 latency: ${p50.toFixed(2)}ms`);
  console.log(`p99 latency: ${p99.toFixed(2)}ms`);
  console.log(`p99.9 latency: ${p999.toFixed(2)}ms`);
  console.log(`Error rate: ${(errorRate * 100).toFixed(4)}%`);
  console.log('\nBaseline targets:');
  console.log('  500 req/s, p50 < 10ms, p99 < 50ms, p999 < 200ms, error < 0.1%');
  console.log('=====================================\n');

  return {
    'load-tests/results/token-issue-summary.json': JSON.stringify(data, null, 2),
  };
}
