//! Token cache for the AgentAuth SDK.
//!
//! The cache stores issued tokens and automatically triggers refresh
//! when tokens are near expiry. This reduces unnecessary network calls
//! and improves latency for authenticated requests.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use agentauth_core::types::ServiceProviderId;

/// A cached token entry.
#[derive(Debug, Clone)]
pub struct CachedToken {
    /// The raw token string (AAT).
    pub token: String,

    /// Token ID (JTI).
    pub jti: String,

    /// When the token expires.
    pub expires_at: DateTime<Utc>,

    /// When the token was cached.
    pub cached_at: DateTime<Utc>,

    /// Service provider this token is bound to.
    pub service_provider_id: ServiceProviderId,
}

impl CachedToken {
    /// Creates a new cached token entry.
    #[must_use]
    pub fn new(
        token: String,
        jti: String,
        expires_at: DateTime<Utc>,
        service_provider_id: ServiceProviderId,
    ) -> Self {
        Self {
            token,
            jti,
            expires_at,
            cached_at: Utc::now(),
            service_provider_id,
        }
    }

    /// Returns true if the token has expired.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    /// Returns true if the token should be refreshed.
    ///
    /// # Arguments
    ///
    /// * `refresh_window` - Time before expiry to trigger refresh
    #[must_use]
    pub fn should_refresh(&self, refresh_window: Duration) -> bool {
        let refresh_threshold =
            self.expires_at - chrono::Duration::from_std(refresh_window).unwrap_or_default();
        Utc::now() >= refresh_threshold
    }

    /// Returns the remaining time until expiry.
    #[must_use]
    pub fn time_to_expiry(&self) -> Duration {
        let diff = self.expires_at - Utc::now();
        diff.to_std().unwrap_or(Duration::ZERO)
    }
}

/// Thread-safe token cache.
///
/// Stores tokens keyed by service provider ID. Each agent should have
/// at most one active token per service provider.
#[derive(Clone)]
pub struct TokenCache {
    /// Cached tokens by service provider ID.
    pub(crate) tokens: Arc<RwLock<HashMap<ServiceProviderId, CachedToken>>>,

    /// Time before expiry to trigger refresh.
    pub(crate) refresh_window: Duration,
}

impl TokenCache {
    /// Creates a new token cache.
    ///
    /// # Arguments
    ///
    /// * `refresh_window` - How long before expiry to trigger proactive refresh
    #[must_use]
    pub fn new(refresh_window: Duration) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
            refresh_window,
        }
    }

    /// Gets a cached token for a service provider.
    ///
    /// Returns `None` if no valid token is cached.
    /// Returns the token even if it should be refreshed (caller handles refresh).
    pub async fn get(&self, service_provider_id: &ServiceProviderId) -> Option<CachedToken> {
        let tokens = self.tokens.read().await;
        let entry = tokens.get(service_provider_id)?;

        // Don't return expired tokens
        if entry.is_expired() {
            tracing::debug!(
                service_provider_id = %service_provider_id,
                jti = %entry.jti,
                "Cached token has expired"
            );
            return None;
        }

        Some(entry.clone())
    }

    /// Stores a token in the cache.
    pub async fn put(&self, entry: CachedToken) {
        let service_provider_id = entry.service_provider_id;

        tracing::debug!(
            service_provider_id = %service_provider_id,
            jti = %entry.jti,
            expires_at = %entry.expires_at,
            "Caching token"
        );

        let mut tokens = self.tokens.write().await;
        tokens.insert(service_provider_id, entry);
    }

    /// Removes a token from the cache.
    pub async fn remove(&self, service_provider_id: &ServiceProviderId) {
        let mut tokens = self.tokens.write().await;
        if let Some(entry) = tokens.remove(service_provider_id) {
            tracing::debug!(
                service_provider_id = %service_provider_id,
                jti = %entry.jti,
                "Removed token from cache"
            );
        }
    }

    /// Checks if a cached token needs refresh.
    ///
    /// Returns `Some(true)` if the token exists and needs refresh,
    /// `Some(false)` if the token exists and doesn't need refresh,
    /// `None` if no token is cached.
    pub async fn needs_refresh(&self, service_provider_id: &ServiceProviderId) -> Option<bool> {
        let tokens = self.tokens.read().await;
        let entry = tokens.get(service_provider_id)?;

        if entry.is_expired() {
            return None;
        }

        Some(entry.should_refresh(self.refresh_window))
    }

    /// Clears all cached tokens.
    pub async fn clear(&self) {
        let mut tokens = self.tokens.write().await;
        let count = tokens.len();
        tokens.clear();
        tracing::debug!(count, "Cleared token cache");
    }

    /// Removes expired tokens from the cache.
    ///
    /// Returns the number of tokens removed.
    pub async fn cleanup_expired(&self) -> usize {
        let mut tokens = self.tokens.write().await;
        let before = tokens.len();
        tokens.retain(|_, entry| !entry.is_expired());
        let removed = before - tokens.len();

        if removed > 0 {
            tracing::debug!(removed, "Cleaned up expired tokens");
        }

        removed
    }

    /// Returns the number of cached tokens.
    pub async fn len(&self) -> usize {
        self.tokens.read().await.len()
    }

    /// Returns true if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.tokens.read().await.is_empty()
    }
}

impl Default for TokenCache {
    fn default() -> Self {
        // Default refresh window: 2 minutes before expiry
        Self::new(Duration::from_secs(120))
    }
}

impl std::fmt::Debug for TokenCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenCache")
            .field("refresh_window", &self.refresh_window)
            .finish_non_exhaustive()
    }
}

/// Handle for pending token refresh operations.
///
/// This prevents multiple concurrent refresh attempts for the same token.
pub struct RefreshGuard {
    /// Service provider being refreshed.
    service_provider_id: ServiceProviderId,
    /// Shared state for tracking in-progress refreshes.
    in_progress: Arc<RwLock<HashSet<ServiceProviderId>>>,
}

impl RefreshGuard {
    /// Attempts to acquire a refresh guard.
    ///
    /// Returns `None` if a refresh is already in progress for this service provider.
    pub async fn try_acquire(
        service_provider_id: ServiceProviderId,
        in_progress: Arc<RwLock<HashSet<ServiceProviderId>>>,
    ) -> Option<Self> {
        {
            let mut set = in_progress.write().await;
            if set.contains(&service_provider_id) {
                return None;
            }
            set.insert(service_provider_id);
        }
        Some(Self {
            service_provider_id,
            in_progress,
        })
    }

    /// Releases the refresh guard.
    pub async fn release(self) {
        let mut set = self.in_progress.write().await;
        set.remove(&self.service_provider_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_service_provider() -> ServiceProviderId {
        ServiceProviderId::new()
    }

    fn make_token(service_provider_id: ServiceProviderId, expires_in: Duration) -> CachedToken {
        CachedToken::new(
            "test-token".to_string(),
            Uuid::now_v7().to_string(),
            Utc::now() + chrono::Duration::from_std(expires_in).unwrap(),
            service_provider_id,
        )
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = TokenCache::new(Duration::from_secs(60));
        let sp_id = make_service_provider();
        let token = make_token(sp_id, Duration::from_secs(300));

        cache.put(token.clone()).await;

        let retrieved = cache.get(&sp_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.as_ref().map(|t| &t.jti), Some(&token.jti));
    }

    #[tokio::test]
    async fn test_cache_returns_none_for_missing() {
        let cache = TokenCache::new(Duration::from_secs(60));
        let sp_id = make_service_provider();

        let retrieved = cache.get(&sp_id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_cache_returns_none_for_expired() {
        let cache = TokenCache::new(Duration::from_secs(60));
        let sp_id = make_service_provider();

        // Create an already-expired token
        let token = CachedToken::new(
            "test-token".to_string(),
            Uuid::now_v7().to_string(),
            Utc::now() - chrono::Duration::seconds(1),
            sp_id,
        );

        cache.put(token).await;

        let retrieved = cache.get(&sp_id).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_cache_remove() {
        let cache = TokenCache::new(Duration::from_secs(60));
        let sp_id = make_service_provider();
        let token = make_token(sp_id, Duration::from_secs(300));

        cache.put(token).await;
        assert!(cache.get(&sp_id).await.is_some());

        cache.remove(&sp_id).await;
        assert!(cache.get(&sp_id).await.is_none());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = TokenCache::new(Duration::from_secs(60));

        for _ in 0..5 {
            let sp_id = make_service_provider();
            let token = make_token(sp_id, Duration::from_secs(300));
            cache.put(token).await;
        }

        assert_eq!(cache.len().await, 5);

        cache.clear().await;
        assert!(cache.is_empty().await);
    }

    #[tokio::test]
    async fn test_cache_cleanup_expired() {
        let cache = TokenCache::new(Duration::from_secs(60));

        // Add a valid token
        let sp1 = make_service_provider();
        let valid = make_token(sp1, Duration::from_secs(300));
        cache.put(valid).await;

        // Add an expired token
        let sp2 = make_service_provider();
        let expired = CachedToken::new(
            "expired-token".to_string(),
            Uuid::now_v7().to_string(),
            Utc::now() - chrono::Duration::seconds(1),
            sp2,
        );
        cache.put(expired).await;

        assert_eq!(cache.len().await, 2);

        let removed = cache.cleanup_expired().await;
        assert_eq!(removed, 1);
        assert_eq!(cache.len().await, 1);

        // The valid token should still be there
        assert!(cache.get(&sp1).await.is_some());
    }

    #[tokio::test]
    async fn test_needs_refresh() {
        let cache = TokenCache::new(Duration::from_secs(60));
        let sp_id = make_service_provider();

        // Token that doesn't need refresh (expires in 5 minutes)
        let fresh = make_token(sp_id, Duration::from_secs(300));
        cache.put(fresh).await;
        assert_eq!(cache.needs_refresh(&sp_id).await, Some(false));

        // Replace with token that needs refresh (expires in 30 seconds)
        let stale = make_token(sp_id, Duration::from_secs(30));
        cache.put(stale).await;
        assert_eq!(cache.needs_refresh(&sp_id).await, Some(true));
    }

    #[test]
    fn test_cached_token_is_expired() {
        let sp_id = make_service_provider();

        let future = CachedToken::new(
            "token".to_string(),
            "jti".to_string(),
            Utc::now() + chrono::Duration::hours(1),
            sp_id,
        );
        assert!(!future.is_expired());

        let past = CachedToken::new(
            "token".to_string(),
            "jti".to_string(),
            Utc::now() - chrono::Duration::seconds(1),
            sp_id,
        );
        assert!(past.is_expired());
    }

    #[test]
    fn test_cached_token_should_refresh() {
        let sp_id = make_service_provider();

        // Token expires in 5 minutes, refresh window is 2 minutes
        let token = CachedToken::new(
            "token".to_string(),
            "jti".to_string(),
            Utc::now() + chrono::Duration::minutes(5),
            sp_id,
        );
        assert!(!token.should_refresh(Duration::from_secs(120)));

        // Token expires in 1 minute, refresh window is 2 minutes
        let token = CachedToken::new(
            "token".to_string(),
            "jti".to_string(),
            Utc::now() + chrono::Duration::minutes(1),
            sp_id,
        );
        assert!(token.should_refresh(Duration::from_secs(120)));
    }
}
