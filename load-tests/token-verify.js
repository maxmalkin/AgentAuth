/**
 * Token Verification Load Test
 *
 * Tests the verifier service's token verification endpoint.
 *
 * Baseline targets (from CLAUDE.md):
 * - Redis warm: 10,000 req/s, p50 < 1ms, p99 < 5ms, p999 < 15ms, error rate < 0.01%
 * - Cold (DB fallback): 1,000 req/s, p50 < 5ms, p99 < 20ms, p999 < 50ms, error rate < 0.01%
 *
 * Usage:
 *   k6 run --vus 50 --duration 60s load-tests/token-verify.js
 *   k6 run --vus 100 --duration 300s load-tests/token-verify.js  # Full baseline test
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

// Configuration
const VERIFIER_URL = __ENV.VERIFIER_URL || 'http://localhost:8081';

// Custom metrics
const verifyDuration = new Trend('verify_duration_ms', true);
const verifyErrors = new Counter('verify_errors');
const verifySuccess = new Rate('verify_success_rate');

// Test options
export const options = {
  // Default scenario: ramp up to target VUs
  scenarios: {
    warmup: {
      executor: 'constant-vus',
      vus: 10,
      duration: '10s',
      startTime: '0s',
      tags: { scenario: 'warmup' },
    },
    baseline: {
      executor: 'constant-vus',
      vus: 50,
      duration: '60s',
      startTime: '10s',
      tags: { scenario: 'baseline' },
    },
    spike: {
      executor: 'ramping-vus',
      startVUs: 50,
      stages: [
        { duration: '10s', target: 100 },
        { duration: '20s', target: 100 },
        { duration: '10s', target: 50 },
      ],
      startTime: '70s',
      tags: { scenario: 'spike' },
    },
  },
  thresholds: {
    // Latency thresholds (Redis warm path)
    'http_req_duration{scenario:baseline}': ['p(50)<5', 'p(99)<20', 'p(99.9)<50'],
    // Error rate threshold
    'verify_success_rate': ['rate>0.9999'],
    // Custom verify duration
    'verify_duration_ms': ['p(50)<5', 'p(99)<20'],
  },
};

// Pre-generated test tokens (in real test, these would be valid tokens from the registry)
// For now, we use placeholder JTIs that the verifier will process
const testTokens = [];
for (let i = 0; i < 100; i++) {
  testTokens.push({
    jti: uuidv4(),
    service_provider_id: uuidv4(),
    nonce: uuidv4(),
  });
}

export default function () {
  // Pick a random test token
  const token = testTokens[Math.floor(Math.random() * testTokens.length)];

  const payload = JSON.stringify({
    jti: token.jti,
    service_provider_id: token.service_provider_id,
    nonce: uuidv4(), // Fresh nonce for each request
    dpop_proof: null, // Optional in dev mode
    dpop_thumbprint: null,
  });

  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
    tags: {
      endpoint: 'verify',
    },
  };

  const startTime = Date.now();
  const response = http.post(`${VERIFIER_URL}/v1/tokens/verify`, payload, params);
  const duration = Date.now() - startTime;

  // Record custom metrics
  verifyDuration.add(duration);

  // Check response
  const success = check(response, {
    'status is 200': (r) => r.status === 200,
    'response has valid field': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.valid !== undefined;
      } catch {
        return false;
      }
    },
    'response time < 50ms': (r) => r.timings.duration < 50,
  });

  if (success) {
    verifySuccess.add(1);
  } else {
    verifyErrors.add(1);
    verifySuccess.add(0);
  }

  // Small sleep to prevent overwhelming the system
  sleep(0.01);
}

export function handleSummary(data) {
  const p50 = data.metrics.http_req_duration.values['p(50)'];
  const p99 = data.metrics.http_req_duration.values['p(99)'];
  const p999 = data.metrics.http_req_duration.values['p(99.9)'];
  const errorRate = 1 - (data.metrics.verify_success_rate?.values?.rate || 0);
  const rps = data.metrics.http_reqs.values.rate;

  console.log('\n=== Token Verify Load Test Results ===');
  console.log(`Requests/sec: ${rps.toFixed(2)}`);
  console.log(`p50 latency: ${p50.toFixed(2)}ms`);
  console.log(`p99 latency: ${p99.toFixed(2)}ms`);
  console.log(`p99.9 latency: ${p999.toFixed(2)}ms`);
  console.log(`Error rate: ${(errorRate * 100).toFixed(4)}%`);
  console.log('\nBaseline targets (Redis warm):');
  console.log('  p50 < 1ms, p99 < 5ms, p999 < 15ms, error < 0.01%');
  console.log('=======================================\n');

  return {
    'load-tests/results/token-verify-summary.json': JSON.stringify(data, null, 2),
  };
}
