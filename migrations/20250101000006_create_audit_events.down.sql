-- Rollback audit_events table

-- Revoke permissions from service role
REVOKE ALL ON audit_events FROM agentauth_service;
REVOKE ALL ON audit_events_2025_01 FROM agentauth_service;
REVOKE ALL ON audit_events_2025_02 FROM agentauth_service;

-- Drop indexes
DROP INDEX IF EXISTS idx_audit_events_type;
DROP INDEX IF EXISTS idx_audit_events_created;
DROP INDEX IF EXISTS idx_audit_events_token;
DROP INDEX IF EXISTS idx_audit_events_service_provider;
DROP INDEX IF EXISTS idx_audit_events_agent;

-- Drop partitions and parent table
DROP TABLE IF EXISTS audit_events_2025_02;
DROP TABLE IF EXISTS audit_events_2025_01;
DROP TABLE IF EXISTS audit_events;

-- Drop the enum type
DROP TYPE IF EXISTS audit_event_type;

-- Note: We don't drop the agentauth_service role as it may be used elsewhere
