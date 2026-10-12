-- Utilisateurs de Prisme.
-- En mode poste de développement, un seul utilisateur, sans mot de passe :
-- password_hash reste vide. En mode serveur, il contiendra le hachage argon2
-- du mot de passe. Les dates sont en UTC, au format ISO 8601 à la milliseconde.
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE CHECK (length(email) BETWEEN 3 AND 254),
    password_hash TEXT NOT NULL DEFAULT '' CHECK (length(password_hash) <= 200),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
