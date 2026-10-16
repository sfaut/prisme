-- Flavor of the connection's server (mysql or mariadb), detected on the last
-- successful connection: the explorer shows it as a badge next to the source,
-- even before opening its catalog. Empty string as long as the connection
-- never succeeded.
ALTER TABLE connections ADD COLUMN server_flavor TEXT NOT NULL DEFAULT '' CHECK (server_flavor IN ('', 'mysql', 'mariadb'));
