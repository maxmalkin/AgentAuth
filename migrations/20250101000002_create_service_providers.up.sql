-- Create service_providers table
-- External services that agents can authenticate to

CREATE TABLE service_providers (
    id UUID PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    -- Base URL for token verification callbacks
    verification_endpoint VARCHAR(2048) NOT NULL,
    -- Public key for verifying service provider signatures
    public_key BYTEA NOT NULL,
    -- Allowed capabilities this service provider accepts
    allowed_capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Whether the service provider is active
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for active service providers lookup
CREATE INDEX idx_service_providers_active
    ON service_providers (is_active)
    WHERE is_active = TRUE;

-- Unique constraint on name
CREATE UNIQUE INDEX idx_service_providers_name
    ON service_providers (name);
