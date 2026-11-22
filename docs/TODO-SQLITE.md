# SQLite: progress

Steps of the work on SQLite as a source, and on the engine contracts it is the first to fill after MySQL. Each step is validated with the maintainer before it starts; its boxes are ticked as the work is done (⬜ to do, ✅ done), and the maintainer validates its result before the next step starts. Mockups of the structure of the interface (their style is not Prisme's): design canvas "Connexions SQLite", https://claude.ai/artifact/6S8PeMpF5p7ESik8bHpbM7.

## Decisions

Functional:

1. A database is designated by a location whose protocol is explicit: `file` (a bare absolute path, with `\` or `/`, or a `file:///` URL), public `http` / `https` (no authentication), `memory` (`:memory:`). `s3`, `ftp` and others come later through the same contract. Any file the process of Prisme can reach.
2. The protocol of each database shows in the interface: settings, check, list and card of the connections, navigator.
3. Writes: the same switches as MySQL; the files open read-only while **Sessions en lecture seule** is on. A remote database (`http`, `https`) is always read-only.
4. `PRAGMA` without a value is a read; `PRAGMA` with an assignment is refused; the `pragma_*()` functions stay possible in a `SELECT`.
5. New database: the empty file is created when the connection is saved, refused if it exists; a session then only opens existing files. Only for `file`.
6. Settings in two sections: the **main database** (`main`: existing file, new file or in memory), then the **attached databases** (0 to n: schema, location and "create the file" box, or `:memory:`). Every database of the connection is attached at each opening of a session.
7. Step 3 of a creation, **Vérification**, database by database: correct (size, journal, read-only), to create, in memory, not found ("create it" / "remove"). A database not found does not prevent saving; the session opens without it and says so.
8. Detaching: in the navigator, **Détacher pour la session** (`DETACH`, back at the next session) and **Retirer de la connexion…** (opens the connections manager); in the manager, removal for good, the line struck through and restorable until saved, with the warning of the sessions closed and transactions rolled back; the file stays on the disk.
9. Attaching from the navigator: "⋮" menu of the source, window **Attacher une base** (origin, schema, location), for this session only or in the connection (attached at once, without closing the session).
10. `ATTACH` / `DETACH` written in SQL: allowed for the session under an `ATTACH / DETACH` switch.
11. `VACUUM`, `REINDEX`, `ANALYZE` under a **Maintenance** switch; `VACUUM INTO` always refused.
12. A database in memory has **Enregistrer dans un fichier…** (SQLite's backup API), an existing file refused.
13. The interface is coded with the `frontend-design` skill, in Prisme's theme and rules.
14. Numbers are shown in one normalized writing, whatever the engine: plain decimal, roughly `[+-]?[0-9]*\.?[0-9]+`, never in scientific notation. A floating-point number is written with the fewest digits that read back the same value (`100`, `100000000000000000000`, `0.0000001`, `3.141592653589793`). MySQL and MariaDB send their `DOUBLE`s in scientific notation beyond some sizes (`1e20`, `1.5e300`, `1.2345678901234568e17`, measured on 8.0.11, the latest MySQL, 10.6.3 and the latest MariaDB on 2026-11-22), and SQLite writes `1.0e+20`, `100.0`: the connectors normalize them.
15. Remote locations (`http`, `https`): 10 s to connect, 5 min for a whole download, 1 GiB at most per download, 5 redirects at most and never from `https` to `http`; private addresses (`127.0.0.1`, local network) accepted. Prisme keeps the copy of a remote database in its data folder (`cache/locations/`), opens it read-only and downloads it again only when the server says it changed (`ETag` / `Last-Modified`); a copy no session opened for 30 days is deleted. The timeouts, the largest download and the delay of the purge are preferences of each user, saved in the internal database.

Architecture:

16. Connectors through **traits and a registry**, no dispatch enum.
17. Settings of an engine in **columns and tables** of the internal database, with their `CHECK`s; reference tables `engines`, `flavors`, `schemes`.
18. Catalog of the navigator in **namespaces of variable depth** and **open kinds of objects**, labels given by the engine.

## Contracts

- **Connector** (`crates/connectors`, module `api`): `Engine` (family of a driver) and `Flavor` (product detected); `Driver` (`probe`, `open`), `Session` (`execute`, transaction state, namespace, attach / detach, notices, catalog, canceller), `CatalogReader` (`namespaces`, `objects`, `details`, through the connection of the session), `Rows` (columns, rows read one at a time, outcome: the server gathers them into batches by their arrival, and a read is never cut in the middle of a message of the protocol), `Canceller`, behind boxed futures; `OpenOptions { read_only, namespace, session_key }`, read-only applied by the connector; `Capabilities` sent to the interface; `SourceInfo` with `DialectOptions`; `ConnectorError` with `Unsupported`; `TypeFamily` only gains variants.
- **Catalog**: `Namespace` (path of names), `NamespaceInfo { namespace, system }`; events `namespaces { source, namespaces }`, `objects { namespace, objects }`, `details { namespace, tables }`; `ObjectInfo { name, kind, rows, enabled }`, `ObjectKind` open, each kind in a folder of the navigator.
- **Synchronous engines**: module `blocking`, one thread per connection, bounded channel of batches, interrupt handle (SQLite now, DuckDB later).
- **Locations** (module `location`): `Location { scheme, address }`, `LocationProvider` (capabilities, `probe`, `create`, `materialize`) in a registry; `FileProvider`, `HttpProvider` (`reqwest`, `rustls`); cache of the remote copies in the data folder, revalidated by `ETag` / `Last-Modified`; an engine declares the protocols it reads natively.
- **Dialect**: `DialectSpec` per dialect, `LexicalRules` read by the splitter and the editor.
- **Storage**: `engines`, `flavors`, `schemes`; `connections` with the common columns; `connection_servers`, `connection_files`, `connection_probes`.
- **Interface**: the registry of the flavors (`web/src/api/flavors.ts`, `Record<Flavor, FlavorSpec>`), components that test capabilities, never a flavor; one form per family of settings.
- **Contract suite** (`crates/connectors/tests/contract/`): generic tests over a `Fixture` per engine.
- **Checklists**: `docs/DEVELOPER.md`, sections "Adding an engine" and "Adding a protocol".

## 0. Measures

- ✅ Read-only inherited by an `ATTACH` on a connection opened read-only
- ✅ File created by an `ATTACH` of a missing path
- ✅ A `memdb` database shared by the connections of a process, and its lifetime
- ✅ Text of a `REAL` (SQLite's conversion against Rust's shortest round trip)
- ✅ `interrupt` on a running query, connection kept
- ✅ `is_autocommit` after an error inside a transaction
- ✅ `DETACH` during a transaction and during a read
- ✅ Paths with `/` and `\` on Windows
- ✅ Measures recorded in `docs/SQL-COMPARISON.md`, with their date

## 1. Connector contracts, MySQL alone

- ✅ Module `api`: traits, registry, `Capabilities`, `SourceInfo`, errors
- ✅ Neutral types moved out of `mysql::`, serialization unchanged
- ✅ Catalog in namespaces and open kinds of objects
- ✅ MySQL connector behind the traits, read-only moved into it
- ✅ Contract suite, on MySQL and MariaDB
- ✅ Server on the traits only
- ✅ Interface on the capabilities, the registry of the flavors and the general catalog
- ✅ Nothing visible changed: checks and MySQL / MariaDB walk-through on screen

## 2. Storage and locations

- ✅ Reference tables `setting_families`, `engines`, `flavors`, `schemes`
- ✅ `connections` rebuilt; `connection_servers`, `connection_files`, `connection_probes`
- ✅ API of the connections tagged by family of settings (the family `files` joins it with the SQLite connector)
- ✅ Module `location`: parsing, registry, `FileProvider`, `HttpProvider`, `MemoryProvider`, cache and its purge, tests
- ✅ Preferences of the remote locations: timeouts, largest download, delay of the purge (`users`, migration 0039; **Bases distantes** section of the preferences)
- ✅ Migration tried on a copy of the development internal database

## 3. SQLite dialect

- ✅ `DialectSpec`, `LexicalRules` (splitter and editor)
- ✅ Default limit, `LIMIT -1` counted as a limit written; a query ending with `VALUES`, which takes no `LIMIT`, read up to the extra row (`capped`)
- ⬜ Classification: `PRAGMA`, `ATTACH` / `DETACH`, maintenance, `VACUUM INTO`, triggers, `INSERT OR REPLACE`, `ON CONFLICT DO UPDATE`, `temp.`
- ⬜ `ends_transaction`, `session_setting_refusal`
- ✅ `keywords/sqlite.yaml`, checked against `sqlite3_keyword_name`

## 4. SQLite connector

- ⬜ Module `blocking`
- ⬜ `Driver`, `Session`, `CatalogReader`, `Rows`, `Canceller`
- ⬜ Attachments, remote copies read-only, databases in `memdb`
- ⬜ Backup of a database in memory
- ⬜ Catalog
- ⬜ Check database by database
- ⬜ `Fixture` SQLite in the contract suite

## 5. Server, SQLite part

- ⬜ Files created when the connection is saved
- ⬜ API of the attachments of the navigator (session or connection)
- ⬜ Saving a database in memory to a file
- ⬜ Schemas read again after `ATTACH` / `DETACH`
- ⬜ Switches `attach` and `maintenance`
- ⬜ Remote databases read within the preferences of their user (`RemoteLimits`), cache of each user in `cache/locations/<user>`, purged by `location::purge` after `remote_cache_days`

## 6. SQLite interface

- ⬜ Family of the files ready, form of the files (main database, attached databases, location field with its protocol)
- ⬜ Step **Vérification** database by database
- ⬜ Saved connection: removal struck through and restorable
- ⬜ List and card: location of `main`, number of attached databases, protocol
- ⬜ Badge SQLite, light and dark
- ⬜ Navigator: schemas, protocol, menus, window **Attacher une base**; labels of the namespace levels (`NamespaceLevel`) in the navigator and the completion
- ⬜ Editor dialect, CTE and `IN` clause in SQLite
- ⬜ Switches of the engine in the settings

## 7. Documentation

- ⬜ `docs/DEVELOPER.md`: "Adding an engine", "Adding a protocol", schema, API, tests
- ⬜ `docs/V1.md`: SQLite, sections 2.4, 2.5 and 3.2
- ⬜ `README.md`, `docs/QUESTIONS.md`

## Open points, settled at their step

- `PRAGMA` with an assignment in the SQL run at connection: allow-list of the pragmas of the connection, never those that change the file.
- `ATTACH` written in SQL whose location is not a literal: refusal proposed.
- Switch needed by **Enregistrer dans un fichier…**, and its fate in a read-only session.
- Exact form of the neutral variant of `FailureCause`, given what `recorder.rs` saved already.
