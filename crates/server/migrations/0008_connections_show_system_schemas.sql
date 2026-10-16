-- Does the explorer show the connection's system databases
-- (information_schema, mysql, performance_schema, sys)? 1: yes (as before
-- this setting), 0: no.
ALTER TABLE connections ADD COLUMN show_system_schemas INTEGER NOT NULL DEFAULT 1 CHECK (show_system_schemas IN (0, 1));
