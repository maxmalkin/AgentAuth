//! Compliance tests for AgentAuth security invariants.
//!
//! These tests verify that critical security properties hold:
//! - Tampered tokens are rejected
//! - Behavioral envelopes are enforced
//! - DPoP binding prevents token theft
//! - Nonce replay is detected
//! - Capability boundaries are enforced
//! - Audit log integrity is maintained

mod audit_integrity;
mod behavioral_envelope;
mod capability_boundary;
mod dpop_binding;
mod nonce_replay;
mod token_security;
