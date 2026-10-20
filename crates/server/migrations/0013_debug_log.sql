-- Debug log shown in the console of the interface (src/console/debug.rs):
-- the lines of the server's log, those the terminal shows, and the
-- notifications of the interface. It is the server's log: no user column,
-- the single user of the workstation mode sees all of it.
--
-- target: module that wrote the line (prisme::api), empty for the
-- interface.
CREATE TABLE debug_log (
    id INTEGER PRIMARY KEY,
    logged_at TEXT NOT NULL,
    level TEXT NOT NULL CHECK (level IN ('error', 'warn', 'info', 'debug', 'trace')),
    source TEXT NOT NULL CHECK (source IN ('server', 'interface')),
    target TEXT NOT NULL DEFAULT '' CHECK (length(target) <= 200),
    message TEXT NOT NULL CHECK (length(message) <= 2000)
) STRICT;

CREATE INDEX debug_log_logged ON debug_log (logged_at);
