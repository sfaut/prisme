-- Délai pendant lequel une session de requêtes reste ouverte après la
-- fermeture de sa page (rafraîchissement, navigateur fermé), en minutes :
-- la page qui revient s'y raccroche, transactions et requêtes en cours
-- comprises. 0 ferme la session dès la déconnexion. La vérification est
-- faite par src/connections.rs, qui reprend le même maximum (24 heures).
ALTER TABLE connections ADD COLUMN session_timeout_minutes INTEGER NOT NULL DEFAULT 5 CHECK (session_timeout_minutes BETWEEN 0 AND 1440);
