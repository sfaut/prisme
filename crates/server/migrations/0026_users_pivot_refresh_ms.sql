-- Shortest delay between the starts of two computations of the table of the
-- Pivoteur while the rows of a result are being received, in milliseconds,
-- set in the settings.
ALTER TABLE users ADD COLUMN pivot_refresh_ms INTEGER NOT NULL DEFAULT 1000 CHECK (pivot_refresh_ms BETWEEN 200 AND 60000);
