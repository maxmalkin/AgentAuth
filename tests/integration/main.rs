//! Integration tests for AgentAuth services.
//!
//! These tests require docker-compose running with PostgreSQL and Redis.
//! Run: `docker-compose up -d`
//! Then: `cargo nextest run --test integration`

mod helpers;

mod audit;
mod concurrency;
mod happy_path;
mod idempotency;
mod revocation;
mod token_verification;
