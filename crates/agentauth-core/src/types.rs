//! Core protocol types for AgentAuth.
//!
//! All types in this module use UUID v7 for IDs (time-ordered) and support
//! both JSON and compact binary (prost) serialization.

pub mod agent;
pub mod capability;
pub mod envelope;
pub mod token;

pub use agent::{AgentId, AgentManifest, HumanPrincipalId, ServiceProviderId, SignedManifest};
pub use capability::Capability;
pub use envelope::BehavioralEnvelope;
pub use token::{
    AgentAccessToken, ApprovalAssertion, CapabilityGrant, GrantId, GrantStatus, SignedAAT, TokenId,
};
