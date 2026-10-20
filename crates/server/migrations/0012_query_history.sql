-- History of the queries sent to the target servers, shown in the console
-- of the interface (src/console/history.rs): those of the editors and those
-- Prisme sends itself (session opening, explorer, connection test,
-- interruptions).
--
-- connection_name: name of the connection when the query was sent; a text
-- and no foreign key, so that the history outlives the connection.
-- duration_us: execution time, until the result or, for a result read in
-- streaming, until its columns. transfer_us: transfer time of the rows of
-- a result read in streaming, 0 when not measured.
-- error: failure message, empty on success.
CREATE TABLE query_history (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    action TEXT NOT NULL CHECK (action IN (
        'session', 'run', 'run_all', 'refresh', 'use', 'cancel', 'explorer', 'probe'
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

CREATE INDEX query_history_user ON query_history (user_id, id);
CREATE INDEX query_history_started ON query_history (started_at);
