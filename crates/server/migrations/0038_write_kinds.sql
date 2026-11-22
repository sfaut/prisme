-- Kinds of writes become a reference table (docs/TODO-SQLITE.md, step 2): a
-- new switch (`attach`, `maintenance` for SQLite) joins by an INSERT of its
-- migration, without rebuilding `user_allowed_writes` as migration 0019
-- had to. The migration runs with the foreign keys off (src/store.rs) and
-- checks them before its commit.

-- Kinds of writes a switch of the settings allows (WriteKind,
-- crates/dialects/src/classify.rs).
CREATE TABLE write_kinds (
    -- Key of the kind, as serialized ('create_table').
    key TEXT PRIMARY KEY CHECK (length(key) BETWEEN 1 AND 40)
) STRICT, WITHOUT ROWID;

INSERT INTO write_kinds (key) VALUES
    ('insert'), ('update'), ('delete'), ('truncate_table'),
    ('create_table'), ('alter_table'), ('rename_table'), ('drop_table'),
    ('create_temporary_table'), ('alter_temporary_table'), ('rename_temporary_table'),
    ('drop_temporary_table'),
    ('create_view'), ('drop_view'),
    ('create_function'), ('drop_function'),
    ('create_procedure'), ('drop_procedure'),
    ('create_event'), ('alter_event'), ('drop_event');

-- Writes allowed to the query sessions of a user (docs/V1.md, section 2.4):
-- one row per switch turned on in the settings, none by default. Read-only
-- sessions (users.read_only_session) refuse every write, whatever is
-- allowed here.
CREATE TABLE user_allowed_writes_new (
    -- User the switch belongs to.
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Kind of write the switch allows.
    kind TEXT NOT NULL REFERENCES write_kinds (key),
    PRIMARY KEY (user_id, kind)
) STRICT, WITHOUT ROWID;

INSERT INTO user_allowed_writes_new (user_id, kind) SELECT user_id, kind FROM user_allowed_writes;
DROP TABLE user_allowed_writes;
ALTER TABLE user_allowed_writes_new RENAME TO user_allowed_writes;
