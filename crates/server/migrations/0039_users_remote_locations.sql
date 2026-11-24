-- Preferences of the remote locations (http, https) of the databases held
-- in files (docs/TODO-SQLITE.md, decision 15), set in the settings. The
-- bounds are repeated in the validation of src/settings.rs.

-- Longest wait for the connection to a web server, in seconds.
ALTER TABLE users ADD COLUMN remote_connect_timeout_seconds INTEGER NOT NULL DEFAULT 10 CHECK (remote_connect_timeout_seconds BETWEEN 1 AND 300);

-- Longest wait for a whole download, in minutes.
ALTER TABLE users ADD COLUMN remote_download_timeout_minutes INTEGER NOT NULL DEFAULT 5 CHECK (remote_download_timeout_minutes BETWEEN 1 AND 1440);

-- Largest remote file downloaded, in mebibytes.
ALTER TABLE users ADD COLUMN remote_max_mebibytes INTEGER NOT NULL DEFAULT 1024 CHECK (remote_max_mebibytes BETWEEN 1 AND 102400);

-- Days after which a copy no session opened is deleted from the cache.
ALTER TABLE users ADD COLUMN remote_cache_days INTEGER NOT NULL DEFAULT 30 CHECK (remote_cache_days BETWEEN 1 AND 3650);
