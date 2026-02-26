# AgentAuth Operations Runbook

This runbook provides guidance for responding to alerts and operational issues in the AgentAuth system.

## Table of Contents

- [Token Verification Errors](#token-verify-errors)
- [Token Verification Latency](#token-verify-latency)
- [Circuit Breaker](#circuit-breaker)
- [Revocation Lag](#revocation-lag)
- [Audit Lag](#audit-lag)
- [Audit Buffer](#audit-buffer)
- [Nonce Store](#nonce-store)
- [Replica Lag](#replica-lag)
- [SLO Budget](#slo-budget)
- [OOM](#oom)
- [Cache Hit Ratio](#cache-hit-ratio)
- [Redis Unavailable](#redis-unavailable)
- [Archival Issues](#archival-issues)

---

## token-verify-errors

### What this alert means
Token verification requests are failing at a rate above 0.1%. This indicates a significant portion of authentication requests are not succeeding.

### Immediate mitigation steps (first 5 minutes)
1. Check the verifier logs for error patterns: `kubectl logs -l app=verifier -n agentauth --tail=100`
2. Check Redis connectivity: `redis-cli -h $REDIS_HOST ping`
3. Check if circuit breakers are open: Look at `agentauth_circuit_breaker_state` metrics
4. If Redis is down, verifier should fall back to PostgreSQL - verify this is working

### How to verify recovery
- Error rate drops below 0.1%
- `agentauth_tokens_verified_total{outcome="allowed"}` is increasing
- No new error logs appearing

### Root cause investigation steps
1. Correlate with deployment events - was there a recent deploy?
2. Check for Redis cluster issues
3. Check for database connectivity issues
4. Look for patterns in failed token JTIs - are specific agents affected?
5. Check if KMS is available for public key fetching

### Known false-positive conditions
- Brief spikes during rolling deployments
- Intentional load testing with invalid tokens

---

## token-verify-latency

### What this alert means
Token verification p99 latency is above 5ms. This may indicate Redis issues or increased database fallback.

### Immediate mitigation steps (first 5 minutes)
1. Check Redis latency: `redis-cli --latency -h $REDIS_HOST`
2. Check cache hit ratio: `agentauth_cache_hit_ratio{cache="token"}`
3. If cache hit ratio is low, check if Redis has memory pressure
4. Check verifier pod resource usage: `kubectl top pods -l app=verifier -n agentauth`

### How to verify recovery
- p99 latency drops below 5ms
- Cache hit ratio returns to >95%

### Root cause investigation steps
1. Check Redis cluster for hot keys or memory pressure
2. Check if there's a cache stampede (many requests for same cold key)
3. Review database query performance
4. Check network latency between verifier and Redis

### Known false-positive conditions
- Initial cold start after deployment
- After Redis failover

---

## circuit-breaker

### What this alert means
A circuit breaker has been open for more than 2 minutes, indicating a dependency is failing.

### Immediate mitigation steps (first 5 minutes)
1. Identify which circuit breaker: Check `agentauth_circuit_breaker_state` labels
2. For Redis: Check cluster health with `redis-cli cluster info`
3. For PostgreSQL: Check connection with `pg_isready -h $PG_HOST`
4. For KMS: Check cloud provider status page

### How to verify recovery
- Circuit breaker state changes to 0 (closed) or 2 (half-open attempting recovery)
- Dependency connectivity restored

### Root cause investigation steps
1. Check dependency service logs and metrics
2. Check network connectivity and DNS resolution
3. Review recent infrastructure changes
4. Check for resource exhaustion on dependency services

### Known false-positive conditions
- Planned maintenance on dependencies
- Brief network partitions that self-heal

---

## revocation-lag

### What this alert means
Token revocations are taking more than 200ms to propagate to the cache. This could allow revoked tokens to be used during the lag window.

### Immediate mitigation steps (first 5 minutes)
1. Check Redis write latency
2. Check registry to verifier network latency
3. Verify revocation events are being published
4. Check for Redis replication lag in cluster mode

### How to verify recovery
- `agentauth_revocation_propagation_seconds` p99 drops below 200ms
- Revocation test completes within expected time

### Root cause investigation steps
1. Check Redis cluster for write performance issues
2. Review revocation event publishing code path
3. Check for network issues between services
4. Verify Redis cluster replication is healthy

### Known false-positive conditions
- During Redis cluster failover

---

## audit-lag

### What this alert means
Audit events are taking more than 30 seconds to be written. This may indicate database issues or backpressure.

### Immediate mitigation steps (first 5 minutes)
1. Check PostgreSQL connections: `SELECT count(*) FROM pg_stat_activity`
2. Check for long-running transactions: `SELECT * FROM pg_stat_activity WHERE state = 'active'`
3. Check audit buffer usage: `agentauth_audit_buffer_pct`
4. Check for disk I/O issues on database

### How to verify recovery
- `agentauth_audit_write_lag_seconds` drops below 30s
- Audit buffer usage decreasing

### Root cause investigation steps
1. Check database for lock contention
2. Review recent schema or index changes
3. Check for partition issues (is next month's partition created?)
4. Analyze slow query logs

### Known false-positive conditions
- During large batch operations
- During partition rotation

---

## audit-buffer

### What this alert means
The in-memory audit buffer is above 70% capacity. If it fills completely, primary operations will start failing.

### Immediate mitigation steps (first 5 minutes)
1. **This is critical** - audit writes must succeed or operations will fail
2. Check PostgreSQL connectivity immediately
3. Check for database transaction locks
4. Consider scaling registry replicas down temporarily to reduce write volume
5. Check disk space on database server

### How to verify recovery
- `agentauth_audit_buffer_pct` drops below 50%
- Audit write lag returning to normal

### Root cause investigation steps
1. Check database for the root cause of slow writes
2. Review audit table partitioning
3. Check for disk I/O saturation
4. Verify database autovacuum is working

### Known false-positive conditions
- None - this alert should always be investigated

---

## nonce-store

### What this alert means
The nonce store Redis memory is above 70%. If it reaches capacity with `noeviction` policy, new requests will be rejected rather than risk replay attacks.

### Immediate mitigation steps (first 5 minutes)
1. Check Redis memory usage: `redis-cli info memory`
2. Check nonce TTLs are working: Keys should expire with token lifetime
3. Consider scaling Redis cluster if persistent
4. Check for abnormal traffic patterns

### How to verify recovery
- `agentauth_nonce_store_memory_pct` drops below 60%
- Memory growth rate returns to normal

### Root cause investigation steps
1. Check for abnormal request volume
2. Verify nonce TTLs are being set correctly
3. Check for memory leaks in Redis configuration
4. Review token lifetime settings

### Known false-positive conditions
- After major traffic spikes (should self-heal as nonces expire)

---

## replica-lag

### What this alert means
PostgreSQL read replica is more than 5 seconds behind the primary. Read queries may return stale data.

### Immediate mitigation steps (first 5 minutes)
1. Check replication status: `SELECT * FROM pg_stat_replication`
2. Check replica disk I/O and CPU
3. Check network between primary and replica
4. Consider failing over to a healthy replica if multiple are available

### How to verify recovery
- Replica lag drops below 5 seconds
- `pg_stat_replication` shows active streaming

### Root cause investigation steps
1. Check for large transactions on primary
2. Review replica resource utilization
3. Check network bandwidth between primary and replica
4. Review WAL generation rate on primary

### Known false-positive conditions
- During large bulk operations
- During initial replica sync

---

## slo-budget

### What this alert means
Error budget is being consumed at 5x the normal rate. At this rate, the monthly error budget will be exhausted prematurely.

### Immediate mitigation steps (first 5 minutes)
1. Identify the source of errors from recent alerts
2. Check for recent deployments that may have introduced issues
3. Consider rolling back recent changes
4. Freeze non-critical deployments

### How to verify recovery
- Error rate returns to baseline
- Error budget burn rate drops below 2x normal

### Root cause investigation steps
1. Correlate with other alerts and deployment events
2. Review error logs for patterns
3. Check dependency health
4. Review recent code changes

### Known false-positive conditions
- Intentional chaos engineering exercises
- Load testing

---

## oom

### What this alert means
A pod was killed due to exceeding its memory limit.

### Immediate mitigation steps (first 5 minutes)
1. Pod should auto-restart - verify it's running
2. Check if it's a recurring issue: `kubectl get events -n agentauth --field-selector reason=OOMKilled`
3. Check current memory usage of surviving pods
4. Consider increasing memory limits if consistently hitting limits

### How to verify recovery
- Pod is running and healthy
- Memory usage is stable

### Root cause investigation steps
1. Check for memory leaks using profiling tools
2. Review recent code changes that may affect memory usage
3. Analyze heap dumps if available
4. Check for unbounded caches or buffers

### Known false-positive conditions
- None - OOM kills should always be investigated

---

## cache-hit-ratio

### What this alert means
Token cache hit ratio is below 90%, meaning more than 10% of verifications are hitting the database.

### Immediate mitigation steps (first 5 minutes)
1. Check Redis connectivity and health
2. Check cache eviction rate: `redis-cli info stats | grep evicted`
3. Check if there's a spike in unique tokens being verified
4. Verify cache population is working correctly

### How to verify recovery
- Cache hit ratio returns above 90%
- Database query rate decreases

### Root cause investigation steps
1. Check Redis memory pressure and eviction policy
2. Review token access patterns
3. Check for cache invalidation bugs
4. Verify cache warming on startup

### Known false-positive conditions
- After verifier pod restart (cache needs to warm)
- After Redis restart

---

## redis-unavailable

### What this alert means
The verifier is unable to connect to Redis. It should fall back to PostgreSQL but with degraded latency.

### Immediate mitigation steps (first 5 minutes)
1. Check Redis cluster health: `redis-cli cluster info`
2. Check network connectivity to Redis
3. Verify verifier is falling back to PostgreSQL correctly
4. Check Redis for OOM or connection limit issues

### How to verify recovery
- Redis connection errors stop
- Latency returns to normal (sub-5ms)

### Root cause investigation steps
1. Check Redis logs for errors
2. Review network configuration and firewall rules
3. Check for Redis cluster failover events
4. Verify Redis resource limits

### Known false-positive conditions
- During planned Redis maintenance

---

## archival-issues

### Archival Job Failed

#### What this means
The audit archival job failed to complete successfully.

#### Immediate steps
1. Check archiver logs: `kubectl logs -l app=audit-archiver -n agentauth`
2. Verify database connectivity
3. Check cold storage (S3/GCS) access

#### Recovery verification
- Next scheduled job completes successfully
- `agentauth_archival_job_status` returns to 1

### Partition Creation Failed

#### What this means
Failed to create next month's audit partition. If not fixed, audit writes will fail when the current partition ends.

#### Immediate steps
1. **This is critical** - manually create the partition if needed
2. Check database connectivity
3. Check for disk space issues
4. Verify database user permissions

#### Recovery verification
- Partition exists for next month
- `agentauth_partition_creation_status` returns to 1

### Cold Storage Upload Failed

#### What this means
Archived audit data failed to upload to cold storage (S3/GCS).

#### Immediate steps
1. Check cloud provider credentials
2. Verify bucket exists and is accessible
3. Check for network issues to cloud storage

#### Recovery verification
- Upload succeeds on retry
- `agentauth_cold_storage_upload_status` returns to 1

---

## General Troubleshooting

### Useful Commands

```bash
# Check all pod status
kubectl get pods -n agentauth

# Check recent events
kubectl get events -n agentauth --sort-by='.lastTimestamp'

# Check service logs
kubectl logs -l app=registry -n agentauth --tail=100
kubectl logs -l app=verifier -n agentauth --tail=100

# Check metrics endpoint
kubectl port-forward svc/registry-metrics 9090:9090 -n agentauth
curl localhost:9090/metrics

# Check Redis
redis-cli -h $REDIS_HOST cluster info
redis-cli -h $REDIS_HOST info memory

# Check PostgreSQL
psql -h $PG_HOST -U agentauth -c "SELECT count(*) FROM pg_stat_activity"
```

### Escalation Path

1. **P1 (Critical)**: Page on-call immediately
   - Token verification down
   - Audit buffer full
   - Nonce store full

2. **P2 (High)**: Page during business hours
   - High latency
   - Circuit breakers open
   - SLO budget burning fast

3. **P3 (Medium)**: Ticket for next business day
   - Replica lag
   - Cache hit ratio low
   - Archival issues
