-- Queries run when each connection to the target database opens
-- (SET NAMES utf8mb4; SET SESSION sql_mode = ...), separated by ";".
-- Only session settings are allowed: the check is done by
-- src/connections.rs, which also repeats the maximum length.
ALTER TABLE connections ADD COLUMN init_sql TEXT NOT NULL DEFAULT '' CHECK (length(init_sql) <= 4000);
