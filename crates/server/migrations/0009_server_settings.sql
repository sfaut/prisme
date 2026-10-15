-- Réglages de l'instance de Prisme, communs à tous les utilisateurs : une
-- seule ligne (id = 1). Pour l'instant, le port d'écoute, demandé au premier
-- lancement ou par `prisme --setup` (voir src/setup.rs) ; l'adresse reste
-- 127.0.0.1. Une base créée avant ce réglage garde le port 8484, son réglage
-- d'avant.
CREATE TABLE server_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    listen_port INTEGER NOT NULL DEFAULT 8484 CHECK (listen_port BETWEEN 1 AND 65535)
) STRICT;

INSERT INTO server_settings (id) VALUES (1);
