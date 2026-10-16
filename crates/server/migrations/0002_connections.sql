-- Saved connections to the target databases.
-- Only the password is encrypted (AES-256-GCM, see src/secrets.rs): the rest
-- is in clear, so that the explorer lists the connections without decrypting
-- anything.
-- engine: family of engines. 'mysql' covers MySQL and MariaDB, whose flavor
-- is detected on connection. The maximum lengths are repeated in the
-- validation of src/connections.rs.
CREATE TABLE connections (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 100),
    engine TEXT NOT NULL CHECK (engine IN ('mysql')),
    host TEXT NOT NULL CHECK (length(host) BETWEEN 1 AND 253),
    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    username TEXT NOT NULL CHECK (length(username) BETWEEN 1 AND 80),
    password_secret BLOB NOT NULL,
    database_name TEXT NOT NULL DEFAULT '' CHECK (length(database_name) <= 64),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    UNIQUE (user_id, name)
) STRICT;
