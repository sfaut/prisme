-- Adds the run_exact action ("Exécuter précisément": the selected text
-- only) to the history of the queries. SQLite cannot change a CHECK
-- constraint: the table is rebuilt, with its rows and its indexes.
CREATE TABLE query_history_new (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN (
        'session', 'run', 'run_all', 'run_exact', 'refresh', 'use', 'cancel', 'explorer', 'probe'
    )),
    connection_name TEXT NOT NULL DEFAULT '' CHECK (length(connection_name) <= 100),
    sql TEXT NOT NULL,
    duration_us INTEGER NOT NULL DEFAULT 0 CHECK (duration_us >= 0),
    transfer_us INTEGER NOT NULL DEFAULT 0 CHECK (transfer_us >= 0),
    returns_rows INTEGER NOT NULL DEFAULT 0 CHECK (returns_rows IN (0, 1)),
    row_count INTEGER NOT NULL DEFAULT 0 CHECK (row_count >= 0),
    affected_rows INTEGER NOT NULL DEFAULT 0 CHECK (affected_rows >= 0),
    truncated INTEGER NOT NULL DEFAULT 0 CHECK (truncated IN (0, 1)),
    error TEXT NOT NULL DEFAULT ''
) STRICT;

INSERT INTO query_history_new SELECT * FROM query_history;
DROP TABLE query_history;
ALTER TABLE query_history_new RENAME TO query_history;

CREATE INDEX query_history_user ON query_history (user_id, id);
CREATE INDEX query_history_started ON query_history (started_at);
