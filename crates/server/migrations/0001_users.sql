-- Users of Prisme.
-- In workstation mode, a single user, without a password: password_hash
-- stays empty. In server mode, it will contain the argon2 hash of the
-- password. Dates are in UTC, in ISO 8601 format to the millisecond.
CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE CHECK (length(email) BETWEEN 3 AND 254),
    password_hash TEXT NOT NULL DEFAULT '' CHECK (length(password_hash) <= 200),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
