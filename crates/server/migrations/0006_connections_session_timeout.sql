-- Delay during which a query session stays open after its page is closed
-- (reload, browser closed), in minutes: the page that comes back reattaches
-- to it, transactions and running queries included. 0 closes the session as
-- soon as it disconnects. The check is done by src/connections.rs, which
-- repeats the same maximum (24 hours).
ALTER TABLE connections ADD COLUMN session_timeout_minutes INTEGER NOT NULL DEFAULT 5 CHECK (session_timeout_minutes BETWEEN 0 AND 1440);
