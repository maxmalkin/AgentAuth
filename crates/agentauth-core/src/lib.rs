//! AgentAuth Core Library
//!
//! This crate provides the core protocol types, cryptographic primitives, and token logic
//! for the AgentAuth system. It has zero I/O, zero network calls, and zero database access.
//!
//! # Key Types
//!
//! - [`AgentManifest`] - Agent identity document
//! - [`Capability`] - Hierarchical capability enum
//! - [`BehavioralEnvelope`] - Rate limiting and behavioral constraints
//! - [`AgentAccessToken`] - The primary access token type
//! - [`ApprovalAssertion`] - Human approval proof
//!
//! # Crypto Module
//!
//! The [`crypto`] module provides signing backends and verification functions.
//! Production deployments must use [`crypto::KmsSigningBackend`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]

pub mod crypto;
pub mod error;
pub mod types;

pub use error::CoreError;
pub use types::*;
