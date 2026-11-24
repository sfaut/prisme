-- Options of the connections held in files (family 'files': SQLite) that
-- concern the connection as a whole, one row per connection; a connection
-- without a row has the defaults.
CREATE TABLE connection_file_options (
    -- Connection the options belong to.
    connection_id INTEGER PRIMARY KEY REFERENCES connections (id) ON DELETE CASCADE,
    -- Does each session refresh the statistics of the query planner of its
    -- files when it opens (PRAGMA optimize)? 1: yes, 0: no (default).
    optimize INTEGER NOT NULL DEFAULT 0 CHECK (optimize IN (0, 1))
) STRICT;
