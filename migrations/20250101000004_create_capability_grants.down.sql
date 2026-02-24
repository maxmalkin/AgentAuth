-- Rollback capability_grants table
DROP INDEX IF EXISTS idx_capability_grants_idempotency;
DROP INDEX IF EXISTS idx_capability_grants_active;
DROP INDEX IF EXISTS idx_capability_grants_pending;
DROP INDEX IF EXISTS idx_capability_grants_service_provider;
DROP INDEX IF EXISTS idx_capability_grants_agent;
DROP TABLE IF EXISTS capability_grants;
DROP TYPE IF EXISTS grant_status;
