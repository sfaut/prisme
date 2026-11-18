-- Queries of the history stopped by the user (button "Arrêter" of a result
-- tab, of the editor's run): their failure is a stop, which the console shows
-- as a warning. The queries already saved are recognized by the message of
-- the stop.
ALTER TABLE query_history ADD COLUMN interrupted INTEGER NOT NULL DEFAULT 0 CHECK (interrupted IN (0, 1));

UPDATE query_history SET interrupted = 1 WHERE error = 'query interrupted by the user';
