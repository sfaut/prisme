-- Date of the last successful connection to the connection's server, in UTC,
-- in ISO 8601 format: written with the flavor and the version of the server
-- (connection test, reading of the catalog), shown by the connections manager.
-- Empty string as long as the connection never succeeded.
ALTER TABLE connections ADD COLUMN connected_at TEXT NOT NULL DEFAULT '';
