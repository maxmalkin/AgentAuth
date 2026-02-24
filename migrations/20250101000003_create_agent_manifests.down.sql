-- Rollback agent_manifests table
DROP INDEX IF EXISTS idx_agent_manifests_name_per_principal;
DROP INDEX IF EXISTS idx_agent_manifests_key_id;
DROP INDEX IF EXISTS idx_agent_manifests_active;
DROP INDEX IF EXISTS idx_agent_manifests_human_principal;
DROP TABLE IF EXISTS agent_manifests;
