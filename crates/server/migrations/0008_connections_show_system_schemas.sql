-- L'explorateur affiche-t-il les bases système de la connexion
-- (information_schema, mysql, performance_schema, sys) ? 1 : oui (comme
-- avant ce réglage), 0 : non.
ALTER TABLE connections ADD COLUMN show_system_schemas INTEGER NOT NULL DEFAULT 1 CHECK (show_system_schemas IN (0, 1));
