/**
 * Token Verification Load Test
 *
 * Tests POST /v1/tokens/verify on the verifier service.
 *
 * Baseline targets (from CLAUDE.md):
 *   Redis warm: 10,000 req/s, p50 < 1ms, p99 < 5ms, p999 < 15ms, error < 0.01%
 *   Cold (DB fallback): 1,000 req/s, p50 < 5ms, p99 < 20ms, p999 < 50ms, error < 0.01%
 *
 * Usage:
 *   k6 run load-tests/token-verify.js
 *   k6 run --vus 100 --duration 300s load-tests/token-verify.js
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';
import { randomBytes } from 'k6/crypto';

const VERIFIER_URL = __ENV.VERIFIER_URL || 'http://localhost:8081';

const verifyDuration = new Trend('verify_duration_ms', true);
const verifyErrors = new Counter('verify_errors');
const verifySuccess = new Rate('verify_success_rate');
const cacheHits = new Counter('cache_hits');
const cacheMisses = new Counter('cache_misses');

export const options = {
  scenarios: {
    warmup: {
      executor: 'constant-vus',
      vus: 10,
      duration: '10s',
      startTime: '0s',
      tags: { phase: 'warmup' },
    },
    baseline: {
      executor: 'constant-vus',
      vus: 50,
      duration: '60s',
      startTime: '10s',
      tags: { phase: 'baseline' },
    },
    spike: {
      executor: 'ramping-vus',
      startVUs: 50,
      stages: [
        { duration: '10s', target: 200 },
        { duration: '30s', target: 200 },
        { duration: '10s', target: 50 },
      ],
      startTime: '70s',
      tags: { phase: 'spike' },
    },
  },
  thresholds: {
    'http_req_duration{phase:baseline}': ['p(50)<5', 'p(99)<20', 'p(99.9)<50'],
    'verify_success_rate': ['rate>0.9999'],
  },
};

// Pre-generate a pool of token JTIs and service provider IDs to simulate
// realistic cache hit patterns (same tokens verified multiple times).
const TOKEN_POOL_SIZE = 100;
const tokenPool = [];
for (let i = 0; i < TOKEN_POOL_SIZE; i++) {
  tokenPool.push({
    jti: uuidv4(),
    service_provider_id: uuidv4(),
  });
}

export default function () {
  const token = tokenPool[Math.floor(Math.random() * tokenPool.length)];

  const payload = JSON.stringify({
    jti: token.jti,
    service_provider_id: token.service_provider_id,
    nonce: uuidv4(),
    dpop_proof: null,
    dpop_thumbprint: null,
  });

  const params = {
    headers: { 'Content-Type': 'application/json' },
    tags: { endpoint: 'verify' },
  };

  const start = Date.now();
  const res = http.post(`${VERIFIER_URL}/v1/tokens/verify`, payload, params);
  const duration = Date.now() - start;

  verifyDuration.add(duration);

  const ok = check(res, {
    'status is 200 or 503': (r) => r.status === 200 || r.status === 503,
    'has outcome field': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.outcome !== undefined;
      } catch {
        return false;
      }
    },
    'latency < 50ms': (r) => r.timings.duration < 50,
  });

  if (ok) {
    verifySuccess.add(1);
  } else {
    verifyErrors.add(1);
    verifySuccess.add(0);
  }

  // Track cache behavior from response
  if (res.status === 200) {
    try {
      const body = JSON.parse(res.body);
      if (body.outcome === 'allowed' || body.outcome === 'expired' || body.outcome === 'revoked') {
        cacheHits.add(1);
      } else if (body.outcome === 'not_found') {
        cacheMisses.add(1);
      }
    } catch { /* ignore parse errors */ }
  }

  sleep(0.01);
}

export function handleSummary(data) {
  const p50 = data.metrics.http_req_duration.values['p(50)'];
  const p99 = data.metrics.http_req_duration.values['p(99)'];
  const p999 = data.metrics.http_req_duration.values['p(99.9)'];
  const errorRate = 1 - (data.metrics.verify_success_rate?.values?.rate || 0);
  const rps = data.metrics.http_reqs.values.rate;

  console.log('\n=== Token Verify Load Test Results ===');
  console.log(`Throughput:    ${rps.toFixed(0)} req/s`);
  console.log(`p50 latency:  ${p50.toFixed(2)}ms`);
  console.log(`p99 latency:  ${p99.toFixed(2)}ms`);
  console.log(`p999 latency: ${p999.toFixed(2)}ms`);
  console.log(`Error rate:   ${(errorRate * 100).toFixed(4)}%`);
  console.log('\nBaseline targets (Redis warm):');
  console.log('  10,000 req/s | p50 < 1ms | p99 < 5ms | p999 < 15ms | error < 0.01%');
  console.log('=======================================\n');

  return {
    'load-tests/results/token-verify.json': JSON.stringify(data, null, 2),
  };
}
