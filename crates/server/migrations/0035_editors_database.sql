-- Last current database of each editor's query session (`USE`, or the one
-- it opened on), saved by the session as it changes (src/recorder.rs). A
-- session that opens without a database chosen (reconnection, page
-- reloaded after the session expired) opens on it. Empty string: the
-- database of the connection. Emptied when the editor changes connection,
-- and when the database no longer opens (dropped, access withdrawn).
ALTER TABLE editors ADD COLUMN database_name TEXT NOT NULL DEFAULT '';
