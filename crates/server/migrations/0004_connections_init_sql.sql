-- Requêtes exécutées à l'ouverture de chaque connexion vers la base cible
-- (SET NAMES utf8mb4; SET SESSION sql_mode = ...), séparées par des « ; ».
-- Seuls les réglages de la session sont permis : la vérification est faite
-- par src/connections.rs, qui reprend aussi la longueur maximale.
ALTER TABLE connections ADD COLUMN init_sql TEXT NOT NULL DEFAULT '' CHECK (length(init_sql) <= 4000);
