-- PostgreSQL initialization script for AgentAuth
-- This script runs when the primary database is first created

-- Create the agentauth_service role with limited permissions for the application
CREATE ROLE agentauth_service WITH LOGIN PASSWORD 'agentauth_service_dev';

-- Create the agentauth_readonly role for the verifier service
CREATE ROLE agentauth_readonly WITH LOGIN PASSWORD 'agentauth_readonly_dev';

-- Grant connect permissions
GRANT CONNECT ON DATABASE agentauth TO agentauth_service;
GRANT CONNECT ON DATABASE agentauth TO agentauth_readonly;

-- Create replication user for replica
CREATE ROLE replication_user WITH REPLICATION LOGIN PASSWORD 'replication_dev';

-- The actual tables and permissions will be created by SQLx migrations
-- This script only sets up the roles

-- Note: The audit_events table will have special permissions:
-- agentauth_service: INSERT, SELECT only (no UPDATE, DELETE)
-- This is enforced by the migration, not here
