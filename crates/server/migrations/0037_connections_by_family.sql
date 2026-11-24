-- Connections split by family of settings (docs/TODO-SQLITE.md, step 2):
-- `connections` keeps what every engine shares, and the settings of each
-- family go into a table of their own, keyed by the connection. The engines,
-- their flavors and the protocols of the locations become reference tables:
-- a new engine, flavor or protocol joins by an INSERT of its migration,
-- without rebuilding `connections`.
--
-- The migration runs with the foreign keys off (src/store.rs), as SQLite asks
-- for a table rebuilt: dropping the old `connections` must not empty
-- `editors.connection_id`. The foreign keys are checked before the commit.
-- The maximum lengths are repeated in the validation of src/connections.rs.

-- Families of settings: each one names the table that holds the settings of
-- its connections.
CREATE TABLE setting_families (
    -- Key of the family: 'server' (connection_servers), 'files'
    -- (connection_files).
    key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 20)
) STRICT, WITHOUT ROWID;

INSERT INTO setting_families (key) VALUES ('server'), ('files');

-- Engines of the sources: one row per connector of Prisme (Engine,
-- crates/connectors/src/api.rs).
CREATE TABLE engines (
    -- Key of the engine, as the connector names it ('mysql').
    key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 20),
    -- Family of the settings of its connections.
    family TEXT NOT NULL REFERENCES setting_families (key)
) STRICT, WITHOUT ROWID;

INSERT INTO engines (key, family) VALUES ('mysql', 'server');

-- Flavors an engine detects on connection (Flavor,
-- crates/connectors/src/flavor.rs).
CREATE TABLE flavors (
    -- Key of the flavor ('mysql', 'mariadb').
    key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 20),
    -- Engine that detects it.
    engine TEXT NOT NULL REFERENCES engines (key)
) STRICT, WITHOUT ROWID;

INSERT INTO flavors (key, engine) VALUES ('mysql', 'mysql'), ('mariadb', 'mysql');

-- Protocols of the locations of the databases held in files (Scheme,
-- crates/connectors/src/location.rs): the protocols Prisme serves.
CREATE TABLE schemes (
    -- Key of the protocol ('file', 'http', 'https', 'memory').
    key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 20)
) STRICT, WITHOUT ROWID;

INSERT INTO schemes (key) VALUES ('file'), ('http'), ('https'), ('memory');

-- Saved connections to the sources, with the settings every engine shares.
CREATE TABLE connections_new (
    -- Id of the connection, kept from the former table.
    id INTEGER PRIMARY KEY,
    -- Owner of the connection.
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Name chosen by the user, unique among their connections.
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    -- Engine of the source; its family names the table of the settings.
    engine TEXT NOT NULL REFERENCES engines (key),
    -- Statements run each time a connection to the source opens, separated
    -- by ";": session settings only (src/connections.rs checks them).
    init_sql TEXT NOT NULL DEFAULT '' CHECK (length(init_sql) <= 4000),
    -- Delay during which a query session stays open after its page is
    -- closed, in minutes; 0 closes it at once.
    session_timeout_minutes INTEGER NOT NULL DEFAULT 5 CHECK (session_timeout_minutes BETWEEN 0 AND 1440),
    -- Does the navigator show the system namespaces of the source? 1: yes.
    show_system_schemas INTEGER NOT NULL DEFAULT 1 CHECK (show_system_schemas IN (0, 1)),
    -- Date it was saved, in UTC, in ISO 8601 format.
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (user_id, name)
) STRICT;

INSERT INTO connections_new (id, user_id, name, engine, init_sql, session_timeout_minutes, show_system_schemas, created_at)
SELECT id, user_id, name, engine, init_sql, session_timeout_minutes, show_system_schemas, created_at
FROM connections;

-- Settings of the connections to a server reached over the network (family
-- 'server': MySQL, MariaDB). Only the password is encrypted (AES-256-GCM,
-- src/secrets.rs): the rest stays in clear, so that the connections are
-- listed without decrypting anything.
CREATE TABLE connection_servers (
    -- Connection these settings belong to.
    connection_id INTEGER PRIMARY KEY REFERENCES connections (id) ON DELETE CASCADE,
    -- Host name or IP address of the server.
    host TEXT NOT NULL CHECK (length(host) BETWEEN 1 AND 253),
    -- TCP port of the server.
    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    -- Connection account.
    username TEXT NOT NULL CHECK (length(username) BETWEEN 1 AND 80),
    -- Password of the account, encrypted with the master key.
    password_secret BLOB NOT NULL,
    -- Database selected on connection; empty string: none.
    database_name TEXT NOT NULL DEFAULT '' CHECK (length(database_name) <= 64)
) STRICT;

INSERT INTO connection_servers (connection_id, host, port, username, password_secret, database_name)
SELECT id, host, port, username, password_secret, database_name
FROM connections;

-- Databases of the connections held in files (family 'files': SQLite), one
-- row per database: the main one (`main`) at position 0, then the databases
-- attached to it, in their order of attachment.
CREATE TABLE connection_files (
    -- Connection the database belongs to.
    connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
    -- Rank of the database in the connection: 0 for `main`, then the order
    -- of attachment.
    position INTEGER NOT NULL CHECK (position >= 0),
    -- Name of the schema of the database in the sessions (`main`, then the
    -- name of the `ATTACH`), unique in the connection whatever its case;
    -- `temp` belongs to the engine.
    schema_name TEXT NOT NULL COLLATE NOCASE CHECK (length(schema_name) BETWEEN 1 AND 64 AND schema_name <> 'temp' AND (schema_name = 'main') = (position = 0)),
    -- Protocol of the location.
    scheme TEXT NOT NULL REFERENCES schemes (key),
    -- Location of the database in its protocol: absolute path for 'file',
    -- URL for 'http' and 'https', empty string for 'memory'.
    location TEXT NOT NULL DEFAULT '' CHECK (length(location) <= 2048 AND (location = '') = (scheme = 'memory')),
    PRIMARY KEY (connection_id, position),
    UNIQUE (connection_id, schema_name)
) STRICT, WITHOUT ROWID;

-- Last successful connection to the source of each connection: what it
-- detected, shown by the navigator and the connections manager before the
-- catalog is read. A connection that never succeeded has no row. A
-- connection whose flavor, version or date was not saved yet gets its row
-- at its next successful connection.
CREATE TABLE connection_probes (
    -- Connection probed.
    connection_id INTEGER PRIMARY KEY REFERENCES connections (id) ON DELETE CASCADE,
    -- Flavor of the source.
    flavor TEXT NOT NULL REFERENCES flavors (key),
    -- Version of the source, as it gives it (`SELECT VERSION()` for MySQL).
    version TEXT NOT NULL CHECK (length(version) BETWEEN 1 AND 100),
    -- Date of the connection, in UTC, in ISO 8601 format.
    connected_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

INSERT INTO connection_probes (connection_id, flavor, version, connected_at)
SELECT id, server_flavor, substr(server_version, 1, 100), connected_at
FROM connections
WHERE server_flavor <> '' AND server_version <> '' AND connected_at <> '';

DROP TABLE connections;
ALTER TABLE connections_new RENAME TO connections;
