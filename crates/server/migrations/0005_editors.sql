-- Éditeurs SQL ouverts par chaque utilisateur (onglets du haut de l'écran),
-- avec leur texte, leur connexion et leurs onglets de résultat : tout
-- survit à un rafraîchissement de la page ou à la fermeture du navigateur.
--
-- connection_id : connexion de l'éditeur ; NULL tant qu'aucune n'est choisie,
-- et quand la connexion est supprimée (la clef étrangère l'exige).
-- closed_at : vide pour un éditeur ouvert ; date de fermeture sinon. Un
-- éditeur fermé reste rouvrable 24 heures, puis il est purgé (src/editors.rs).
CREATE TABLE editors (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name TEXT NOT NULL DEFAULT '' CHECK (length(name) <= 100),
    position INTEGER NOT NULL DEFAULT 0,
    connection_id INTEGER REFERENCES connections (id) ON DELETE SET NULL,
    sql TEXT NOT NULL DEFAULT '',
    closed_at TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX editors_user ON editors (user_id, closed_at);

-- Onglets de résultat d'un éditeur, enregistrés par le serveur au fil de
-- l'exécution. result : état du résultat en JSON (colonnes, durées, erreur...,
-- voir SavedResult dans src/editors.rs) ; rows : lignes reçues, en JSON.
CREATE TABLE result_tabs (
    id INTEGER PRIMARY KEY,
    editor_id INTEGER NOT NULL REFERENCES editors (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    query INTEGER NOT NULL,
    sql TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '' CHECK (length(name) <= 100),
    pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    result TEXT NOT NULL DEFAULT '{}',
    rows TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX result_tabs_editor ON result_tabs (editor_id, position);
