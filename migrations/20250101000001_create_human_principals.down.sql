-- Rollback human_principals table
DROP INDEX IF EXISTS idx_human_principals_credential;
DROP INDEX IF EXISTS idx_human_principals_email;
DROP TABLE IF EXISTS human_principals;
