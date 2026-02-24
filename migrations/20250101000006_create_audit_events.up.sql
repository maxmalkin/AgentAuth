-- Create audit_events table (partitioned)
-- Append-only audit log with hash chain for integrity verification

-- Event type enum
CREATE TYPE audit_event_type AS ENUM (
    'agent_registered',
    'agent_updated',
    'agent_revoked',
    'grant_requested',
    'grant_approved',
    'grant_denied',
    'grant_revoked',
    'token_issued',
    'token_verified_allowed',
    'token_verified_denied',
    'token_revoked',
    'rate_limit_exceeded',
    'security_violation'
);

-- Create the partitioned parent table
CREATE TABLE audit_events (
    id UUID NOT NULL,
    -- Event classification
    event_type audit_event_type NOT NULL,
    -- Entities involved
    agent_id UUID,
    service_provider_id UUID,
    human_principal_id UUID,
    grant_id UUID,
    token_jti UUID,
    -- Event details (structured JSON)
    event_data JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Outcome of the event
    outcome VARCHAR(50) NOT NULL,
    -- Error details if outcome is failure
    error_message TEXT,
    -- Request context
    source_ip INET,
    user_agent VARCHAR(512),
    request_id UUID,
    trace_id VARCHAR(64),
    -- Hash chain for integrity
    previous_event_hash BYTEA NOT NULL,
    row_hash BYTEA NOT NULL,
    -- Registry signature over this event
    registry_signature BYTEA NOT NULL,
    -- Timestamp (partition key)
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Primary key includes partition key for partition pruning
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- Create partitions for current and next month
-- Using 2025-01 as base; audit-archiver creates future partitions
CREATE TABLE audit_events_2025_01 PARTITION OF audit_events
    FOR VALUES FROM ('2025-01-01') TO ('2025-02-01');

CREATE TABLE audit_events_2025_02 PARTITION OF audit_events
    FOR VALUES FROM ('2025-02-01') TO ('2025-03-01');

-- Index for looking up events by agent
CREATE INDEX idx_audit_events_agent
    ON audit_events (agent_id, created_at DESC)
    WHERE agent_id IS NOT NULL;

-- Index for looking up events by service provider
CREATE INDEX idx_audit_events_service_provider
    ON audit_events (service_provider_id, created_at DESC)
    WHERE service_provider_id IS NOT NULL;

-- Index for looking up events by token
CREATE INDEX idx_audit_events_token
    ON audit_events (token_jti, created_at DESC)
    WHERE token_jti IS NOT NULL;

-- Index for hash chain verification (sequential order)
CREATE INDEX idx_audit_events_created
    ON audit_events (created_at);

-- Index for event type filtering
CREATE INDEX idx_audit_events_type
    ON audit_events (event_type, created_at DESC);

-- Create the service role with restricted permissions
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'agentauth_service') THEN
        CREATE ROLE agentauth_service WITH LOGIN;
    END IF;
END
$$;

-- Grant SELECT and INSERT only - no UPDATE or DELETE
GRANT SELECT, INSERT ON audit_events TO agentauth_service;
GRANT SELECT, INSERT ON audit_events_2025_01 TO agentauth_service;
GRANT SELECT, INSERT ON audit_events_2025_02 TO agentauth_service;

-- Explicitly revoke UPDATE and DELETE
REVOKE UPDATE, DELETE ON audit_events FROM agentauth_service;
REVOKE UPDATE, DELETE ON audit_events_2025_01 FROM agentauth_service;
REVOKE UPDATE, DELETE ON audit_events_2025_02 FROM agentauth_service;
