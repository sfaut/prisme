-- Version of the connection's server (raw result of SELECT VERSION()), seen on
-- the last successful connection: the explorer shows it in the tooltip of the
-- flavor badge, even before opening the catalog. Empty string as long as the
-- connection never succeeded.
ALTER TABLE connections ADD COLUMN server_version TEXT NOT NULL DEFAULT '';
