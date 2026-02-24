-- Create capability_grants table
-- Approved capability grants linking agents to service providers

-- Grant status enum
CREATE TYPE grant_status AS ENUM (
    'pending',      -- Awaiting human approval
    'approved',     -- Human approved the grant
    'denied',       -- Human denied the grant
    'revoked',      -- Previously approved but now revoked
    'expired'       -- Grant expired before approval
);

CREATE TABLE capability_grants (
    id UUID PRIMARY KEY,
    -- The agent requesting capabilities
    agent_id UUID NOT NULL REFERENCES agent_manifests(id) ON DELETE CASCADE,
    -- The service provider being accessed
    service_provider_id UUID NOT NULL REFERENCES service_providers(id) ON DELETE CASCADE,
    -- The human principal who approved (NULL if pending)
    approved_by UUID REFERENCES human_principals(id) ON DELETE SET NULL,
    -- Granted capabilities (subset of requested)
    granted_capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Behavioral envelope for this grant
    behavioral_envelope JSONB NOT NULL,
    -- Current status
    status grant_status NOT NULL DEFAULT 'pending',
    -- Approval nonce for replay prevention
    approval_nonce BYTEA,
    -- Human signature over the approval assertion
    approval_signature BYTEA,
    -- When the grant request was created
    requested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- When the grant was approved/denied (NULL if pending)
    decided_at TIMESTAMPTZ,
    -- When the grant expires (for pending: expiry of approval window)
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for looking up grants by agent
CREATE INDEX idx_capability_grants_agent
    ON capability_grants (agent_id);

-- Index for looking up grants by service provider
CREATE INDEX idx_capability_grants_service_provider
    ON capability_grants (service_provider_id);

-- Index for pending grants (for approval UI)
CREATE INDEX idx_capability_grants_pending
    ON capability_grants (status, requested_at)
    WHERE status = 'pending';

-- Index for active grants (approved and not expired)
CREATE INDEX idx_capability_grants_active
    ON capability_grants (agent_id, service_provider_id, status)
    WHERE status = 'approved';

-- Idempotency: prevent duplicate pending grants for same agent+service+capabilities
CREATE UNIQUE INDEX idx_capability_grants_idempotency
    ON capability_grants (agent_id, service_provider_id, md5(granted_capabilities::text))
    WHERE status = 'pending';
