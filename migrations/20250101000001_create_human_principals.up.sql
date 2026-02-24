-- Create human_principals table
-- Human users who can approve agent actions via WebAuthn/Passkey

CREATE TABLE human_principals (
    id UUID PRIMARY KEY,
    email VARCHAR(255) NOT NULL,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    webauthn_credential_id BYTEA,
    webauthn_public_key BYTEA,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Unique constraint on email
CREATE UNIQUE INDEX idx_human_principals_email
    ON human_principals (email);

-- Index for credential lookup during WebAuthn authentication
CREATE INDEX idx_human_principals_credential
    ON human_principals (webauthn_credential_id)
    WHERE webauthn_credential_id IS NOT NULL;
