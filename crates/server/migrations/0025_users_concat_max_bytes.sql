-- Longest text of a GROUP_CONCAT of the Pivoteur, in bytes of UTF-8, set in the
-- settings: beyond, the text keeps the whole values that hold in it, then `…`
-- and the number of values left out.
ALTER TABLE users ADD COLUMN concat_max_bytes INTEGER NOT NULL DEFAULT 1000 CHECK (concat_max_bytes BETWEEN 100 AND 1048576);
