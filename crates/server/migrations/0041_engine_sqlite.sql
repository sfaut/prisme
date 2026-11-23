-- Adds the engine SQLite and its flavor (Engine and Flavor,
-- crates/connectors/src/api.rs and flavor.rs): its connections are databases
-- held in files (connection_files), the main one and those attached to it.
INSERT INTO engines (key, family) VALUES ('sqlite', 'files');
INSERT INTO flavors (key, engine) VALUES ('sqlite', 'sqlite');
