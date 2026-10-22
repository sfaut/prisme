-- Writes allowed to the query sessions of a user (see docs/V1.md, section
-- 2.4): one row per switch turned on in the settings, none by default.
-- Read-only sessions (users.read_only_session) refuse every write, whatever
-- is allowed here. The kinds are those of WriteKind (crates/dialects/src/classify.rs).
CREATE TABLE user_allowed_writes (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN (
        'insert', 'update', 'delete',
        'create_table', 'alter_table', 'rename_table', 'truncate_table', 'drop_table',
        'create_view', 'drop_view',
        'create_function', 'drop_function',
        'create_procedure', 'drop_procedure',
        'create_event', 'alter_event', 'drop_event'
    )),
    PRIMARY KEY (user_id, kind)
) STRICT, WITHOUT ROWID;
