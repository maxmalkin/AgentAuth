-- Rollback service_providers table
DROP INDEX IF EXISTS idx_service_providers_name;
DROP INDEX IF EXISTS idx_service_providers_active;
DROP TABLE IF EXISTS service_providers;
