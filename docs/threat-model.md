# AgentAuth Threat Model

This document identifies security threats to the AgentAuth system and describes the mitigations implemented, residual risks, and detection mechanisms for each threat vector.

## Overview

AgentAuth is a capability-based authentication system for AI agents. The system involves:
- **Registry Service**: Issues and manages agent access tokens (AATs)
- **Verifier Service**: Validates tokens for service providers
- **Agent SDK**: Client library for agents to authenticate
- **Approval UI**: Human-facing interface for capability approvals
- **Service Providers**: Third-party services that accept AgentAuth tokens

## Threat Vectors

---

### 1. Stolen Registry Signing Key

**Attack Description:**
An attacker obtains the registry's private signing key, enabling them to forge arbitrary AATs and capability grants. This is a catastrophic compromise that would allow impersonation of any agent.

**Mitigations Implemented:**
- Registry signing keys are stored exclusively in Hardware Security Modules (HSM) via KMS backends (AWS KMS, GCP Cloud KMS, or HashiCorp Vault Transit)
- Keys never exist in plaintext form on any server - all signing operations occur within the HSM
- The `InMemorySigningBackend` and `PlaintextKeyfile` backends are disabled in production via compile-time feature flags and CI checks
- Key rotation is supported via the `key_id` field in tokens and the `/well-known/agentauth/keys` endpoint

**Residual Risk:**
- Compromise of cloud provider KMS infrastructure (extremely rare, covered by provider SLAs)
- Insider threat with KMS admin access

**Detection:**
- Monitor KMS audit logs for unusual signing operations
- Alert on tokens signed with unknown `key_id` values
- Track signing operation volume - sudden spikes indicate compromise

---

### 2. Stolen Agent Private Key

**Attack Description:**
An attacker steals an agent's private key, allowing them to authenticate as that agent and perform actions within the agent's granted capabilities.

**Mitigations Implemented:**
- Agent keys are stored in KMS, never as plaintext in the agent's runtime environment
- OTP-based bootstrap flow ensures agents never handle raw private keys
- DPoP (Demonstration of Proof of Possession) sender-constraint requires proof of key possession for every authenticated request
- Short token lifetimes (15 minutes maximum) limit the window of exploitation

**Residual Risk:**
- Compromise of the KMS where agent keys are stored
- If an attacker also has network MITM capability during the 15-minute token window

**Detection:**
- Monitor for DPoP proofs signed with keys not matching the `cnf` claim
- Alert on authentication from unexpected IP addresses/regions
- Track behavioral anomalies (sudden capability usage patterns)

---

### 3. Phished Human Principal Credential

**Attack Description:**
An attacker tricks a human principal into approving malicious capability grants through phishing or social engineering.

**Mitigations Implemented:**
- WebAuthn/Passkey required for approval assertions - phishing-resistant by design
- Approval assertions are cryptographically signed and bound to the specific capability set shown
- Two-step confirmation required for dangerous capabilities (Transact, Delete)
- Capability descriptions rendered in plain English to prevent confusion

**Residual Risk:**
- Real-time phishing where attacker proxies the legitimate UI
- Social engineering to approve legitimate-looking but malicious requests

**Detection:**
- Monitor for unusual approval patterns (time, location, frequency)
- Alert on approvals from new devices
- Audit log all approval decisions with human-readable capability descriptions

---

### 4. AAT Interception and Replay

**Attack Description:**
An attacker intercepts a valid AAT from network traffic and attempts to reuse it.

**Mitigations Implemented:**
- Nonce-based replay prevention: each token usage includes a unique nonce stored in Redis
- DPoP sender-constraint: tokens are bound to a specific keypair; replay without the private key fails
- Short token lifetimes (15 minutes) minimize replay window
- TLS required for all communications

**Residual Risk:**
- If an attacker compromises both the AAT and the agent's DPoP private key
- Redis failure allowing nonce storage bypass

**Detection:**
- Alert on nonce replay attempts (logged with source IP)
- Monitor for high-volume verification requests with identical nonces
- Track verification failures with "nonce already used" errors

---

### 5. AAT Claims Forgery

**Attack Description:**
An attacker attempts to modify token claims (capabilities, expiry, service provider binding) to escalate privileges.

**Mitigations Implemented:**
- All token claims are covered by the registry's Ed25519 signature
- `key_id` field is verified before selecting the public key for verification
- Tampered claims cause signature verification failure
- Verification uses constant-time comparison (via `subtle` crate) to prevent timing attacks

**Residual Risk:**
- Theoretical cryptographic break of Ed25519 (currently considered infeasible)

**Detection:**
- Log all verification failures with reason codes
- Alert on repeated forgery attempts from the same source
- Monitor for attempts to use old/rotated `key_id` values

---

### 6. Cross-Service-Provider Token Reuse

**Attack Description:**
An attacker takes a token issued for Service Provider A and attempts to use it at Service Provider B.

**Mitigations Implemented:**
- Every AAT contains a `service_provider_id` claim binding it to a specific service provider
- Verifiers must validate that the `service_provider_id` matches their own identity
- DPoP proofs include the target URL, preventing replay across different endpoints

**Residual Risk:**
- Service provider misconfiguration not checking `service_provider_id`

**Detection:**
- Log service_provider_id mismatches at verification time
- Alert on tokens verified by unexpected service providers (via audit logs)

---

### 7. Malicious Service Provider Forging Audit Records

**Attack Description:**
A compromised or malicious service provider attempts to forge audit records to hide unauthorized access or frame other entities.

**Mitigations Implemented:**
- Audit events include a hash chain: each event contains `previous_event_hash`
- Registry signs all audit records with `registry_signature`
- `UPDATE` and `DELETE` operations are revoked at the database level for the service role
- Audit events are immutable and append-only

**Residual Risk:**
- Registry compromise allowing signing of malicious audit records
- Database admin with elevated privileges

**Detection:**
- Audit chain integrity verification endpoint (`/v1/audit/:agent_id/verify`)
- Alert on hash chain breaks or missing events
- Regular automated chain integrity checks

---

### 8. Approval UI CSRF

**Attack Description:**
An attacker tricks a logged-in human principal into submitting an approval request through a malicious website.

**Mitigations Implemented:**
- `SameSite=Strict` cookie policy prevents cross-site request inclusion
- Double Submit Cookie pattern: CSRF token in cookie and request body must match
- `Origin` header validation rejects requests from unexpected origins
- Approval assertion is cryptographically signed via WebAuthn - cannot be forged without the user's authenticator

**Residual Risk:**
- Browser vulnerabilities bypassing SameSite
- XSS in the approval UI itself (mitigated by CSP)

**Detection:**
- Log requests with missing or mismatched CSRF tokens
- Alert on approval attempts from unexpected origins
- Monitor for patterns indicating automated CSRF attempts

---

### 9. Grant Request Flooding / Approval Spam

**Attack Description:**
An attacker floods the system with grant requests or approval submissions to overwhelm human reviewers or cause denial of service.

**Mitigations Implemented:**
- Maximum 5 pending approval requests per agent at any time
- Approval requests expire after 1 hour if not acted upon
- Denied requests trigger exponential backoff cooldown: 1h, 4h, 24h
- Rate limiting at load balancer, middleware, and SDK levels

**Residual Risk:**
- Distributed attack from many compromised agents
- Resource exhaustion if flood protection thresholds are too high

**Detection:**
- Monitor pending approval counts per agent
- Alert on agents hitting the pending limit repeatedly
- Track denial rates and cooldown trigger frequency

---

### 10. Agent Manifest Spoofing / Impersonation

**Attack Description:**
An attacker creates a fake agent manifest claiming to be a legitimate agent or claiming capabilities beyond what should be allowed.

**Mitigations Implemented:**
- Agent manifests are signed and registered through the registry
- `model_origin` field tracks the source model provider
- Registry validates manifest claims during registration
- Capability grants cannot exceed what was declared in the original manifest

**Residual Risk:**
- Compromised agent provisioning pipeline
- Social engineering to get a malicious manifest approved

**Detection:**
- Audit log all manifest registrations
- Alert on capability requests exceeding manifest declarations
- Monitor for manifests claiming sensitive `model_origin` values

---

### 11. Registry Compromise

**Attack Description:**
An attacker gains control of the registry service, potentially accessing all agent data and signing keys.

**Mitigations Implemented:**
- Signing keys stored in HSM - even full registry compromise cannot extract raw keys
- Registry does not store tokens - only issues them
- Write operations require proper authentication
- Separation of registry (write-heavy) and verifier (read-only) services limits blast radius
- Database credentials are minimal-privilege

**Residual Risk:**
- Attacker could issue new tokens during compromise window
- Access to agent metadata and grant history

**Detection:**
- Intrusion detection on registry hosts
- Anomaly detection on token issuance rates
- File integrity monitoring on registry binaries
- Alert on unusual database query patterns

---

### 12. Supply Chain Attack on SDK

**Attack Description:**
An attacker compromises the SDK build process or dependencies to inject malicious code that exfiltrates tokens or keys.

**Mitigations Implemented:**
- `cargo-deny` enforces license compliance and bans known-malicious crates
- `cargo audit` checks for known vulnerabilities in dependencies
- SDK makes no network requests except to configured registry/KMS endpoints
- No telemetry or analytics in the SDK
- Banned crates list includes native-tls (uses rustls only)

**Residual Risk:**
- Zero-day in a dependency before it's added to advisory database
- Compromise of crates.io infrastructure

**Detection:**
- Reproducible builds enable verification
- Network monitoring can detect unexpected outbound connections
- Dependency diff review in CI for any new dependencies

---

### 13. Secret Zero / First Provisioning

**Attack Description:**
An attacker intercepts the initial provisioning process to obtain or substitute agent credentials.

**Mitigations Implemented:**
- OTP (One-Time Password) bootstrap flow: agent receives single-use provisioning token
- OTP is immediately invalidated after first use
- Keypair is generated inside KMS - agent only receives a key reference, never the raw key
- Reuse of OTP returns `409 Conflict` and emits security audit event

**Residual Risk:**
- OTP interception during initial deployment
- Compromise of the system distributing OTPs

**Detection:**
- Audit log all bootstrap attempts
- Alert on OTP reuse attempts
- Monitor for bootstrap requests from unexpected sources

---

## Security Invariants

The following invariants must hold for the system to be secure:

1. **No plaintext keys in production**: `InMemorySigningBackend` and `PlaintextKeyfile` never instantiated outside `#[cfg(test)]`
2. **Constant-time comparisons**: All secret comparisons use `subtle::ConstantTimeEq` or ed25519-dalek's internal constant-time verification
3. **TLS everywhere**: No service starts without TLS configured
4. **Audit atomicity**: Audit write failures cause the primary operation to fail
5. **Nonce uniqueness**: Every token usage has a unique nonce that cannot be replayed
6. **DPoP binding**: Tokens without valid DPoP proofs are rejected
7. **Capability boundary**: Agents cannot request capabilities beyond their manifest

---

## Incident Response

In case of security incident:

1. **Immediate**: Revoke affected tokens, rotate compromised keys via KMS
2. **Short-term**: Audit logs to determine blast radius, notify affected service providers
3. **Long-term**: Root cause analysis, implement additional mitigations, update threat model

---

## Review Schedule

This threat model should be reviewed:
- After any significant architectural change
- After any security incident
- At minimum quarterly

Last reviewed: Stage 5 implementation
