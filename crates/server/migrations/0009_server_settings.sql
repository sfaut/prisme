-- Settings of the Prisme instance, common to all users: a single row
-- (id = 1). For now, the listening port, asked on first launch or by
-- `prisme --setup` (see src/setup.rs); the address stays 127.0.0.1. A database
-- created before this setting keeps port 8484, its previous setting.
CREATE TABLE server_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    listen_port INTEGER NOT NULL DEFAULT 8484 CHECK (listen_port BETWEEN 1 AND 65535)
) STRICT;

INSERT INTO server_settings (id) VALUES (1);
