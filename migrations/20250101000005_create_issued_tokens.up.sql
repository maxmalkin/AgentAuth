-- Create issued_tokens table
-- Agent Access Tokens (AAT) that have been issued

CREATE TABLE issued_tokens (
    -- JWT ID (jti claim) - UUID v7 for time-ordering
    jti UUID PRIMARY KEY,
    -- The grant this token was issued from
    grant_id UUID NOT NULL REFERENCES capability_grants(id) ON DELETE CASCADE,
    -- The agent this token was issued to
    agent_id UUID NOT NULL REFERENCES agent_manifests(id) ON DELETE CASCADE,
    -- The service provider this token is for
    service_provider_id UUID NOT NULL REFERENCES service_providers(id) ON DELETE CASCADE,
    -- Human principal who approved the underlying grant
    human_principal_id UUID NOT NULL REFERENCES human_principals(id) ON DELETE CASCADE,
    -- Key ID used to sign this token (for key rotation)
    key_id VARCHAR(255) NOT NULL,
    -- Token binding (cnf claim) - typically DPoP thumbprint
    token_binding BYTEA,
    -- Capabilities encoded in this token
    granted_capabilities JSONB NOT NULL,
    -- Behavioral envelope for this token
    behavioral_envelope JSONB NOT NULL,
    -- Token validity period
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    -- Revocation status
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE,
    revoked_at TIMESTAMPTZ,
    revocation_reason VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for token verification by jti (already primary key, but explicit)
-- Index for looking up tokens by grant
CREATE INDEX idx_issued_tokens_grant
    ON issued_tokens (grant_id);

-- Index for looking up tokens by agent
CREATE INDEX idx_issued_tokens_agent
    ON issued_tokens (agent_id);

-- Index for looking up tokens by service provider
CREATE INDEX idx_issued_tokens_service_provider
    ON issued_tokens (service_provider_id);

-- Index for active (non-revoked, non-expired) tokens
CREATE INDEX idx_issued_tokens_active
    ON issued_tokens (jti, expires_at)
    WHERE is_revoked = FALSE;

-- Index for idempotent token issuance (same grant in same time window)
-- Application handles 15-minute window lookup using this index
CREATE INDEX idx_issued_tokens_idempotency
    ON issued_tokens (grant_id, issued_at DESC);
