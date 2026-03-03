//! AgentAuth Registry Service Logic
//!
//! This crate provides the service logic for the AgentAuth registry,
//! including agent management, grant handling, and token operations.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
// Allow these lints where they're unavoidable or acceptable
#![allow(clippy::too_many_arguments)] // Some functions legitimately need many parameters
#![allow(clippy::needless_raw_string_hashes)] // SQL strings with hashes are clearer

pub mod config;
pub mod db;
pub mod demo;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod services;
pub mod state;

pub use config::RegistryConfig;
pub use error::RegistryError;
pub use state::AppState;
