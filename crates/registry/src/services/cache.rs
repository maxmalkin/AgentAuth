//! Redis cache service.

use crate::config::RedisConfig;
use crate::error::{RegistryError, Result};
use auth_core::TokenId;
use redis::aio::MultiplexedConnection;
use redis::AsyncCommands;
use std::time::Duration;

/// Cache service for Redis operations.
pub struct CacheService {
    /// Redis connection.
    conn: MultiplexedConnection,
    /// Token cache prefix.
    token_prefix: String,
    /// Nonce store prefix.
    nonce_prefix: String,
    /// Rate limit prefix.
    rate_limit_prefix: String,
}

impl CacheService {
    /// Create a new cache service.
    pub async fn new(config: &RedisConfig) -> Result<Self> {
        let url = config
            .urls
            .first()
            .ok_or_else(|| RegistryError::Cache("no Redis URLs configured".to_string()))?;

        let client = redis::Client::open(url.as_str())
            .map_err(|e| RegistryError::Cache(format!("failed to create Redis client: {e}")))?;

        let conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to connect to Redis: {e}")))?;

        Ok(Self {
            conn,
            token_prefix: config.token_cache_prefix.clone(),
            nonce_prefix: config.nonce_store_prefix.clone(),
            rate_limit_prefix: config.rate_limit_prefix.clone(),
        })
    }

    /// Check health of Redis connection.
    pub async fn health_check(&self) -> Result<()> {
        let mut conn = self.conn.clone();
        let _: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .map_err(|e| RegistryError::Cache(format!("Redis health check failed: {e}")))?;
        Ok(())
    }

    // ========================================================================
    // Token Cache Operations
    // ========================================================================

    /// Cache a token's verification data.
    pub async fn cache_token(
        &self,
        jti: &TokenId,
        service_provider_id: &str,
        expires_at: i64,
        is_revoked: bool,
    ) -> Result<()> {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.token_prefix, jti);

        // Store as hash for efficient partial reads
        let _: () = redis::pipe()
            .hset(&key, "sp", service_provider_id)
            .hset(&key, "exp", expires_at)
            .hset(&key, "revoked", if is_revoked { "1" } else { "0" })
            .expire(&key, (expires_at - chrono::Utc::now().timestamp()).max(1))
            .query_async(&mut conn)
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to cache token: {e}")))?;

        Ok(())
    }

    /// Get cached token data.
    pub async fn get_cached_token(&self, jti: &TokenId) -> Result<Option<CachedToken>> {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.token_prefix, jti);

        let result: Option<(String, i64, String)> = redis::cmd("HMGET")
            .arg(&key)
            .arg("sp")
            .arg("exp")
            .arg("revoked")
            .query_async(&mut conn)
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to get cached token: {e}")))?;

        match result {
            Some((sp, exp, revoked)) if !sp.is_empty() => Ok(Some(CachedToken {
                service_provider_id: sp,
                expires_at: exp,
                is_revoked: revoked == "1",
            })),
            _ => Ok(None),
        }
    }

    /// Mark a token as revoked in cache.
    pub async fn mark_token_revoked(&self, jti: &TokenId) -> Result<()> {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.token_prefix, jti);

        let _: () = conn
            .hset(&key, "revoked", "1")
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to mark token revoked: {e}")))?;

        Ok(())
    }

    // ========================================================================
    // Nonce Store Operations
    // ========================================================================

    /// Check if a nonce has been used (and mark it as used).
    /// Returns true if nonce was already used (replay detected).
    pub async fn check_and_set_nonce(&self, nonce: &str, ttl: Duration) -> Result<bool> {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.nonce_prefix, nonce);

        // Use SETNX to atomically check and set
        let was_set: bool = conn
            .set_nx(&key, "1")
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to check nonce: {e}")))?;

        if was_set {
            // Set expiry
            #[allow(clippy::cast_possible_wrap)]
            let expiry = ttl.as_secs() as i64;
            let _: () = conn
                .expire(&key, expiry)
                .await
                .map_err(|e| RegistryError::Cache(format!("failed to set nonce expiry: {e}")))?;
        }

        // Return true if nonce was already used (was_set = false means it existed)
        Ok(!was_set)
    }

    // ========================================================================
    // Rate Limit Operations (Sliding Window)
    // ========================================================================

    /// Check rate limit using sliding window algorithm.
    /// Returns (allowed, current_count, limit).
    #[allow(clippy::cast_possible_wrap)] // These values won't overflow in practice
    pub async fn check_rate_limit(
        &self,
        key_suffix: &str,
        window_secs: u64,
        limit: u64,
    ) -> Result<(bool, u64, u64)> {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.rate_limit_prefix, key_suffix);
        let now = chrono::Utc::now().timestamp_millis();
        let window_ms = (window_secs * 1000) as i64;

        // Lua script for atomic sliding window rate limiting
        let script = redis::Script::new(
            r#"
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
                return {1, count + 1, limit}
            else
                return {0, count, limit}
            end
            "#,
        );

        let req_id = uuid::Uuid::now_v7().to_string();
        let result: Vec<i64> = script
            .key(&key)
            .arg(window_ms)
            .arg(limit as i64)
            .arg(now)
            .arg(&req_id)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| RegistryError::Cache(format!("rate limit check failed: {e}")))?;

        let allowed = result.first().copied().unwrap_or(0) == 1;
        #[allow(clippy::cast_sign_loss)]
        let count = result.get(1).copied().unwrap_or(0) as u64;
        #[allow(clippy::cast_sign_loss)]
        let returned_limit = result.get(2).copied().unwrap_or(limit as i64) as u64;

        Ok((allowed, count, returned_limit))
    }

    // ========================================================================
    // Idempotency Key Operations
    // ========================================================================

    /// Store an idempotency key with response.
    pub async fn store_idempotency_key(
        &self,
        key: &str,
        response: &str,
        ttl: Duration,
    ) -> Result<()> {
        let mut conn = self.conn.clone();
        let full_key = format!("idempotency:{key}");

        let _: () = conn
            .set_ex(&full_key, response, ttl.as_secs())
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to store idempotency key: {e}")))?;

        Ok(())
    }

    /// Get an idempotency key response.
    pub async fn get_idempotency_key(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.conn.clone();
        let full_key = format!("idempotency:{key}");

        let result: Option<String> = conn
            .get(&full_key)
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to get idempotency key: {e}")))?;

        Ok(result)
    }

    // ========================================================================
    // OTP Operations
    // ========================================================================

    /// Validate and consume a one-time provisioning token.
    /// Returns true if the OTP was valid and has been consumed.
    pub async fn validate_and_consume_otp(&self, otp: &str) -> Result<bool> {
        let mut conn = self.conn.clone();
        let key = format!("otp:{otp}");

        // Use GETDEL to atomically get and delete (consume) the OTP
        let result: Option<String> = redis::cmd("GETDEL")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to validate OTP: {e}")))?;

        Ok(result.is_some())
    }

    /// Store a new OTP with expiry.
    pub async fn store_otp(&self, otp: &str, ttl: Duration) -> Result<()> {
        let mut conn = self.conn.clone();
        let key = format!("otp:{otp}");

        let _: () = conn
            .set_ex(&key, "1", ttl.as_secs())
            .await
            .map_err(|e| RegistryError::Cache(format!("failed to store OTP: {e}")))?;

        Ok(())
    }
}

/// Cached token data.
#[derive(Debug, Clone)]
pub struct CachedToken {
    /// Service provider ID.
    pub service_provider_id: String,
    /// Expiry timestamp.
    pub expires_at: i64,
    /// Whether the token is revoked.
    pub is_revoked: bool,
}
