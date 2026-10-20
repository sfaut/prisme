-- Retention of the console (src/console.rs), in days, set in the profile:
-- the queries of the history and the lines of the debug log older than that
-- are purged at startup, then every hour. The debug log is the server's:
-- the local user's setting applies to it.
ALTER TABLE users ADD COLUMN history_retention_days INTEGER NOT NULL DEFAULT 30 CHECK (history_retention_days BETWEEN 1 AND 3650);
ALTER TABLE users ADD COLUMN debug_retention_days INTEGER NOT NULL DEFAULT 30 CHECK (debug_retention_days BETWEEN 1 AND 3650);
