//! Compliance tests for AgentAuth security invariants.
//!
//! These tests verify that critical security properties hold:
//! - Tampered tokens are rejected
//! - Behavioral envelopes are enforced
//! - DPoP binding prevents token theft
//! - Nonce replay is detected
//! - Capability boundaries are enforced
//! - Audit log integrity is maintained

mod token_security;
mod behavioral_envelope;
mod dpop_binding;
mod nonce_replay;
mod capability_boundary;
mod audit_integrity;
