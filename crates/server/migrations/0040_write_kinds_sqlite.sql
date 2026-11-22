-- Adds the write switches of SQLite (WriteKind, crates/dialects/src/classify.rs;
-- docs/TODO-SQLITE.md, decisions 10 and 11): ATTACH and DETACH for the
-- session, and the maintenance of a file (VACUUM, REINDEX, ANALYZE, the
-- pragmas that write in it).
INSERT INTO write_kinds (key) VALUES ('attach'), ('maintenance');
