-- SQL editors opened by each user (tabs at the top of the screen), with
-- their text, their connection and their result tabs: everything survives a
-- page reload or the browser being closed.
--
-- connection_id: connection of the editor; NULL as long as none is chosen,
-- and when the connection is deleted (the foreign key requires it).
-- closed_at: empty for an open editor; closing date otherwise. A closed
-- editor stays reopenable for 24 hours, then it is purged (src/editors.rs).
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

-- Result tabs of an editor, saved by the server as the run goes.
-- result: state of the result in JSON (columns, durations, error..., see
-- SavedResult in src/editors.rs); rows: rows received, in JSON.
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
