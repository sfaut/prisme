-- Variante du serveur de la connexion (mysql ou mariadb), détectée à la
-- dernière connexion réussie : l'explorateur l'affiche en badge à côté de
-- la source, même avant d'ouvrir son catalogue. Chaîne vide tant que la
-- connexion n'a jamais abouti.
ALTER TABLE connections ADD COLUMN server_flavor TEXT NOT NULL DEFAULT '' CHECK (server_flavor IN ('', 'mysql', 'mariadb'));
