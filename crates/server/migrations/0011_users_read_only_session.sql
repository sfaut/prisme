-- Read-only sessions (see docs/V1.md, section 2.4): on by default, each
-- connection of a query session is put in READ ONLY mode by the database, so
-- that it refuses the writes even if the query check missed one.
ALTER TABLE users ADD COLUMN read_only_session INTEGER NOT NULL DEFAULT 1 CHECK (read_only_session IN (0, 1));
