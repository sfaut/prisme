-- Name of each user, shown in the profile. Empty until the user fills them in,
-- so that the columns need no nullable value.
ALTER TABLE users ADD COLUMN last_name TEXT NOT NULL DEFAULT '' CHECK (length(last_name) <= 100);
ALTER TABLE users ADD COLUMN first_name TEXT NOT NULL DEFAULT '' CHECK (length(first_name) <= 100);
