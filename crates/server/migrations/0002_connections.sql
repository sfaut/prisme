-- Connexions enregistrées vers les bases cibles.
-- Seul le mot de passe est chiffré (AES-256-GCM, voir src/secrets.rs) : le reste
-- est en clair, pour que l'explorateur liste les connexions sans rien déchiffrer.
-- engine : famille de moteurs. 'mysql' couvre MySQL et MariaDB, dont la variante
-- est détectée à la connexion. Les longueurs maximales sont reprises dans la
-- validation de src/connections.rs.
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
