-- Adds the write switches of the temporary tables (WriteKind,
-- crates/dialects/src/classify.rs). SQLite cannot change a CHECK
-- constraint: the table is rebuilt, with its rows. Until now, a switch of
-- the tables also covered the temporary tables: each user keeps that right,
-- under the matching switch of the temporary tables. The data of the
-- temporary tables stays under the switches of the data, `TRUNCATE`
-- included.
CREATE TABLE user_allowed_writes_new (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN (
        'insert', 'update', 'delete', 'truncate_table',
        'create_table', 'alter_table', 'rename_table', 'drop_table',
        'create_temporary_table', 'alter_temporary_table', 'rename_temporary_table',
        'drop_temporary_table',
        'create_view', 'drop_view',
        'create_function', 'drop_function',
        'create_procedure', 'drop_procedure',
        'create_event', 'alter_event', 'drop_event'
    )),
    PRIMARY KEY (user_id, kind)
) STRICT, WITHOUT ROWID;

INSERT INTO user_allowed_writes_new SELECT user_id, kind FROM user_allowed_writes;
INSERT INTO user_allowed_writes_new (user_id, kind)
SELECT user_id, replace(kind, '_table', '_temporary_table')
FROM user_allowed_writes
WHERE kind IN ('create_table', 'alter_table', 'rename_table', 'drop_table');
DROP TABLE user_allowed_writes;
ALTER TABLE user_allowed_writes_new RENAME TO user_allowed_writes;
