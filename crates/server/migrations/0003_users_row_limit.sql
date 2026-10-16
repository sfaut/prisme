-- Default row limit of each user (see docs/V1.md, section 2.3).
-- The allowed values are a constant of the code (src/settings.rs): the
-- database only checks that the limit is positive, so that changing the list
-- of choices requires no migration.
ALTER TABLE users ADD COLUMN row_limit INTEGER NOT NULL DEFAULT 1000 CHECK (row_limit > 0);
