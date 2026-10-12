-- Limite de lignes par défaut de chaque utilisateur (voir docs/V1.md, section 2.3).
-- Les valeurs permises sont une constante du code (src/settings.rs) : la base
-- vérifie seulement que la limite est positive, pour que changer la liste des
-- choix ne demande pas de migration.
ALTER TABLE users ADD COLUMN row_limit INTEGER NOT NULL DEFAULT 1000 CHECK (row_limit > 0);
