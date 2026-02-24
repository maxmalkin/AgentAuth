# AgentAuth — Claude Code Instructions

---

## Table of Contents

0. [Implementation Instructions for Claude Code](#0-implementation-instructions-for-claude-code)
1. [Environment Setup](#1-environment-setup)
2. [Repository Structure](#2-repository-structure)
3. [Build Rules](#3-build-rules)
4. [Test Rules](#4-test-rules)
5. [Git Rules](#5-git-rules)
6. [Phase Execution Order](#6-phase-execution-order)
7. [Phase Gate Rule](#7-phase-gate-rule)
8. [Security Rules (Non-Negotiable)](#8-security-rules-non-negotiable)
9. [Scalability Rules](#9-scalability-rules)
10. [Stability Rules](#10-stability-rules)
11. [Observability Rules](#11-observability-rules)
12. [CI Pipeline Requirements](#12-ci-pipeline-requirements)
13. [Phase Specifications](#13-phase-specifications)

---

## 0. Implementation Instructions for Claude Code

This section governs how Claude Code must operate throughout the entire project. Read this section completely before taking any action. These instructions override any other instinct about how to proceed.

---

### 0.1 Claude's Role

You are the **Lead Architect and Implementation Engineer** for AgentAuth. You are responsible for making technical decisions, writing production-quality code, running tests, interpreting results, and knowing when to stop and ask rather than guess. You are not a code generator — you are an engineer who happens to write code. That means you reason before you act, you verify your work, and you own the outcome of every decision you make.

**Your specific responsibilities:**
- Read this entire file before writing a single line of code
- Plan each stage before executing it — write your plan as a brief summary in a `WORKLOG.md` file at the repo root, updated after every task
- Execute one phase at a time, strictly in the order defined in Section 6
- Run every gate check defined in Section 7 before declaring a phase complete
- Write clean, idiomatic, well-documented Rust (and TypeScript/Python where specified)
- Never take shortcuts that violate Section 8 (Security), Section 9 (Scalability), or Section 10 (Stability)
- **Stop and request human approval after every phase is complete** — do not begin the next phase until you receive explicit written approval

**What you must never do:**
- Begin a new phase without approval
- Skip a test because it is difficult to write
- Suppress a compiler warning with `#[allow(...)]` without a comment explaining why
- Make an architectural decision that contradicts this document without flagging it for review first
- Proceed past a failing gate check

---

### 0.2 Agentic Sub-Roles

For phases that involve parallel workstreams (Phase 3, Phase 7, Phase 9), Claude Code may spin up **sub-agents** to work on independent tasks concurrently. When doing so, each sub-agent must be given a focused role and a tightly scoped prompt. Sub-agents must not make decisions outside their scope — they escalate to the Lead (you) when they encounter ambiguity.

The following sub-agent roles are defined for this project. Only instantiate a sub-agent when the phase explicitly calls for it.

---

#### Sub-Agent: `core-impl`

**When to use:** Phase 1 — implementing `agentauth-core` types and crypto.

**Role prompt to use when spawning:**
```
You are the Core Library Engineer for AgentAuth.
Your sole responsibility is implementing the agentauth-core crate located at crates/agentauth-core/.
This crate must have zero I/O, zero network calls, and zero database access.

Your tasks for this session:
1. Implement all types defined in Phase 1 of CLAUDE.md (AgentManifest, Capability,
   BehavioralEnvelope, AgentAccessToken, ApprovalAssertion)
2. Implement the SigningBackend trait and KmsSigningBackend + InMemorySigningBackend
3. Implement all crypto operations listed in Phase 1
4. Implement the AgentKeyBackend enum with all variants
5. Write all unit tests required by the Test Rules section for agentauth-core
6. Run: cargo nextest run -p agentauth-core
7. Run: cargo clippy -p agentauth-core -- -D warnings
8. Report back: test results, any design decisions you made, and anything that
   required interpretation of the spec. Do NOT proceed to any other crate.
```

---

#### Sub-Agent: `db-impl`

**When to use:** Phase 2 — writing database migrations.

**Role prompt to use when spawning:**
```
You are the Database Engineer for AgentAuth.
Your sole responsibility is creating the SQLx migration files in migrations/.

Your tasks for this session:
1. Write forward and rollback migrations for all tables defined in Phase 2 of CLAUDE.md
2. Ensure audit_events is range-partitioned by created_at (monthly) from the first migration
3. Ensure all indexes use CREATE INDEX CONCURRENTLY
4. Ensure the agentauth_service role has REVOKE UPDATE, DELETE on audit_events
5. Ensure all foreign keys have ON DELETE behavior explicitly specified
6. Run: docker-compose up -d (postgres must be running)
7. Run: sqlx migrate run
8. Run: sqlx migrate revert --all
9. Run: sqlx migrate run (verify clean re-apply)
10. Report back: migration output, any ambiguities you encountered, schema decisions made.
    Do NOT write any application code.
```

---

#### Sub-Agent: `registry-impl`

**When to use:** Phase 3 — implementing the registry service.

**Role prompt to use when spawning:**
```
You are the Registry Service Engineer for AgentAuth.
Your responsibility is implementing services/registry and crates/agentauth-registry.

Your tasks for this session:
1. Implement all routes defined in Phase 3 of CLAUDE.md for the registry binary
2. Implement token issuance with idempotency (same grant + same 15-min window = same JTI)
3. Implement the approval flood protection rules (max 5 pending, 1h expiry, cooldown backoff)
4. Wire up all circuit breakers defined in Section 9.7 for PostgreSQL and KMS dependencies
5. Implement health check endpoints: /health/live, /health/ready, /health/startup
6. Instrument all routes with tower-http tracing middleware (OpenTelemetry)
7. Wire up graceful shutdown per Section 10.3
8. Run: cargo nextest run -p agentauth-registry
9. Report back: what you built, any decisions you made, open questions.
   Do NOT touch the verifier binary or the approval UI.
```

---

#### Sub-Agent: `verifier-impl`

**When to use:** Phase 3 (concurrent with `registry-impl`) — implementing the verifier service.

**Role prompt to use when spawning:**
```
You are the Verifier Service Engineer for AgentAuth.
Your responsibility is implementing services/verifier — the lightweight, read-only,
horizontally-scalable token verification binary.

Your tasks for this session:
1. Implement POST /v1/tokens/verify with the strict check ordering defined in Phase 3
   (nonce → revocation → cnf binding → DPoP proof → signature → expiry)
2. Redis-first verification: cache miss falls back to PostgreSQL read replica only
3. Implement cache stampede prevention for tokens near expiry (Section 9.5)
4. Implement the readiness probe: only ready when Redis is reachable AND cache is warm
5. Implement health check endpoints: /health/live, /health/ready, /health/startup
6. Implement Prometheus metrics endpoint on a separate port (Section 11.3)
7. Wire up graceful shutdown per Section 10.3
8. Run: cargo nextest run -p agentauth-verifier
9. Verify: p99 latency target sub-5ms with Redis warm (use a local benchmark)
10. Report back: benchmark results, design decisions, open questions.
    Do NOT modify the registry binary or any shared library beyond what's needed.
```

---

#### Sub-Agent: `sdk-impl`

**When to use:** Phase 5 — implementing the Rust agent SDK.

**Role prompt to use when spawning:**
```
You are the SDK Engineer for AgentAuth.
Your responsibility is implementing crates/agentauth-sdk — the library that agent
authors use to authenticate with AgentAuth-enabled services.

Your tasks for this session:
1. Implement AgentAuthClient with all methods defined in Phase 5 of CLAUDE.md
2. Implement BehavioralRateLimiter (client-side sliding window, mandatory)
3. Implement token caching with refresh-when-within-60s logic
4. Implement DPoP proof generation and attachment on every authenticated request
5. Implement retry logic per Section 10.4 (transient vs non-transient, Retry-After header)
6. Implement connection pool reuse (one reqwest::Client per registry URL)
7. Write all unit tests required by the Test Rules section for agentauth-sdk
8. Run: cargo nextest run -p agentauth-sdk
9. Run: cargo clippy -p agentauth-sdk -- -D warnings
10. Report back: test results, design decisions, open questions.
    Do NOT touch the Python bindings or any other crate.
```

---

#### Sub-Agent: `security-audit`

**When to use:** Phase 7 — compliance hardening and threat model.

**Role prompt to use when spawning:**
```
You are the Security Auditor for AgentAuth.
Your responsibility is Phase 7: compliance hardening and threat model documentation.
You do NOT write new features — you audit what has been built and document threats.

Your tasks for this session:
1. Run the full compliance test suite: cargo nextest run --test compliance
   Fix any test failures before proceeding.
2. Manually audit the following against the security rules in Section 8 of CLAUDE.md:
   - crates/agentauth-core/src/crypto/ (timing-safe comparisons, no unwrap)
   - services/registry (TLS required, CSRF protection, audit atomicity)
   - services/verifier (check ordering is correct, no write endpoints)
   - crates/agentauth-sdk (no extra outbound URLs, retry policy correct)
3. Run all banned pattern grep checks from Section 8.10 and report results
4. Run: cargo audit
5. Write docs/threat-model.md covering all 13 vectors listed in Phase 7 of CLAUDE.md.
   For each vector: describe the attack, describe the mitigation already implemented,
   describe the residual risk, describe how it would be detected.
6. Report back: audit findings (issues found + fixes made), threat model summary,
   any residual risks that require human decision.
   Do NOT implement new features. Flag issues and fix them; do not paper over them.
```

---

#### Sub-Agent: `observability-impl`

**When to use:** Phase 9 — wiring up metrics, traces, alerts, and runbook.

**Role prompt to use when spawning:**
```
You are the Observability Engineer for AgentAuth.
Your responsibility is Phase 9: wiring up all metrics, distributed tracing,
alerts, and operational documentation.

Your tasks for this session:
1. Verify all Prometheus metrics defined in Section 11.3 are emitted by all services.
   Add any missing metrics.
2. Verify all OpenTelemetry spans defined in Section 11.2 are present with the
   required custom attributes. Add any missing spans.
3. Write all PrometheusRule alert definitions in deploy/helm/*/alerts.yaml,
   covering every alert in the table in Section 11.4.
4. Write docs/runbook.md with an entry for every alert. Each entry must cover
   the 5 points defined in Section 11.5.
5. Write docs/capacity-planning.md with initial sizing estimates and 12-month
   projections for the metrics listed in Section 9.12.
6. Write Grafana dashboard JSON definitions in deploy/grafana/ for:
   token verification SLO, circuit breaker states, cache hit ratios,
   audit log lag, and per-service request rates.
7. Define all 5 chaos experiment files in chaos/ with hypothesis and expected results.
8. Configure .github/workflows/nightly.yml with the nightly pipeline steps.
9. Report back: what was wired up, what was missing, any gaps that remain.
   Do NOT make functional changes to service code.
```

---

### 0.3 Stage-by-Stage Execution Plan

This is the master execution plan Claude Code follows. Each stage maps to one or more phases. **After completing every stage, stop, summarize what was done, and explicitly ask the human for approval before proceeding.**

---

#### Stage 1 — Foundation
**Covers:** Phase 1 (core library) + Phase 2 (database schema)
**Sub-agents:** Spawn `core-impl` and `db-impl` concurrently
**Lead tasks:**
- Initialize the Cargo workspace and all crate skeletons (`cargo new --lib` for each crate)
- Initialize the Node.js project for `services/approval-ui`
- Create `docker-compose.yml` with postgres (primary + replica), redis (3-node cluster), otel-collector, prometheus, grafana
- Create `.env.example` with all required variable names and documentation
- Create `deny.toml` with license and ban configuration
- Create `WORKLOG.md` and log stage start
- Coordinate `core-impl` and `db-impl` sub-agents; review and merge their outputs
- Run the Phase 1 gate: `cargo nextest run -p agentauth-core && cargo audit`
- Run the Phase 2 gate: `sqlx migrate run && sqlx migrate revert --all && sqlx migrate run`

**Approval checkpoint message:**
```
## Stage 1 Complete — Awaiting Approval

### What was built:
- agentauth-core crate: [list key types and crypto ops implemented]
- Database migrations: [list tables created]
- Workspace skeleton: [list all crates and services initialized]
- docker-compose: [list services included]

### Gate results:
- cargo nextest run -p agentauth-core: [PASS/FAIL + summary]
- cargo audit: [PASS/FAIL]
- sqlx migrate run + revert + re-run: [PASS/FAIL]
- cargo clippy -p agentauth-core: [PASS/FAIL]

### Design decisions made:
[List any decisions that required interpretation of the spec]

### Open questions for human review:
[List anything ambiguous or that requires a decision]

**Ready to proceed to Stage 2 (registry + verifier services)?**
Please reply with APPROVE to continue, or provide feedback.
```

---

#### Stage 2 — Core Services
**Covers:** Phase 3 (registry + verifier)
**Sub-agents:** Spawn `registry-impl` and `verifier-impl` concurrently
**Lead tasks:**
- Review and merge outputs from both sub-agents
- Verify the registry and verifier compile together without conflicts in the shared `agentauth-registry` crate
- Run full integration test suite: `cargo nextest run --test integration`
- Run baseline load test: `k6 run --vus 50 --duration 60s load-tests/token-verify.js`
- Verify all health check endpoints return correct status codes
- Verify Prometheus metrics endpoints are on a separate port
- Verify graceful shutdown works: send SIGTERM during an active load test and confirm zero errors
- Update `WORKLOG.md`

**Approval checkpoint message:**
```
## Stage 2 Complete — Awaiting Approval

### What was built:
- services/registry: [list routes implemented]
- services/verifier: [list routes implemented]
- Circuit breakers: [list dependencies wrapped]
- Health checks: [live/ready/startup implemented for each service]

### Gate results:
- cargo nextest run --test integration: [PASS/FAIL + test count]
- k6 load test (token-verify, 50 VUs, 60s): [p50/p99/p999, error rate]
- Load test vs baseline targets: [PASS/FAIL per endpoint]
- Graceful shutdown under load: [PASS/FAIL]
- cargo clippy --workspace: [PASS/FAIL]
- cargo audit: [PASS/FAIL]

### Design decisions made:
[List decisions, especially around circuit breaker thresholds or caching strategy]

### Open questions for human review:
[List anything that requires a decision]

**Ready to proceed to Stage 3 (approval UI)?**
Please reply with APPROVE to continue, or provide feedback.
```

---

#### Stage 3 — Approval UI
**Covers:** Phase 4 (approval UI)
**Sub-agents:** None — UI work is sequential
**Lead tasks:**
- Scaffold the React + TypeScript + Vite project in `services/approval-ui/`
- Implement all three routes: `/approve/:grant_id`, `/agents`, `/agents/:agent_id/activity`
- Implement capability-to-human-readable translation for all capability types
- Implement two-step confirmation for `Transact` and `Delete` capabilities
- Integrate WebAuthn/Passkey for approval assertion signing
- Implement CSRF protection: SameSite=Strict cookie + Double Submit pattern
- Implement graceful error state when registry is unreachable
- Run Playwright test suite: `playwright test`
- Update `WORKLOG.md`

**Approval checkpoint message:**
```
## Stage 3 Complete — Awaiting Approval

### What was built:
- Approval UI routes: [list routes]
- Capability translations: [list all capability types covered]
- Two-step confirmation: [confirm Transact and Delete require it]
- WebAuthn integration: [confirm passkey signing implemented]
- CSRF protection: [confirm SameSite + double submit implemented]
- Error states: [confirm graceful degradation when registry unreachable]

### Gate results:
- playwright test: [PASS/FAIL + test count]
- npm run build: [PASS/FAIL]
- All Playwright security tests (CSRF, two-step bypass attempt): [PASS/FAIL]

### Design decisions made:
[List any UX or implementation decisions]

### Open questions for human review:
[List anything requiring a decision]

**Ready to proceed to Stage 4 (Rust SDK)?**
Please reply with APPROVE to continue, or provide feedback.
```

---

#### Stage 4 — Agent SDKs
**Covers:** Phase 5 (Rust SDK) + Phase 6 (Python bindings)
**Sub-agents:** Spawn `sdk-impl` for the Rust SDK. Lead handles Python bindings.
**Lead tasks:**
- Review and integrate `sdk-impl` output
- Run Rust SDK tests: `cargo nextest run -p agentauth-sdk`
- Set up PyO3 + maturin project structure for `agentauth-py/`
- Implement Python bindings wrapping the Rust SDK
- Implement `agentauth.integrations.langchain.AgentAuthToolkit`
- Implement `agentauth.integrations.autogen.AgentAuthMiddleware`
- Run Python tests: `pytest agentauth-py/tests/`
- Run maturin build: `maturin build`
- Update `WORKLOG.md`

**Approval checkpoint message:**
```
## Stage 4 Complete — Awaiting Approval

### What was built:
- agentauth-sdk (Rust): [confirm AgentAuthClient, BehavioralRateLimiter, DPoP, retry, caching]
- agentauth-py: [confirm Python bindings, LangChain toolkit, AutoGen middleware]

### Gate results:
- cargo nextest run -p agentauth-sdk: [PASS/FAIL + test count]
- pytest agentauth-py/tests/: [PASS/FAIL + test count]
- maturin build: [PASS/FAIL]
- Token caching test (no second network request): [PASS/FAIL]
- Retry policy test (transient vs non-transient): [PASS/FAIL]
- BehavioralRateLimiter throttling test: [PASS/FAIL]

### Design decisions made:
[List SDK design decisions, especially retry backoff parameters]

### Open questions for human review:
[List anything requiring a decision]

**Ready to proceed to Stage 5 (security audit + compliance)?**
Please reply with APPROVE to continue, or provide feedback.
```

---

#### Stage 5 — Security Audit & Compliance
**Covers:** Phase 7 (threat model + compliance hardening)
**Sub-agents:** Spawn `security-audit`
**Lead tasks:**
- Review all findings reported by `security-audit`
- Apply any fixes the sub-agent identified
- Re-run compliance tests after fixes: `cargo nextest run --test compliance`
- Run all banned pattern grep checks from Section 8.10 — must return zero results
- Run `cargo audit` — must return zero vulnerabilities
- Run `cargo deny check licenses && cargo deny check bans`
- Review `docs/threat-model.md` for completeness against all 13 required vectors
- Update `WORKLOG.md`

**Approval checkpoint message:**
```
## Stage 5 Complete — Awaiting Approval

### Security audit findings:
[List each issue found, its severity, and how it was fixed]

### Threat model coverage:
[Confirm all 13 vectors are documented with mitigations and residual risks]

### Gate results:
- cargo nextest run --test compliance: [PASS/FAIL + test count]
- cargo audit: [PASS/FAIL — zero vulnerabilities]
- cargo deny check: [PASS/FAIL]
- Banned pattern grep checks (8 patterns): [PASS/FAIL — must be zero matches]
- All Section 8 rules verified: [PASS/FAIL per subsection]

### Residual risks requiring human decision:
[List any security risks that require a deliberate acceptance decision from the human,
e.g., a known acceptable tradeoff or a mitigation that requires infrastructure not
yet available]

**Ready to proceed to Stage 6 (discovery document + observability)?**
Please reply with APPROVE to continue, or provide feedback.
```

---

#### Stage 6 — Discovery, Observability & Hardening
**Covers:** Phase 8 (discovery document) + Phase 9 (observability + runbook)
**Sub-agents:** Spawn `observability-impl` for Phase 9. Lead handles Phase 8.
**Lead tasks:**
- Implement `GET /.well-known/agentauth` with the schema defined in Phase 8
- Implement `GET /.well-known/agentauth/keys` key rotation support
- Create `agentauth-schema` crate with JSON Schema for discovery document validation
- Write integration test that validates the live discovery document against the schema
- Review and integrate `observability-impl` output
- Verify all Prometheus metrics are present by running the full stack and querying `/metrics`
- Verify all PrometheusRule alerts are syntactically valid YAML
- Verify Grafana dashboards load without errors
- Verify `docs/runbook.md` has an entry for every alert
- Verify `docs/capacity-planning.md` is populated
- Verify all 5 chaos experiment files exist with hypothesis and expected results
- Verify `.github/workflows/nightly.yml` is configured
- Run the full CI pipeline locally end-to-end (all 18 steps)
- Update `WORKLOG.md`

**Approval checkpoint message:**
```
## Stage 6 Complete — Awaiting Approval

### What was built:
- Discovery document: [confirm schema matches spec]
- Key rotation endpoint: [confirm key versioning works]
- agentauth-schema crate: [confirm JSON Schema validates discovery doc]
- Prometheus metrics: [list any that were missing and added]
- PrometheusRule alerts: [confirm count matches Section 11.4 table]
- Grafana dashboards: [list dashboards created]
- Runbook: [confirm entry count matches alert count]
- Chaos experiments: [list 5 experiments]
- Nightly pipeline: [confirm configured]

### Gate results:
- Discovery document schema validation test: [PASS/FAIL]
- Full CI pipeline (all 18 steps): [PASS/FAIL — note any failures]
- All metrics present in /metrics output: [PASS/FAIL]
- All alert YAML valid: [PASS/FAIL]

### Open questions for human review:
[List anything requiring a decision before the project is considered complete]

**All 9 phases complete. Full implementation ready for human review.**
Please reply with APPROVE to finalize, or provide feedback.
```

---

### 0.4 WORKLOG.md Format

Create `WORKLOG.md` at the repo root on Stage 1. Update it after every significant task — not just at stage boundaries. It is your working memory and the human's audit trail.

```markdown
# AgentAuth — Work Log

## Stage 1 — Foundation
### [DATE TIME] Started Stage 1
- Initialized Cargo workspace
- ...

### [DATE TIME] core-impl sub-agent completed
- Findings: ...
- Decisions made: ...

### [DATE TIME] Stage 1 gate checks
- cargo nextest run -p agentauth-core: PASS (42 tests)
- cargo audit: PASS
- sqlx migrate: PASS
- Awaiting human approval

## Stage 2 — Core Services
...
```

---

### 0.5 Decision Escalation Protocol

When you encounter a situation not covered by this document, or where two valid interpretations of the spec lead to meaningfully different implementations, **do not guess**. Follow this protocol:

1. Stop what you are doing
2. Document the ambiguity in `WORKLOG.md` under a `## Decision Required` heading
3. Write a clear, concise question to the human at the next approval checkpoint
4. In the meantime, implement the most conservative interpretation (the one that is least likely to violate security or correctness invariants)
5. Mark the conservative implementation with a `// TODO: awaiting human decision on [topic]` comment so it is easy to find and revisit

Do not pile up multiple unresolved decisions across stages — surface them promptly at the next checkpoint.

---

### 0.6 What Counts as "Approval"

The human must reply with the word **APPROVE** (case-insensitive) or an equivalent explicit affirmation (e.g., "approved", "looks good, proceed", "yes continue"). The following do not count as approval:

- Silence
- A question in response (answer the question, then re-present the checkpoint)
- Partial feedback ("the DB schema looks fine") without explicit approval to proceed
- Approval for a different stage than the one you just presented

If you do not receive explicit approval, do not proceed. Ask again if needed.

---

### 0.7 Quality Bar

Every line of code you write must meet the same bar you would apply in a code review for a production security-critical system. Specifically:

- **No placeholders.** `todo!()`, `unimplemented!()`, and `// TODO: implement` are not acceptable in gate-checked code. If something is genuinely out of scope for the current phase, document it in `WORKLOG.md` and raise it at the checkpoint.
- **No copy-paste without understanding.** If you adapt code from a reference, you must understand it well enough to explain every line.
- **Error messages are user-facing.** Write error messages that tell the caller what went wrong and what they can do about it. Not "error occurred" or "invalid input".
- **Tests are first-class.** Tests are not afterthoughts — they are part of the deliverable. A function without a test is not complete.
- **Comments explain why, not what.** The code shows what. Comments explain the reason for a non-obvious decision, a constraint, or a known limitation.

---

## 1. Environment Setup

### Required Toolchain

- **Rust**: stable (1.78+) — install via `rustup`
- **Node.js**: 20 LTS — for approval UI (`services/approval-ui`)
- **Python**: 3.11+ — for PyO3 bindings (`agentauth-py`)
- **k6**: load testing — `brew install k6` or see k6.io

### Required Cargo Tools

Install before doing anything else:

```bash
cargo install cargo-nextest sqlx-cli cargo-audit cargo-tarpaulin cargo-deny cargo-flamegraph
```

### Required Services

PostgreSQL 16, Redis 7 (cluster mode in staging/prod), and OpenTelemetry Collector are required for integration tests and local development. Start them with:

```bash
docker-compose up -d
```

The `docker-compose.yml` must include: postgres (primary + 1 replica), redis (3-node cluster), otel-collector, prometheus, and grafana. Local dev without observability infrastructure is not permitted — you cannot catch regressions you cannot see.

### Environment Configuration

Copy `.env.example` to `.env` and fill in all values before running any service or test. Required variables are documented in `.env.example`. **Never run any service without a fully populated `.env`.**

### KMS / HSM Setup

- Production deployments must configure a KMS backend (AWS KMS, GCP Cloud HSM, or HashiCorp Vault Transit). See Phase 1 Crypto for backend options.
- The `allow-plaintext-keys` Cargo feature flag must **never** appear in production `Dockerfiles` or Helm values files.
- For local development only, `EncryptedKeyfile` is permitted with a passphrase loaded from the environment (never hardcoded).

---

## 2. Repository Structure

```
agentauth/
├── crates/
│   ├── agentauth-core/          # Protocol types, crypto, token logic (no I/O)
│   ├── agentauth-registry/      # Registry + IdP service logic (Axum)
│   ├── agentauth-sdk/           # Rust agent SDK
│   └── agentauth-py/            # PyO3 Python bindings
├── services/
│   ├── registry/                # Binary — full CRUD, KMS access, low replica count
│   ├── verifier/                # Binary — read-only token verifier, high replica count
│   ├── audit-archiver/          # Binary — async audit log compaction + cold storage
│   └── approval-ui/             # React + TypeScript approval frontend (Vite)
├── migrations/                  # SQLx migrations (forward + rollback for every migration)
├── load-tests/                  # k6 load test scripts
│   ├── token-verify.js          # Verifier throughput baseline
│   ├── token-issue.js           # Registry issuance under load
│   └── scenarios/               # Composite multi-service scenarios
├── chaos/                       # Chaos engineering experiment definitions
│   ├── redis-partition.yaml
│   ├── db-primary-failure.yaml
│   └── kms-latency.yaml
├── tests/
│   ├── integration/             # End-to-end flow tests (require docker-compose)
│   ├── compliance/              # Behavioral contract and security invariant tests
│   └── stability/               # Long-running soak tests and graceful shutdown tests
├── deploy/
│   ├── helm/                    # Helm charts for registry, verifier, audit-archiver
│   │   ├── registry/
│   │   ├── verifier/
│   │   └── audit-archiver/
│   ├── migrations/              # Kubernetes Job manifests for running migrations
│   └── grafana/                 # Grafana dashboard definitions
├── docs/
│   ├── spec.md                  # Protocol specification
│   ├── threat-model.md          # Threat model (required — see Phase 7)
│   ├── runbook.md               # On-call runbook for every alert
│   ├── capacity-planning.md     # Sizing guidelines and growth projections
│   └── adr/                     # Architecture Decision Records (append-only)
│       ├── 001-uuid-v7.md
│       ├── 002-verifier-separation.md
│       └── 003-audit-hash-chain.md
├── docker-compose.yml
├── deny.toml                    # cargo-deny configuration
├── Cargo.toml                   # Workspace root
└── CLAUDE.md                    # This file
```

**Key architectural separation rules:**

The `registry` and `verifier` binaries are deployed and scaled independently. The `audit-archiver` runs as a low-priority background job. Never merge these binaries. A regression in the audit archiver must not affect token verification latency.

Every non-obvious architectural decision must have a corresponding ADR in `docs/adr/`. ADR format: context → decision → consequences → alternatives considered. ADRs are append-only — never edit a past decision, write a new one that supersedes it.

---

## 3. Build Rules

- **Always** run `cargo check --workspace` before `cargo build`.
- **Always** run `cargo clippy --workspace -- -D warnings` before committing. Zero warnings policy — no exceptions.
- **Never** use `unwrap()` in library code under `crates/`. Permitted only in:
  - Test code (`#[cfg(test)]` blocks or `tests/` directory)
  - Binary `main()` functions, with a comment explaining why it is safe
- Use `anyhow` for application-level errors (binaries, services). Use `thiserror` for library errors (`crates/`).
- All public API items must have rustdoc comments (`///`). CI runs `cargo doc --no-deps` and fails on warnings.
- All SQL must use **SQLx parameterized queries**. Never use string interpolation to build SQL.
- All `async` functions must have a configurable timeout. Defaults: 5 seconds for token operations, 30 seconds for grant approval polling, 2 seconds for Redis operations, 10 seconds for KMS operations. Hard fail — do not hang.
- Every service binary must emit a structured startup log line within 500ms of start, confirming: version, config hash, and which backends are connected. If it does not log this line, the health check will fail and the deployment will not proceed.

---

## 4. Test Rules

### Running Tests

```bash
# All tests
cargo nextest run --workspace

# Single crate
cargo nextest run -p agentauth-core

# Integration tests (requires docker-compose up -d)
cargo nextest run --test integration

# Compliance tests
cargo nextest run --test compliance

# Stability / soak tests (long-running — not run in standard CI)
cargo nextest run --test stability -- --ignored

# Load tests
k6 run load-tests/token-verify.js
```

### Test Requirements

- **New functionality requires tests before the implementation is considered done.** Do not mark a phase complete without passing tests.
- **Unit test coverage target: 80% per crate.** Enforced via `cargo tarpaulin` in CI. CI fails if coverage drops below 80%.
- **Integration tests** live in `tests/integration/` and must use real PostgreSQL and Redis. **Never mock the database in integration tests.** Use test transactions that roll back after each test.
- **Compliance tests** live in `tests/compliance/` and test security invariants. Non-optional; must pass before any release.
- **Stability tests** live in `tests/stability/` and are marked `#[ignore]` so they do not run in standard CI. They run in a dedicated nightly pipeline.
- **Load tests** live in `load-tests/` and must be run before any release touching the hot path.

### Specific Test Requirements by Layer

**`agentauth-core`**
- Roundtrip serialize/deserialize all structs (JSON + compact binary)
- Sign and verify a manifest — valid passes, tampered fails
- Sign and verify an AAT — tampered claims fail
- Tampered `key_id` field causes verification failure
- Capability schema validation — invalid combinations rejected
- `BehavioralEnvelope` validation — nonsensical values (burst > max_rpm) rejected
- Serialization is deterministic: same input always produces identical byte output (required for hash chain integrity)

**`agentauth-registry` / `services/registry`**
- Full happy-path: register agent → request grant → approve → issue token → verify token
- Token verify returns correct denial for: expired token, revoked token, tampered signature, replayed nonce, token bound to different service provider
- Revocation propagates to Redis cache within 100ms
- 50 concurrent token verify requests produce no race conditions
- Audit log is written atomically for allowed, denied, and rate-limited outcomes
- Discovery document matches published JSON Schema
- Token issuance is idempotent: calling issue twice for the same grant in the same 15-minute window returns the same JTI
- All mutating endpoints return `409 Conflict` (not `500`) on duplicate submission of the same idempotency key

**`services/verifier`**
- Token verification is sub-5ms p99 when Redis is warm
- Returns correct result on Redis cache miss (falls back to PostgreSQL)
- Cannot be used to issue or modify tokens — all write endpoints return 404 or 405
- Returns `503` (not `500`) when Redis is unavailable, and falls back to PostgreSQL
- Readiness probe returns `503` until Redis is reachable — verifier does not accept traffic until its cache is warm

**`services/approval-ui`** (Playwright)
- Full approval flow renders and submits correctly
- Denial flow works correctly
- Dangerous capability confirmation step (Transact, Delete) cannot be bypassed with a single click
- CSRF protection: requests without valid double-submit cookie are rejected
- Capability-to-human-readable translation is correct for all capability types
- UI degrades gracefully (shows error state, not blank screen) when registry is unreachable

**`agentauth-sdk`**
- `get_token` returns cached token on second call without a network request
- `get_token` refreshes token when within 60 seconds of expiry
- `authenticate_request` correctly attaches `Authorization: AgentBearer` and `AgentDPoP` headers
- `BehavioralRateLimiter` correctly throttles to envelope constraints
- Using a revoked grant returns a clear error, not a panic
- Connection pool is reused across requests (verify no new connections created per-request)
- SDK retries transient errors (connection reset, 503) with exponential backoff + jitter, up to 3 attempts
- SDK does not retry non-transient errors (401, 403, 400) — fails immediately
- SDK respects `Retry-After` header from the registry on 429 responses

**`agentauth-py`**
- All exposed classes have pytest unit tests
- Async usage works correctly with `asyncio` + `tokio` runtime bridging
- LangChain integration correctly authenticates tool calls

**Compliance test suite** (`tests/compliance/`)
- An agent exceeding its behavioral envelope is rate-limited and logged
- A token with a tampered capability claim is rejected
- Revocation is honored within 100ms (cached path)
- A service provider cannot forge audit events for another service provider
- An agent cannot request capabilities beyond what was declared in its original manifest
- Stolen AAT without DPoP private key is rejected
- Replayed nonce within token lifetime is rejected
- Approval assertion with tampered capability list is rejected

**Stability test suite** (`tests/stability/` — nightly pipeline only)
- Verifier sustains 10,000 token verifications/second for 30 minutes with p99 < 5ms
- No memory growth (leak) over a 1-hour soak test (measure RSS before and after)
- Registry handles 1,000 concurrent grant requests without deadlock or timeout cascade
- System recovers correctly after Redis primary failure (elect new primary, resume within 30 seconds)
- System recovers correctly after PostgreSQL primary failure (resume within 60 seconds)
- Zero in-flight requests are lost during a graceful rolling deployment
- Audit log hash chain remains valid after 1 million events

**Load test baselines** (`load-tests/` — required before each release touching hot path)

These are minimum acceptable numbers. A release degrading any baseline by more than 10% must not proceed without explicit sign-off and an ADR documenting the regression and mitigation.

| Endpoint | Throughput | p50 | p99 | p999 | Max error rate |
|---|---|---|---|---|---|
| `POST /v1/tokens/verify` (Redis warm) | 10,000 req/s | < 1ms | < 5ms | < 15ms | 0.01% |
| `POST /v1/tokens/verify` (cold) | 1,000 req/s | < 5ms | < 20ms | < 50ms | 0.01% |
| `POST /v1/tokens/issue` | 500 req/s | < 10ms | < 50ms | < 200ms | 0.1% |
| `POST /v1/grants/request` | 200 req/s | < 20ms | < 100ms | < 500ms | 0.1% |

---

## 5. Git Rules

- One logical change per commit. Do not bundle unrelated changes.
- Commit message format: `<crate-or-service>: <description>`
  - Example: `agentauth-core: add capability schema validation`
  - Example: `services/registry: implement nonce replay prevention`
- **Never commit**: `.env` files, secrets, private keys, or key material of any kind.
- **Never commit** a passing `grep` for plaintext key material (see CI checks).
- Commit after each phase gate passes — each phase must be independently reviewable.
- Database migrations must be committed in their own commit, separate from the application code that depends on them. This enforces the zero-downtime migration pattern: migrate first, deploy code second.

---

## 6. Phase Execution Order

Execute phases strictly in order. Do not begin a phase until the previous phase's gate passes.

```
Phase 1: agentauth-core
  → cargo nextest run -p agentauth-core ✓
  → cargo audit ✓

Phase 2: Database schema & migrations
  → sqlx migrate run ✓
  → rollback tests ✓

Phase 3: agentauth-registry + services/registry + services/verifier
  → cargo nextest run -p agentauth-registry (requires docker-compose) ✓
  → cargo nextest run --test integration ✓
  → k6 run load-tests/token-verify.js (must meet baselines) ✓

Phase 4: services/approval-ui
  → playwright test ✓

Phase 5: agentauth-sdk
  → cargo nextest run -p agentauth-sdk ✓

Phase 6: agentauth-py
  → pytest agentauth-py/tests/ ✓

Phase 7: Threat model + compliance hardening
  → cargo nextest run --test compliance ✓
  → docs/threat-model.md exists and covers all vectors listed in Phase 7 spec ✓

Phase 8: Discovery document + JSON Schema
  → JSON schema validation tests ✓

Phase 9: Observability + runbook
  → All metrics, traces, and alerts wired up ✓
  → docs/runbook.md exists with entries for every alert ✓
  → docs/capacity-planning.md exists ✓
```

---

## 7. Phase Gate Rule

**Do not proceed to the next phase until all tests in the current phase pass.**

Before marking any phase complete, run:

```bash
cargo audit                              # Zero known CVEs
cargo deny check licenses               # Zero license violations
cargo deny check bans                   # Zero banned crates
cargo clippy --workspace -- -D warnings # Zero warnings
cargo nextest run --workspace           # All tests pass
```

Fix any failures before moving forward.

---

## 8. Security Rules (Non-Negotiable)

These rules are absolute. No exceptions. Any PR that violates these rules is rejected regardless of other qualities.

### 8.1 Key Management

- **Never** store private keys as plaintext files in production. Only permitted production backends: `KmsSigningBackend` (AWS KMS, GCP KMS, Vault Transit). `EncryptedKeyfile` is permitted for local development only.
- **Never** log key material, token bytes, raw signatures, or nonces at any log level.
- **Never** use `unwrap()` on any crypto operation — all must return `Result`.
- The `allow-plaintext-keys` feature flag must **never** appear in production `Dockerfiles`, Helm values, or CI deployment configs.
- `InMemorySigningBackend` must **never** be instantiated outside of `#[cfg(test)]` blocks.

**CI enforcement (must return zero results):**
```bash
grep -r "InMemorySigningBackend" crates/ services/ --include="*.rs" | grep -v "#\[cfg(test)\]"
grep -r "PlaintextKeyfile" crates/ services/ --include="*.rs" | grep -v "#\[cfg(test)\]"
grep -r "allow-plaintext-keys" Dockerfile* deploy/ .github/ --include="*.yml"
```

### 8.2 Tokens

- **Always** verify the `key_id` field before selecting a public key for token verification. Never assume a single global key.
- **Always** check token binding (`cnf` claim) if present before proceeding.
- **Always** check nonce for replay **before** any other validation — fail fast.
- Token lifetime **must not** exceed 15 minutes. Longer lifetimes require explicit security review and a second reviewer.
- DPoP sender-constraint is **mandatory**. A bare AAT without a DPoP proof must be rejected by the verifier.

### 8.3 Network & Transport

- **Never** start any server without TLS configured. A server starting without TLS must `panic!`, not warn. Test: starting the server with no TLS config must return `Err`, not `Ok`.
- **All** internal service-to-service communication must use mTLS. No plaintext internal connections.
- **Never** trust `X-Forwarded-For` headers without verifying the source IP is the registered load balancer.
- Rate limiting is enforced at three independent layers: load balancer/WAF, registry middleware, and agent SDK `BehavioralRateLimiter`. All three must be present.

### 8.4 Approval UI & CSRF

- All state-mutating endpoints must validate: `SameSite=Strict` session cookie, Double Submit Cookie pattern, and `Origin` header matching the registered approval UI domain.
- The approval action must produce a **signed `ApprovalAssertion`** containing the exact capability set shown to the human, signed via WebAuthn/Passkey. The registry must verify this signature before writing the grant.
- `Transact` and `Delete` capabilities require two-step confirmation. Single-click approval for these is blocked.

### 8.5 Audit Log Integrity

- **Never** write to `audit_events` outside of the dedicated `AuditWriter` service.
- **Never** allow `UPDATE` or `DELETE` on `audit_events`. Verify in integration tests by attempting an `UPDATE` and asserting it fails.
- If an audit write fails, the **primary operation must also fail**. Use PostgreSQL transactions to make audit write and primary write atomic.
- Each audit event must include the hash of the previous event (hash chain).

### 8.6 SQL & Injection

- All SQL must use SQLx parameterized queries. Never build SQL via format macros or string concatenation.

### 8.7 Timing Attacks

- All signature verification must use timing-safe comparison via `ring`. Not manual `==` on byte slices.
- All token/nonce comparisons where the compared value is secret must use constant-time equality.

### 8.8 SDK Supply Chain

- The SDK must **never** make network requests to any URL except the configured registry endpoint and the configured KMS endpoint.
- New outbound URLs in the SDK require a security review comment in the PR.
- CI lint: enumerate all `reqwest::Client` and `hyper::Client` instantiations in `crates/agentauth-sdk/`. Each must be on an approved list in `deny.toml`.

### 8.9 Agent Provisioning (Secret Zero)

- Agents must never be deployed with a raw private key baked into the image or environment.
- Bootstrap flow: deploy agent with a one-time provisioning token (OTP) → agent calls `POST /v1/agents/bootstrap` → registry generates keypair inside KMS → returns key reference only → OTP is immediately invalidated.
- OTPs are single-use. Reuse returns `409 Conflict` and emits an audit event.

### 8.10 Banned Patterns (CI Grep Checks — Must Return Zero Results)

```bash
# No plaintext key backends in production code
grep -r "PlaintextKeyfile\|InMemorySigningBackend" crates/ services/ --include="*.rs" \
  | grep -v "#\[cfg(test)\]\|//.*test"

# No unwrap() in library crates outside tests
grep -rn "\.unwrap()" crates/ --include="*.rs" \
  | grep -v "#\[cfg(test)\]\|//.*safe because"

# No hardcoded secrets
grep -rn "secret\|password\|private_key\|api_key" crates/ services/ --include="*.rs" \
  | grep -v "//\|\"\"" | grep "= \""

# No SQL string interpolation
grep -rn "format!.*SELECT\|format!.*INSERT\|format!.*UPDATE\|format!.*DELETE" \
  crates/ services/ --include="*.rs"
```

---

## 9. Scalability Rules

### 9.1 IDs

All `AgentId`, `TokenId`, `GrantId`, and `EventId` types must use **UUID v7** (time-ordered). This enables efficient time-range queries without a separate `created_at` index and avoids the index fragmentation caused by random UUID v4 values. Do not use UUID v4 for any entity that will be queried by time range.

### 9.2 Serialization on the Hot Path

`Capability` and `AgentAccessToken` must be serialized to compact `prost` proto3 binary format for the Redis hot path. JSON is too slow at 10,000 req/s. JSON serialization is still required for all external-facing APIs and the discovery document.

Never deserialize a full token from Redis on every verify request. Cache only the fields needed for verification: JTI, expiry, revocation flag, service_provider_id, key_id. Fetch the full record from PostgreSQL only on cache miss.

### 9.3 Database Read/Write Separation

All DB writes go to the PostgreSQL **primary**. All reads for token verification go to **read replicas** via a round-robin connection pool. Use `deadpool-postgres` for connection pooling. Pool size: `(CPU cores × 2) + disk spindles`, typically 16–32 — make this configurable via environment variable.

The verifier service connects **only** to read replicas. It has no credentials for the primary. This is enforced at the database role level, not just application logic.

All queries on high-traffic paths must have `EXPLAIN ANALYZE` results documented in a comment on the query, verifying index usage. A sequential scan on `issued_tokens` or `audit_events` at production scale is a P0 incident.

### 9.4 Audit Log Partitioning

Audit events are append-only, high-volume, and will grow without bound. Partition from the start — retrofitting partitioning is painful.

```sql
CREATE TABLE audit_events (
    ...
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
) PARTITION BY RANGE (created_at);

-- Migration creates current and next month. audit-archiver creates future partitions 7 days in advance.
CREATE TABLE audit_events_2025_01 PARTITION OF audit_events
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');
```

The `audit-archiver` service: creates next-month partitions 7 days in advance, compresses old partitions to Parquet in cold storage (S3/GCS), and drops partitions beyond the retention window. It runs as a Kubernetes CronJob and must never block or lock the main `audit_events` table during archival.

Retention policy: hot (PostgreSQL) — 90 days; warm (compressed object storage) — 2 years; cold (glacier) — indefinitely.

### 9.5 Redis Architecture

Use **Redis Cluster** (minimum 3 primary + 3 replica nodes) in production. Single-node Redis is not acceptable for the token verification path.

Separate logical databases (or separate clusters under high load) for: token cache, nonce store, and rate limit counters. This allows independent eviction policies:

- **Token cache**: `maxmemory-policy allkeys-lru`. Tokens are recreatable from PostgreSQL — eviction is acceptable.
- **Nonce store**: `maxmemory-policy noeviction`. A forgotten nonce enables replay. If near capacity, reject new requests rather than evict nonces. Alert at 70% capacity.
- **Rate limit counters**: `maxmemory-policy volatile-ttl`. Counters have natural TTLs; TTL-based eviction is correct.

Implement **cache stampede prevention** for the token cache: when a token is near expiry and many requests arrive simultaneously, only one refreshes it (use a Redis lock with a short TTL). All others serve the slightly-stale cached value or wait briefly. Never let concurrent requests all hit the database simultaneously on the same token.

### 9.6 Sliding Window Rate Limiting (Redis Lua)

Use an atomic Lua script for rate limit checks. A non-atomic implementation has race conditions under concurrency.

```lua
local key = KEYS[1]
local window = tonumber(ARGV[1])
local limit = tonumber(ARGV[2])
local now = tonumber(ARGV[3])
local req_id = ARGV[4]

redis.call('ZREMRANGEBYSCORE', key, 0, now - window)
local count = redis.call('ZCARD', key)
if count < limit then
    redis.call('ZADD', key, now, req_id)
    redis.call('PEXPIRE', key, window)
    return {1, count + 1, limit}  -- allowed
else
    return {0, count, limit}       -- denied
end
```

### 9.7 Circuit Breakers

Every external dependency call must be wrapped in a circuit breaker with three states (closed, open, half-open). Configure thresholds explicitly — do not use library defaults without reviewing them.

| Dependency | Failure threshold | Recovery probe interval | Fallback behavior |
|---|---|---|---|
| KMS (signing) | 5 failures in 10s | 30s | Fail closed: reject new token issuance, serve verification from cache |
| KMS (key fetch) | 3 failures in 30s | 60s | Serve cached public keys (up to 24h stale is acceptable) |
| PostgreSQL primary | 3 failures in 5s | 15s | Fail closed: reject writes, serve reads from replica |
| PostgreSQL replica | 3 failures in 5s | 10s | Promote another replica or fall back to primary for reads |
| Redis cluster | 3 failures in 2s | 5s | Fall back to PostgreSQL for token verification (degraded mode) |
| Audit write | 5 failures in 10s | 30s | Buffer to in-memory queue (max 10,000 events), flush when recovered; if buffer full, fail primary operation |

Circuit breaker state transitions must emit a structured log event and a Prometheus metric (`agentauth_circuit_breaker_state{dependency, state}`).

### 9.8 Registry vs. Verifier Scaling

- `services/registry`: 3–10 replicas. CPU-bound (crypto). Scale on CPU utilization (target 60%).
- `services/verifier`: 10–100+ replicas. I/O-bound (Redis + network). Scale on request rate and p99 latency. Target: 1 verifier replica per ~1,000 req/s sustained.
- `services/audit-archiver`: 1 replica with leader election via PostgreSQL advisory lock. Not on the hot path.

### 9.9 Horizontal Scaling — Stateless Services

All service instances must be fully stateless between requests. Any state that must survive across requests (token cache, rate limit counters, nonce store) must live in Redis, not in process memory. Verify this: kill any single replica mid-load-test and confirm zero client errors beyond in-flight requests on that instance.

Set `PodDisruptionBudget` for each service:
- Registry: `minAvailable: 2`
- Verifier: `minAvailable: 3` (or 50% of replicas, whichever is higher)

### 9.10 Backpressure and Load Shedding

Services must shed load gracefully under overload rather than crashing or cascading.

**Connection-level (Axum middleware)**: Reject requests with `503 Service Unavailable` when in-flight request count exceeds a configurable limit. Return `Retry-After: <seconds>`. Do not queue indefinitely.

**Queue-level (async audit writes)**: The in-memory audit write buffer has a hard cap of 10,000 events. When the cap is reached, new events that cannot be durably committed must cause the primary operation to fail with `503`. Never silently drop audit events — this is a security invariant, not just a reliability concern.

### 9.11 Zero-Downtime Database Migrations

Every migration must follow the **expand/contract pattern**:

- **Phase A (Expand):** Add new columns as nullable or with defaults; add new tables. Old application code still works. Deploy this migration, then deploy the new application version.
- **Phase B (Backfill):** Backfill existing rows in a background job, not in the migration transaction itself, if the table has more than ~100,000 rows.
- **Phase C (Contract):** After all instances run the new version, drop old columns/tables in a separate follow-up migration.

Additional rules:
- Never add a `NOT NULL` column without a default in the same migration as inserting data.
- Never rename a column in a single migration — add the new name, backfill, drop the old name across three separate deploys.
- Never run `ALTER TABLE ... LOCK` on tables with more than 1M rows during business hours. Use `pg_repack` or `CONCURRENTLY` alternatives.
- Every migration must have a tested rollback. CI runs migrate forward then revert for every migration file.
- Migration commits must be separate from the application code commits that depend on them — migrate first, deploy code second.

### 9.12 Capacity Planning

`docs/capacity-planning.md` must be updated before each major release with current estimates for:

- Tokens verified per second (current and 12-month projection)
- Audit events per day (current and 12-month projection)
- Redis memory usage per 1M active tokens
- PostgreSQL write IOPS per 1,000 grant requests/minute
- Network egress per 1M token verifications

Starting sizing guideline for a new deployment: 3 registry replicas (2 vCPU / 4GB RAM), 5 verifier replicas (1 vCPU / 2GB RAM), PostgreSQL 16 (8 vCPU / 32GB RAM / 500GB SSD), Redis Cluster (3×2 vCPU / 8GB RAM). Scale PostgreSQL vertically before horizontally; add read replicas before sharding.

---

## 10. Stability Rules

### 10.1 Service Level Objectives

These SLOs are targets, not aspirations. Monitor continuously. Roll back any release that degrades an SLO.

| Service | SLO | Measurement window |
|---|---|---|
| Token verification availability | 99.99% (52 min/year downtime) | Rolling 30 days |
| Token verification p99 latency | < 5ms | Rolling 1 hour |
| Token issuance availability | 99.9% (8.7 hours/year downtime) | Rolling 30 days |
| Token issuance p99 latency | < 50ms | Rolling 1 hour |
| Grant approval availability | 99.9% | Rolling 30 days |
| Revocation propagation | < 100ms to Redis | Per-operation |
| Audit log write success rate | 99.99% | Rolling 30 days |

Define error budgets from these SLOs. When more than 50% of the monthly error budget is consumed, freeze non-critical deployments until the budget recovers.

### 10.2 Health Checks

Every service must implement three distinct health check endpoints. These are not optional — Kubernetes probes depend on them.

```
GET /health/live     # Liveness: is the process alive and not deadlocked?
GET /health/ready    # Readiness: can this instance accept traffic right now?
GET /health/startup  # Startup: has initialization completed?
```

**Liveness** (`/health/live`): Returns 200 if the process is running. Checks only internal invariants — never checks external dependencies. A failed liveness check causes Kubernetes to restart the pod.

**Readiness** (`/health/ready`): Returns 200 only when all required dependencies are reachable and the instance is ready to serve traffic. For the verifier, this means Redis is reachable AND the token cache has been warmed. A failed readiness check removes the pod from the load balancer without restarting it.

**Startup** (`/health/startup`): Returns 200 once one-time initialization is complete (connections established, keys loaded). Used during rolling deployments to prevent traffic reaching partially-initialized pods. Give startup a longer `failureThreshold` than liveness.

Each health check must complete within 500ms. A health check that hangs is worse than one that returns 503.

### 10.3 Graceful Shutdown

Every service must handle `SIGTERM` with a graceful drain. Set `terminationGracePeriodSeconds: 30` in all Kubernetes deployment specs.

Required shutdown sequence:
1. Receive `SIGTERM`
2. Mark readiness probe as failing immediately (removes instance from load balancer)
3. Wait for in-flight requests to complete (up to 20 seconds)
4. Flush the audit write buffer to PostgreSQL
5. Close database and Redis connections cleanly
6. Exit 0

If the drain timeout is exceeded, log a warning with the count of abandoned requests, then exit 1. Never hang indefinitely on shutdown.

```rust
// Axum graceful shutdown — implement in every service binary
let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

tokio::spawn(async move {
    tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to install signal handler")
        .recv().await;
    tracing::info!("SIGTERM received, starting graceful shutdown");
    let _ = shutdown_tx.send(());
});

axum::serve(listener, app)
    .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
    .await?;
```

### 10.4 Retry Policy

**Retry transient failures. Never retry non-transient failures.**

Transient (retry): `503`, `502`, `504`, connection reset, connection timeout.

Non-transient (do not retry): `400`, `401`, `403`, `404`, `409`, `422`. These will not succeed on retry.

```rust
// SDK defaults — configurable
RetryConfig {
    max_attempts: 3,
    initial_delay: Duration::from_millis(100),
    max_delay: Duration::from_secs(2),
    multiplier: 2.0,
    jitter: true,  // Full jitter: actual_delay = random(0, computed_delay)
}
```

Always respect a `Retry-After` header from the server — it overrides computed backoff. Never retry KMS operations more than once (latency budget is too tight). Never retry audit writes more than 5 times before routing to a dead-letter queue.

### 10.5 Idempotency

**Every mutating operation must be idempotent.**

- Token issuance: idempotent by `(grant_id, time_window)`. Same grant in the same 15-minute window always returns the same JTI.
- Agent registration: idempotent by `agent_id`. Re-registering with the same manifest is a no-op returning 200.
- OTP bootstrap: idempotent for the same OTP until used. After first use, returns 409.
- Grant requests: idempotent by `(agent_id, service_provider_id, capability_hash)` within a 5-minute window.
- Revocation: idempotent. Revoking an already-revoked token returns 200, not 409.
- Audit writes: idempotent by event ID. The `AuditWriter` deduplicates by event ID before inserting.

Callers must pass an `Idempotency-Key` header on all mutating requests. The registry stores idempotency keys in Redis with a 24-hour TTL and returns the cached response for duplicates.

### 10.6 Deployment Strategy

Use **rolling deployments** for routine releases. Use **blue/green deployments** for releases that include database schema changes. Never use the recreate deployment strategy in production.

**Rolling deployment requirements:**
- `maxSurge: 1`, `maxUnavailable: 0` for all services (capacity never reduced during rollout)
- New pods must pass startup and readiness probes before old pods are terminated
- Monitor error rate during rollout — if error rate increases by >0.1% during rollout, halt and roll back

**Blue/green for schema migrations:**
1. Run migration (expand phase) against production database
2. Deploy green environment with new application version
3. Shift 5% of traffic to green, monitor for 10 minutes
4. Shift 50%, monitor for 10 minutes
5. Shift 100%, decommission blue
6. Run migration (contract phase) in a follow-up deployment

The rollback procedure must be documented and tested for every release. Do not release code that cannot be rolled back within 5 minutes.

### 10.7 Feature Flags

Use feature flags for any change to the token verification path, capability schema, or behavioral envelope logic. This allows instant disabling without a deployment.

Use environment-variable-based flags for simplicity. Flag evaluation must not require a network call during token verification.

```rust
pub struct FeatureFlags {
    pub strict_behavioral_envelope: bool,   // env: AGENTAUTH_FF_STRICT_ENVELOPE
    pub require_dpop_proof: bool,           // env: AGENTAUTH_FF_REQUIRE_DPOP
    pub audit_hash_chain_enabled: bool,     // env: AGENTAUTH_FF_AUDIT_HASH_CHAIN
}

impl FeatureFlags {
    pub fn from_env() -> Self { ... }  // Called once at startup, stored in Arc<FeatureFlags>
}
```

Flags are read once at startup. Changing a flag requires a rolling restart (seconds). Do not hot-reload flags at runtime — the added complexity is not justified for this system.

### 10.8 Dependency Failure Modes

The system must degrade gracefully when dependencies fail. Never silently corrupt state.

| Failure | Registry behavior | Verifier behavior | Agent SDK behavior |
|---|---|---|---|
| Redis unavailable | Reject new token issuance. Reads fall back to PostgreSQL. | Fall back to PostgreSQL for verification. Latency increases. | Retries with backoff. Surfaces clear error if unresolvable. |
| PostgreSQL primary unavailable | Reject all writes. Reads from replica continue. | No impact (reads replica only). | Token issuance fails. Existing tokens work until expiry. |
| All PostgreSQL unavailable | Serve only cached Redis data. Reject all writes. | Serve only Redis cache. Cold misses fail. | Existing valid tokens usable. New issuance fails. |
| KMS unavailable | Reject new token issuance and key rotation. Existing tokens verifiable from cached public keys. | No impact (uses cached public keys). | No impact on existing tokens. New issuance fails. |
| All dependencies unavailable | Return `503` for all requests except `/health/live`. | Return `503` for verification. | Surfaces clear error, stops retrying, awaits recovery. |

These failure modes must be tested by the chaos engineering suite.

### 10.9 Chaos Engineering

The `chaos/` directory contains experiment definitions. Run experiments in a **dedicated staging environment only** — never in production without extensive mitigations.

Required experiments (must be defined before Phase 9 is complete):

- `redis-partition.yaml`: Partition Redis cluster into two halves. Expected: verifier falls back to PostgreSQL, p99 latency degrades to <50ms, error rate stays <1%.
- `db-primary-failure.yaml`: Kill PostgreSQL primary. Expected: writes fail, reads continue from replica, failover completes within 60 seconds, zero data loss.
- `kms-latency.yaml`: Add 2-second latency to all KMS calls. Expected: circuit breaker opens within 30 seconds, new token issuance fails with 503, existing token verification unaffected.
- `registry-kill-50pct.yaml`: Kill 50% of registry replicas simultaneously. Expected: zero client errors (PodDisruptionBudget enforced), surviving replicas handle load.
- `slow-consumer.yaml`: Make audit writes take 5 seconds each. Expected: audit buffer fills, backpressure applied, primary operations fail gracefully before buffer exhausted.

Each experiment must document: hypothesis, steady-state definition, experiment procedure, and expected vs. actual results in `chaos/<experiment>.results.md`.

### 10.10 Memory and Resource Limits

Set explicit resource requests and limits in all Kubernetes deployments. A service without limits can starve its neighbors.

Recommended starting limits (tune from load testing):

| Service | CPU request | CPU limit | Memory request | Memory limit |
|---|---|---|---|---|
| Registry | 500m | 2000m | 256Mi | 1Gi |
| Verifier | 250m | 1000m | 128Mi | 512Mi |
| Audit archiver | 100m | 500m | 128Mi | 256Mi |

Memory limits for Rust services should be approximately 3× the expected working set — the allocator can hold fragmented memory, and the limit should not be so tight that brief spikes trigger OOM kills. An OOM-killed pod does not drain gracefully.

Profile memory during the 1-hour soak test. RSS must not grow continuously over 1 hour. Any growth >10% over the soak period is a bug — investigate for memory leaks before release.

### 10.11 Tokio Runtime Tuning

Set explicitly in each service binary:

```rust
#[tokio::main(flavor = "multi_thread", worker_threads = 0)]  // 0 = use CPU count
async fn main() -> Result<()> { ... }
```

Use `tokio::task::spawn_blocking` only for truly blocking operations (file I/O, heavy crypto) — never for Redis or PostgreSQL calls (both have async drivers).

Monitor Tokio runtime metrics via the `tokio-metrics` crate and expose to Prometheus. Alert if task poll time p99 exceeds 1ms — it indicates a blocking operation on an async thread.

---

## 11. Observability Rules

Observability is not optional. You cannot operate a distributed auth system you cannot see. All three pillars — metrics, traces, logs — must be in place before any service is considered production-ready.

### 11.1 Structured Logging

All log output must be structured JSON via `tracing` + `tracing-subscriber` with JSON formatter. No unstructured `println!` or `eprintln!` in service code.

Every log line must include: `timestamp`, `level`, `service`, `version`, `trace_id`, `span_id`, `message`, and relevant domain fields.

**Log levels:**
- `ERROR`: Production incident — something is broken and needs immediate attention. Every ERROR must trigger an alert or have a documented reason why it does not.
- `WARN`: Unexpected but recoverable — circuit breaker opened, retry succeeded, degraded mode activated.
- `INFO`: Normal significant events — service started, new agent registered, grant approved/denied, circuit breaker state change.
- `DEBUG`: Verbose operational detail — disabled in production. Do not log token bytes or key material at any level.
- `TRACE`: Internal execution detail — never in production.

**Never log:** token bytes, signatures, nonces, full request/response bodies containing capability grants, user email addresses in plaintext (use hashed IDs), or key material.

### 11.2 Distributed Tracing

Use **OpenTelemetry** with the OTLP exporter. Every inbound HTTP request must create a trace. Every outbound call (Redis, PostgreSQL, KMS) must create a child span.

Propagate trace context via `traceparent` header (W3C Trace Context). The agent SDK must propagate trace context through to service provider calls.

Required custom span attributes:
- `agentauth.agent_id` — on all token and grant operations
- `agentauth.service_provider_id` — on all verification operations
- `agentauth.capability` — on all operations involving a specific capability
- `agentauth.token_jti` — on all token verification operations (hash if treating JTI as sensitive)
- `agentauth.cache_hit` — on all Redis operations (true/false)

Sample rate: 100% for errors and slow requests (>100ms). 1% for healthy fast requests. Use head-based sampling with tail-based override for errors.

### 11.3 Metrics

Use the `prometheus` crate. Expose metrics at `GET /metrics` on a **separate port** — not the API port. All metric names must be prefixed with `agentauth_`.

Required metrics:

```
# HTTP (auto-instrument via tower-http middleware)
agentauth_http_requests_total{method, path, status}             counter
agentauth_http_request_duration_seconds{method, path}           histogram
  buckets: [1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s]

# Business metrics
agentauth_tokens_issued_total{service_provider_id}              counter
agentauth_tokens_verified_total{outcome}                        counter
  # outcome: allowed, denied, expired, revoked, replayed
agentauth_grants_created_total{status}                          counter
agentauth_agents_registered_total                               counter
agentauth_revocations_total                                     counter

# Dependency health
agentauth_redis_operations_total{operation, status}             counter
agentauth_redis_operation_duration_seconds{operation}           histogram
agentauth_db_queries_total{query_name, status}                  counter
agentauth_db_query_duration_seconds{query_name}                 histogram
agentauth_kms_operations_total{operation, status}               counter
agentauth_kms_operation_duration_seconds{operation}             histogram

# Circuit breaker
agentauth_circuit_breaker_state{dependency, state}              gauge
  # state: 0=closed, 1=open, 2=half-open

# SLO tracking
agentauth_token_verify_slo_breach_total                         counter
agentauth_audit_write_lag_seconds                               gauge
agentauth_nonce_store_memory_pct                                gauge

# Resources
agentauth_active_connections{backend}                           gauge
agentauth_cache_hit_ratio{cache}                                gauge
agentauth_audit_buffer_pct                                      gauge
```

### 11.4 Alerting

Define alerts in `deploy/helm/*/alerts.yaml` (PrometheusRule format). Every alert must have a corresponding runbook entry in `docs/runbook.md`. An alert without a runbook is not permitted.

| Alert | Condition | Severity | Runbook entry |
|---|---|---|---|
| TokenVerifyHighErrorRate | Error rate > 0.1% over 5min | Critical | `#token-verify-errors` |
| TokenVerifyHighLatency | p99 > 5ms over 10min | Warning | `#token-verify-latency` |
| CircuitBreakerOpen | Any circuit breaker open > 2min | Warning | `#circuit-breaker` |
| RevocationLagHigh | Revocation propagation > 200ms p99 | Critical | `#revocation-lag` |
| AuditWriteLagHigh | Audit write lag > 30s | Warning | `#audit-lag` |
| AuditBufferNearCapacity | Audit buffer > 70% full | Critical | `#audit-buffer` |
| NonceStoreNearCapacity | Nonce store > 70% Redis memory | Critical | `#nonce-store` |
| DatabaseReplicaLag | Replica lag > 5s | Warning | `#replica-lag` |
| SLOErrorBudgetBurn | Error budget burn rate > 5× for 1h | Critical | `#slo-budget` |
| PodOOMKilled | Any pod OOM-killed | Warning | `#oom` |

### 11.5 Runbook Requirements

`docs/runbook.md` must have an entry for every alert before Phase 9 is marked complete. Each entry must include:

1. What this alert means in plain English
2. Immediate mitigation steps (first 5 minutes)
3. How to verify the system has recovered
4. Root cause investigation steps
5. Known false-positive conditions

---

## 12. CI Pipeline Requirements

The CI pipeline (`.github/workflows/ci.yml`) must include all of the following steps in order. Any step failure stops the pipeline.

```yaml
# Claude Code should generate the full YAML. Steps in order:

1.  cargo check --workspace
2.  cargo clippy --workspace -- -D warnings
3.  cargo audit                                  # Zero known CVEs
4.  cargo deny check licenses                   # Zero license violations
5.  cargo deny check bans                       # Zero banned crates
6.  docker-compose up -d                        # Starts postgres + redis + otel-collector
7.  sqlx migrate run                            # Apply all migrations
8.  sqlx migrate revert --all && sqlx migrate run   # Verify rollback + re-apply
9.  cargo nextest run --workspace               # All unit + integration tests
10. cargo tarpaulin --workspace                 # Fail if coverage < 80% per crate
11. cargo nextest run --test compliance         # Compliance / security invariant tests
12. [banned pattern grep checks]                # Shell step — section 8.10 patterns, exits 1 on match
13. maturin build                               # Python bindings compile
14. pytest agentauth-py/tests/                 # Python binding tests
15. cd services/approval-ui && npm ci && npm run build  # UI builds without warnings
16. playwright test                             # UI end-to-end tests
17. cargo doc --no-deps                         # Docs build without warnings
18. k6 run --vus 50 --duration 60s load-tests/token-verify.js  # Baseline load test
```

**Step 12** must be a real executable shell step, not a comment. It exits with code 1 if any banned pattern is found.

**Step 18** runs a 60-second warm-up load test against the local docker-compose stack. It must not exceed the p99 thresholds defined in Section 4 load test baselines. This catches latency regressions before staging.

**Nightly pipeline** (`.github/workflows/nightly.yml`) additionally runs:

```yaml
19. cargo nextest run --test stability -- --ignored    # Soak tests (1+ hour)
20. k6 run load-tests/token-verify.js                 # Full baseline load test suite
21. k6 run load-tests/scenarios/                      # Composite scenarios
```

---

## 13. Phase Specifications

### Phase 1: `agentauth-core`

**Goal**: Define all protocol types and cryptographic primitives. No I/O, no network calls, no database access in this crate.

**Key types to implement:**

- `AgentManifest` — agent identity document (UUID v7 ID, Ed25519 public key, capabilities requested, human principal ID, issued/expiry timestamps, signature)
- `Capability` — hierarchical enum: `Read { resource, filter }`, `Write { resource, conditions }`, `Transact { resource, max_value }`, `Custom { namespace, name, params }`
- `BehavioralEnvelope` — `max_requests_per_minute`, `max_burst`, `requires_human_online`, `human_confirmation_threshold`, `allowed_time_windows`, `max_session_duration_secs`
- `AgentAccessToken (AAT)` — `jti` (UUID v7), `agent_id`, `human_principal_id`, `service_provider_id`, `granted_capabilities`, `behavioral_envelope`, `issued_at`, `expires_at`, `cnf` (token binding), `key_id`, `signature`
- `ApprovalAssertion` — `grant_id`, `agent_id`, `granted_capabilities`, `behavioral_envelope`, `approved_at`, `approval_nonce: [u8; 32]`, `human_signature` (WebAuthn)

**Crypto module** (`crates/agentauth-core/src/crypto/`):

```rust
pub trait SigningBackend: Send + Sync {
    async fn sign(&self, message: &[u8]) -> Result<Signature, CryptoError>;
    async fn public_key(&self) -> Result<Ed25519PublicKey, CryptoError>;
    fn key_id(&self) -> &str;
}

pub struct KmsSigningBackend { ... }        // Production — AWS/GCP/Vault
pub struct InMemorySigningBackend { ... }  // Test only — #[cfg(test)]
```

Key operations:
- `sign_manifest(manifest, backend) -> Result<SignedManifest>`
- `verify_manifest(manifest, public_key) -> Result<(), CryptoError>`
- `sign_token(aat, backend) -> Result<SignedAAT>`
- `verify_token(aat, public_key) -> Result<(), CryptoError>` — must use timing-safe comparison
- `generate_nonce() -> [u8; 32]`
- `hash_chain_event(previous_hash, event_content) -> [u8; 32]`

Serialization must be **deterministic**: given the same input, the output bytes must be identical across calls, machines, and versions. Test this explicitly — it is required for hash chain integrity.

**Agent key backends:**
```rust
pub enum AgentKeyBackend {
    AwsKms { key_id: String },
    GcpKms { key_resource_name: String },
    VaultTransit { mount: String, key_name: String },
    EncryptedKeyfile { path: PathBuf },  // Dev only — emits tracing::warn! at startup
    #[cfg(feature = "allow-plaintext-keys")]
    PlaintextKeyfile { path: PathBuf },  // Never in production
}
```

---

### Phase 2: Database Schema & Migrations

**Goal**: All migration files in `migrations/` with both forward and rollback migrations.

Tables required: `human_principals`, `agent_manifests`, `service_providers`, `capability_grants`, `issued_tokens`, `audit_events`.

**Critical constraints:**
- `audit_events` must have `previous_event_hash BYTEA NOT NULL`, `row_hash BYTEA NOT NULL`, and `registry_signature BYTEA NOT NULL`.
- `audit_events` must be range-partitioned by `created_at` (monthly). The migration creates the first two monthly partitions.
- Down migrations must restore schema completely. CI runs migrate then revert for every migration file.
- `agentauth_service` DB role must have `INSERT, SELECT` on `audit_events` only — explicitly `REVOKE UPDATE, DELETE`.
- All foreign keys must have `ON DELETE` behavior explicitly specified.
- All indexes must use `CREATE INDEX CONCURRENTLY` to avoid locking on large tables.

---

### Phase 3: `agentauth-registry` + `services/registry` + `services/verifier`

**Goal**: Core server handling all protocol operations.

**Registry routes:**
```
POST   /v1/agents/register
POST   /v1/agents/bootstrap          # OTP-based first-time provisioning
GET    /v1/agents/:agent_id
DELETE /v1/agents/:agent_id

POST   /v1/grants/request
GET    /v1/grants/:grant_id
POST   /v1/grants/:grant_id/approve  # Requires signed ApprovalAssertion
POST   /v1/grants/:grant_id/deny

POST   /v1/tokens/issue              # Idempotent: same grant + same 15-min window = same token
POST   /v1/tokens/revoke

GET    /v1/audit/:agent_id
GET    /v1/audit/:agent_id/verify    # Chain integrity check
POST   /v1/audit/record

GET    /.well-known/agentauth
GET    /.well-known/agentauth/keys   # All current + retired verify-only public keys

GET    /health/live
GET    /health/ready
GET    /health/startup
GET    /metrics                      # Prometheus — separate port, not the API port
```

**Verifier routes (separate binary, no write access):**
```
POST   /v1/tokens/verify             # Checks nonce, binding, expiry, revocation, DPoP
GET    /.well-known/agentauth/keys

GET    /health/live
GET    /health/ready
GET    /health/startup
GET    /metrics                      # Prometheus — separate port
```

**Token verify implementation (strict ordering):**
1. Check nonce (Redis) — reject replay immediately
2. Check revocation (Redis) — reject revoked tokens
3. Verify `cnf` token binding if present
4. Verify DPoP proof signature
5. Verify token signature using `key_id`
6. Check expiry last

Target: sub-5ms p99 when Redis is warm. Token issuance idempotent: same grant + same 15-minute window returns the same token without re-signing.

**Approval flood protection:**
- Maximum 5 pending approvals per agent — return `429` if exceeded
- Approval requests expire after 1 hour
- Denied requests trigger exponential backoff cooldown: 1h, 4h, 24h

**Observability:** All Axum routes instrumented with `tower-http` tracing layer. All Redis and PostgreSQL calls create child spans. `agentauth.agent_id` and `agentauth.service_provider_id` set on all relevant spans.

---

### Phase 4: `services/approval-ui`

**Goal**: Human-facing React + TypeScript approval interface.

**Routes:** `/approve/:grant_id`, `/agents`, `/agents/:agent_id/activity`

**UX requirements:**
- `BehavioralEnvelope` renders in plain English (e.g., `max_requests_per_minute: 30` → "Up to 30 actions per minute")
- Each capability shows the exact resource scope in plain English
- `Transact` and `Delete` require a two-step explicit confirmation — not single click
- No third-party analytics, tracking scripts, or CDN-loaded resources that phone home
- Approval submission uses WebAuthn/Passkey to sign the `ApprovalAssertion`
- UI shows a clear error state (not blank screen) when registry is unreachable

---

### Phase 5: `agentauth-sdk` (Rust)

**Goal**: Everything a Rust-based agent needs to authenticate with an AgentAuth-enabled service.

**Primary interface:**
```rust
pub struct AgentAuthClient {
    registry_url: Url,
    manifest: SignedManifest,
    key_backend: AgentKeyBackend,
}

impl AgentAuthClient {
    pub fn new(config: AgentAuthConfig) -> Result<Self>;
    pub async fn register(&self) -> Result<()>;
    pub async fn request_grant(
        &self,
        service_provider_id: &str,
        capabilities: Vec<Capability>,
        envelope: BehavioralEnvelope,
    ) -> Result<CapabilityGrant>;
    pub async fn get_token(&self, service_provider_id: &str) -> Result<AgentAccessToken>;
    pub async fn authenticate_request(
        &self,
        service_provider_id: &str,
        request: &mut reqwest::Request,
    ) -> Result<()>;  // Sets Authorization: AgentBearer <AAT>, AgentDPoP <proof>
}
```

**`BehavioralRateLimiter`**: Client-side sliding window limiter enforcing the `BehavioralEnvelope`. Mandatory — part of the compliance contract, not an optional optimization.

**Token caching**: In-memory, keyed by `service_provider_id`. Refresh when `expires_at - now < 2 minutes`. Refresh uses single-use refresh token (rotated on each use).

**DPoP**: Every authenticated outgoing request includes a DPoP proof — signature over request method + URL using the agent's private key. Attached as `AgentDPoP: <proof>` header.

**Retry**: Transient errors (503, 502, 504, connection reset) retried with exponential backoff + full jitter, max 3 attempts. Non-transient 4xx errors fail immediately. Respect `Retry-After` header.

**Connection reuse**: One `reqwest::Client` (with connection pool) per registry URL. Do not create new clients per request.

---

### Phase 6: `agentauth-py` (Python Bindings)

**Goal**: Python-accessible SDK via PyO3 + maturin, wrapping `agentauth-sdk`.

```python
from agentauth import AgentAuthClient, Capability, BehavioralEnvelope
from agentauth.integrations.langchain import AgentAuthToolkit
from agentauth.integrations.autogen import AgentAuthMiddleware
```

---

### Phase 7: Threat Model + Compliance Hardening

**Goal**: `docs/threat-model.md` must exist and cover all of the following vectors (with mitigations, residual risks, and detection mechanisms for each):

- Stolen registry signing key (HSM + key rotation procedure)
- Stolen agent private key (KMS-only backend + DPoP requirement)
- Phished human principal credential (WebAuthn/Passkey)
- AAT interception and replay (nonce + DPoP sender-constraint)
- AAT claims forgery (key_id verification + short lifetime)
- Cross-service-provider token reuse (`service_provider_id` binding)
- Malicious service provider forging audit records (chain hash + registry signature)
- Approval UI CSRF (SameSite + double-submit + Origin header + signed assertion)
- Grant request flooding / approval spam (pending cap + cooldown)
- Agent manifest spoofing / impersonation (registry + model origin verification)
- Registry compromise (HSM: no raw key on server; damage limited)
- Supply chain attack on SDK (reproducible builds + cargo-deny + no telemetry policy)
- Secret zero / first provisioning (OTP bootstrap — agent never handles raw private key)

---

### Phase 8: Discovery Document + JSON Schema

**Goal**: Machine-readable protocol advertisement.

```json
{
  "agentauth_version": "1.0",
  "registry_endpoint": "https://...",
  "verifier_endpoint": "https://...",
  "supported_capabilities": ["read", "write", "transact", "custom"],
  "supported_resources": ["calendar", "email", "files", "messages"],
  "trusted_model_origins": ["anthropic.com", "openai.com"],
  "token_endpoint": "https://.../v1/tokens/verify",
  "approval_ui_endpoint": "https://.../approve",
  "bootstrap_endpoint": "https://.../v1/agents/bootstrap",
  "public_key": "<registry ed25519 public key, base64url>",
  "keys_endpoint": "https://.../.well-known/agentauth/keys",
  "behavioral_limits": {
    "max_requests_per_minute": 60,
    "max_burst": 10,
    "max_token_lifetime_seconds": 900
  }
}
```

Publish a **JSON Schema file** for validating discovery documents as an `agentauth-schema` crate and a corresponding PyPI package. Integration tests must validate the live discovery document against this schema.

---

### Phase 9: Observability + Runbook

**Goal**: Complete observability infrastructure and operational documentation.

**Deliverables required before this phase is marked complete:**

- All metrics from Section 11.3 are emitted by all services and scraped by Prometheus
- All traces from Section 11.2 are exported to the OpenTelemetry Collector
- All alerts from Section 11.4 are defined in `deploy/helm/*/alerts.yaml` (PrometheusRule format)
- `docs/runbook.md` has an entry for every alert defined in Section 11.4
- `docs/capacity-planning.md` has initial sizing estimates and 12-month projections
- Grafana dashboards are defined in `deploy/grafana/` for: token verification SLO, circuit breaker states, cache hit ratios, audit log lag, and per-service request rates
- All chaos experiments in `chaos/` are defined with hypothesis and expected results documented
- Nightly stability pipeline is configured in `.github/workflows/nightly.yml`
