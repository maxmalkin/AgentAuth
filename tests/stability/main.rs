//! Stability tests for AgentAuth services.
//!
//! All tests are marked `#[ignore]` so they only run in the nightly pipeline.
//! Run: `cargo nextest run --test stability -- --ignored`

mod helpers;

mod audit_chain;
mod concurrent_grants;
mod memory_soak;
mod recovery;
mod throughput;
