-- Create agent_manifests table
-- Agent identity documents containing capabilities and key material

CREATE TABLE agent_manifests (
    id UUID PRIMARY KEY,
    -- Human principal who owns this agent
    human_principal_id UUID NOT NULL REFERENCES human_principals(id) ON DELETE CASCADE,
    -- Agent's display name
    name VARCHAR(255) NOT NULL,
    -- Agent's description
    description TEXT,
    -- Ed25519 public key for verifying agent signatures
    public_key BYTEA NOT NULL,
    -- Key ID for key rotation support
    key_id VARCHAR(255) NOT NULL,
    -- Capabilities this agent is allowed to request
    requested_capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Default behavioral envelope for this agent
    default_behavioral_envelope JSONB NOT NULL,
    -- Model origin (e.g., anthropic.com, openai.com)
    model_origin VARCHAR(255),
    -- Signature over the manifest by the agent's private key
    signature BYTEA NOT NULL,
    -- Manifest validity period
    issued_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    -- Whether the agent is active (not revoked)
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for looking up agents by human principal
CREATE INDEX idx_agent_manifests_human_principal
    ON agent_manifests (human_principal_id);

-- Index for active agents
CREATE INDEX idx_agent_manifests_active
    ON agent_manifests (is_active)
    WHERE is_active = TRUE;

-- Index for key_id lookup (for key rotation)
CREATE INDEX idx_agent_manifests_key_id
    ON agent_manifests (key_id);

-- Unique constraint on name per human principal
CREATE UNIQUE INDEX idx_agent_manifests_name_per_principal
    ON agent_manifests (human_principal_id, name);
