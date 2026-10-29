-- Exports of a full result to a CSV file, written by the server into the
-- exports folder of the data folder (src/exports.rs). The list survives a
-- restart; a file is only deleted from the export manager.
--
-- connection_name: name of the connection, copied: the export stays listed
-- when its connection is deleted.
-- name: name of the file offered on download.
-- file: name of the file in the exports folder, `<id>-<instant in ms>.csv`,
-- set at creation.
-- state: running, ready, failed, cancelled, or interrupted (Prisme stopped
-- during the export). row_count and byte_count are written at the end.
-- expected_rows: rows announced by the interface before the export, 0 when
-- unknown.
-- error: failure, as the interface receives it (ErrorDetail in JSON), empty
-- otherwise.
-- ended_at: empty while the export runs.
CREATE TABLE exports (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    connection_name TEXT NOT NULL DEFAULT '' CHECK (length(connection_name) <= 100),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 200),
    sql TEXT NOT NULL,
    file TEXT NOT NULL DEFAULT '' CHECK (length(file) <= 50),
    state TEXT NOT NULL DEFAULT 'running' CHECK (state IN (
        'running', 'ready', 'failed', 'cancelled', 'interrupted'
    )),
    row_count INTEGER NOT NULL DEFAULT 0 CHECK (row_count >= 0),
    byte_count INTEGER NOT NULL DEFAULT 0 CHECK (byte_count >= 0),
    expected_rows INTEGER NOT NULL DEFAULT 0 CHECK (expected_rows >= 0),
    error TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ended_at TEXT NOT NULL DEFAULT ''
) STRICT;

CREATE INDEX exports_user ON exports (user_id, id);
