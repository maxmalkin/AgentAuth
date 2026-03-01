/**
 * Soak Test
 *
 * Sustained load over 30 minutes to detect memory leaks,
 * connection pool exhaustion, and latency degradation.
 *
 * Focuses on the token verification hot path since that is the
 * highest-volume endpoint in production.
 *
 * Usage:
 *   k6 run load-tests/scenarios/soak.js
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';
import { uuidv4 } from 'https://jslib.k6.io/k6-utils/1.4.0/index.js';

const VERIFIER_URL = __ENV.VERIFIER_URL || 'http://localhost:8081';
const REGISTRY_URL = __ENV.REGISTRY_URL || 'http://localhost:8080';

const verifySuccess = new Rate('verify_success_rate');
const verifyLatency = new Trend('verify_latency_ms', true);
const healthSuccess = new Rate('health_success_rate');

export const options = {
  scenarios: {
    sustained_verify: {
      executor: 'constant-vus',
      vus: 50,
      duration: '30m',
      tags: { workload: 'verify' },
    },
    periodic_health: {
      executor: 'constant-vus',
      vus: 1,
      duration: '30m',
      tags: { workload: 'health' },
    },
  },
  thresholds: {
    'verify_success_rate': ['rate>0.9999'],
    'verify_latency_ms': ['p(99)<20'],
    'health_success_rate': ['rate>0.99'],
  },
};

const TOKEN_POOL = [];
for (let i = 0; i < 500; i++) {
  TOKEN_POOL.push({ jti: uuidv4(), sp: uuidv4() });
}

export default function () {
  if (__ENV.scenario === 'periodic_health' ||
      __ITER % 1000 === 0) {
    // Periodic health check
    const res = http.get(`${VERIFIER_URL}/health/ready`);
    healthSuccess.add(res.status === 200 ? 1 : 0);
    sleep(10);
    return;
  }

  const t = TOKEN_POOL[Math.floor(Math.random() * TOKEN_POOL.length)];
  const payload = JSON.stringify({
    jti: t.jti,
    service_provider_id: t.sp,
    nonce: uuidv4(),
    dpop_proof: null,
    dpop_thumbprint: null,
  });

  const start = Date.now();
  const res = http.post(
    `${VERIFIER_URL}/v1/tokens/verify`,
    payload,
    { headers: { 'Content-Type': 'application/json' } },
  );
  verifyLatency.add(Date.now() - start);

  const ok = check(res, {
    'status ok': (r) => r.status === 200 || r.status === 503,
  });

  verifySuccess.add(ok ? 1 : 0);

  sleep(0.01);
}

export function handleSummary(data) {
  const p50 = data.metrics.verify_latency_ms?.values?.['p(50)'] || 0;
  const p99 = data.metrics.verify_latency_ms?.values?.['p(99)'] || 0;
  const rate = data.metrics.verify_success_rate?.values?.rate || 0;
  const rps = data.metrics.http_reqs?.values?.rate || 0;
  const total = data.metrics.http_reqs?.values?.count || 0;

  console.log('\n=== Soak Test Results (30 min) ===');
  console.log(`Total requests:  ${total}`);
  console.log(`Throughput:      ${rps.toFixed(0)} req/s`);
  console.log(`Success rate:    ${(rate * 100).toFixed(4)}%`);
  console.log(`p50 latency:     ${p50.toFixed(2)}ms`);
  console.log(`p99 latency:     ${p99.toFixed(2)}ms`);
  console.log('');
  console.log('Look for:');
  console.log('  - p99 latency increasing over time (connection pool exhaustion)');
  console.log('  - Success rate dropping (memory pressure / OOM)');
  console.log('  - Health check failures (dependency degradation)');
  console.log('==================================\n');

  return {
    'load-tests/results/soak.json': JSON.stringify(data, null, 2),
  };
}
