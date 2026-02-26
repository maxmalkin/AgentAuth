# AgentAuth Capacity Planning Guide

This document provides guidance for sizing AgentAuth deployments and planning for growth.

## Overview

AgentAuth consists of three main services with different scaling characteristics:

| Service | Scaling Model | Primary Constraint |
|---------|---------------|-------------------|
| Registry | Vertical + Horizontal | CPU (crypto operations) |
| Verifier | Horizontal | Network I/O, Memory |
| Audit Archiver | Single instance | Database I/O |

## Current Baseline Metrics

These metrics should be updated after each major release or significant traffic change.

| Metric | Current Value | 12-Month Projection |
|--------|---------------|---------------------|
| Token verifications/second | - | - |
| Token issuances/second | - | - |
| Audit events/day | - | - |
| Active agents | - | - |
| Active service providers | - | - |

## Resource Sizing Guidelines

### Registry Service

**Scaling triggers:**
- CPU utilization > 60% sustained
- Token issuance p99 > 50ms

**Initial sizing:**
```yaml
replicas: 3
resources:
  requests:
    cpu: 500m
    memory: 256Mi
  limits:
    cpu: 2000m
    memory: 1Gi
```

**Scaling formula:**
- 1 registry replica per 500 token issuances/second
- Add 1 replica for each 1000 concurrent grant approval sessions

**Memory considerations:**
- Base: ~100MB
- Per connection pool: ~10MB per pool
- Audit buffer: Up to 100MB when backpressured

### Verifier Service

**Scaling triggers:**
- p99 latency > 5ms
- Request rate > 1000 req/s per replica

**Initial sizing:**
```yaml
replicas: 5
resources:
  requests:
    cpu: 250m
    memory: 128Mi
  limits:
    cpu: 1000m
    memory: 512Mi
```

**Scaling formula:**
- 1 verifier replica per 1000 token verifications/second sustained
- Account for burst: 2x replicas for 2x peak-to-average ratio

**Memory considerations:**
- Base: ~50MB
- Connection pools: ~20MB total
- In-flight requests: ~1KB per request

### Audit Archiver

**Sizing:**
```yaml
replicas: 1  # Leader election, only one active
resources:
  requests:
    cpu: 100m
    memory: 128Mi
  limits:
    cpu: 500m
    memory: 256Mi
```

**Constraints:**
- Single instance with leader election
- Must complete archival within maintenance window
- Partition creation must run before month end

## Database Sizing

### PostgreSQL Primary

**Initial sizing:**
- vCPU: 8
- RAM: 32GB
- Storage: 500GB SSD (provisioned IOPS)

**Scaling triggers:**
- Write IOPS > 70% provisioned
- Connection count > 80% max_connections
- Storage > 70% capacity

**Growth formula:**
- Audit events: ~1KB per event average
- 1M events/day = ~1GB/day = ~30GB/month (before compression)
- After 90-day retention: ~90GB hot storage

### PostgreSQL Read Replicas

**Initial:** 2 replicas for high availability

**Scaling triggers:**
- Replica lag > 5 seconds sustained
- Read query latency increasing

**Sizing:** Match primary for CPU/RAM, can use smaller storage

### Redis Cluster

**Initial sizing:**
- 3 primary nodes + 3 replicas
- 8GB RAM per node
- Total: 24GB usable (after replication)

**Memory allocation:**
| Store | Allocation | Eviction Policy |
|-------|------------|-----------------|
| Token cache | 40% | allkeys-lru |
| Nonce store | 40% | noeviction |
| Rate limiters | 20% | volatile-ttl |

**Sizing formula:**
- Token cache: ~500 bytes per cached token
- 1M active tokens = ~500MB
- Nonce store: ~100 bytes per nonce
- 1M nonces = ~100MB

**Critical threshold:** Nonce store at 70% triggers alert

## Network Considerations

### Bandwidth Requirements

| Path | Estimate |
|------|----------|
| Verifier → Redis | 1KB per verification |
| Verifier → PostgreSQL (fallback) | 2KB per verification |
| Registry → PostgreSQL | 5KB per token issuance |
| Registry → KMS | 1KB per signing operation |

**Per 10,000 verifications/second:**
- Redis: ~10MB/s
- With 5% fallback to PostgreSQL: ~1MB/s additional

### Latency Requirements

| Path | Target | Maximum |
|------|--------|---------|
| Verifier → Redis | <1ms | 5ms |
| Verifier → PostgreSQL | <5ms | 20ms |
| Registry → PostgreSQL | <10ms | 50ms |
| Registry → KMS | <50ms | 200ms |

## Scaling Scenarios

### Scenario 1: 10x Traffic Increase

**Current:** 1,000 verifications/second
**Target:** 10,000 verifications/second

**Changes needed:**
- Verifier: 5 → 15 replicas
- Redis: Add 3 more primary nodes
- Registry: 3 → 5 replicas
- PostgreSQL: Add 2 more read replicas

### Scenario 2: New Region Deployment

**For each new region:**
- Full verifier deployment (can operate read-only)
- Redis cluster (for local caching)
- PostgreSQL read replica (for fallback)
- Registry not required (can call primary region)

### Scenario 3: High Burst Events

**For 10x burst capacity:**
- HPA maxReplicas = 3x normal
- Redis: Ensure headroom in memory
- Rate limiting at edge to shed excess load

## Cost Optimization

### Right-sizing Recommendations

1. **Off-peak scaling:** Scale verifiers down to 50% during low-traffic hours
2. **Spot instances:** Verifiers are stateless, suitable for spot/preemptible
3. **Reserved capacity:** Registry and database benefit from reserved pricing

### Resource Efficiency Targets

| Metric | Target |
|--------|--------|
| CPU utilization (avg) | 40-60% |
| Memory utilization (avg) | 50-70% |
| Cache hit ratio | >95% |
| Database connection utilization | 50-70% |

## Capacity Planning Checklist

### Monthly Review
- [ ] Update baseline metrics table
- [ ] Review resource utilization trends
- [ ] Check database storage growth
- [ ] Verify audit partition creation
- [ ] Review Redis memory usage

### Quarterly Review
- [ ] Update 12-month projections
- [ ] Review and adjust HPA settings
- [ ] Load test at projected capacity
- [ ] Review cost vs. capacity tradeoffs

### Pre-Launch Checklist
- [ ] Load test at 2x expected peak
- [ ] Verify auto-scaling works correctly
- [ ] Confirm database can handle projected writes
- [ ] Verify Redis cluster can handle projected cache size
- [ ] Test failover scenarios

## Monitoring Dashboard Queries

### Key Capacity Metrics (Prometheus)

```promql
# Verifications per second
sum(rate(agentauth_tokens_verified_total[5m]))

# Token issuances per second
sum(rate(agentauth_tokens_issued_total[5m]))

# Active tokens (approximate)
sum(agentauth_active_tokens)

# Redis memory usage percentage
redis_memory_used_bytes / redis_memory_max_bytes * 100

# Database connections in use
pg_stat_activity_count / pg_settings_max_connections * 100

# Audit events per day (24h)
sum(increase(agentauth_audit_events_total[24h]))
```

## Emergency Procedures

### If approaching Redis memory limit
1. Enable aggressive LRU eviction on token cache
2. Reduce token cache TTL
3. Scale Redis cluster (add nodes)

### If approaching database storage limit
1. Run emergency archival job
2. Drop oldest partitions after archiving
3. Add storage capacity

### If approaching connection limits
1. Review and close idle connections
2. Reduce connection pool sizes temporarily
3. Add read replicas to distribute load
