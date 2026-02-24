-- Rollback issued_tokens table
DROP INDEX IF EXISTS idx_issued_tokens_idempotency;
DROP INDEX IF EXISTS idx_issued_tokens_active;
DROP INDEX IF EXISTS idx_issued_tokens_service_provider;
DROP INDEX IF EXISTS idx_issued_tokens_agent;
DROP INDEX IF EXISTS idx_issued_tokens_grant;
DROP TABLE IF EXISTS issued_tokens;
