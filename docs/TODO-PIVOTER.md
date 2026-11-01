# Pivoteur: progress

Steps of the work on the Pivoteur, the local pivot table under the result grid. Each step is validated with the maintainer before it starts; its boxes are ticked as the work is done (⬜ to do, ✅ done), and the maintainer validates its result before the next step starts. Specification: "Cahier des charges du Pivoteur de Prisme" (Claude Docs, in French), https://claude.ai/artifact/UvfFRfmTaFLZ1d7kDPkDSD.

## 1. Review of the specification

- ✅ Read the specification and check its references against the code
- ✅ Settled: a dialect of the Pivoteur's own, close to MySQL, MariaDB and the others; the Pivoteur works on the result set as it is; the panel stays under the grid; the federated engine is set aside for now
- ✅ Panel in three columns: fields, stacked zones, pivot table

## 2. Benchmarks: engine (point A1), store of the results

- ✅ Data generator shared by Rust and JavaScript (same pseudo-random generator, fixed seed)
- ✅ Rust benchmark, on one thread and on all cores
- ✅ JavaScript benchmark under Node (V8)
- ✅ Same checksums on both sides
- ✅ One command reproduces it all, from the root of the repository: `node bench/pivot/run.mjs` (see `docs/DEVELOPER.md`, section "Benchmarks")
- ✅ Runs on 10 million rows, both profiles, with memory
- ✅ Results submitted to the maintainer
- ✅ Rust compiled to WebAssembly (`wasm32-unknown-unknown`), run by V8 in the page: no transfer of the pivot, one copy of the cells into the memory of WebAssembly
- ✅ Results with WebAssembly submitted to the maintainer
- ✅ Saved results measured on the real server, 1 and 10 million rows: reception, save, reading again by the page (`node bench/reload/run.mjs`)
- ✅ Engine decided (A1): Rust compiled to WebAssembly, run in a Worker of the page, on the rows the page already holds; a server and a client of Prisme are expected in the long run, the client doing this work
- ✅ Reception of a result broken down: database, server, JSON, page, interface (`node bench/store/run.mjs`, `node bench/store/trace.mjs`)
- ✅ Round trip of the visible rows between the page and a Worker, idle and during the reception
- ✅ Results of the store submitted to the maintainer

## 3. Corrections of the specification

- ✅ Single store of the results (`docs/QUESTIONS.md`, section 2): the lot 1 builds it in the Worker, fed by a copy of the rows, as the future single store
- ✅ Expression language: the dialect of the Pivoteur, ANSI SQL first, the rest borrowed from MySQL, MariaDB, PostgreSQL, DuckDB, ClickHouse or another engine, with functions of its own; parsed by `sqlparser` with a dialect of Prisme; conversions (`CONCAT`, date literals), invalid data
- ✅ Behaviours of the engines measured on their servers or sourced, in a reference kept up to date: `docs/SQL-COMPARISON.md` (`node bench/sql/run.mjs`)
- ✅ New section on the cases that diverge from one database to another (D1 to D31), each with a recommendation to decide
- ✅ Catalog: ANSI functions, then the others with their origin; `CAST(x AS DATE)`, `CAST(x AS TIMESTAMP)`
- ✅ Conformance: reference file, binary collations without padding on the servers, functions absent from MySQL or MariaDB
- ✅ `sqlparser` compiled to WebAssembly measured: size of the module, time of a parse (`node bench/parser/run.mjs`, `docs/PERFORMANCES.md`, chapter 13)
- ✅ A1 with the benchmark, and the heading of the points to decide
- ✅ Mockup of the panel in three columns, search in the field list (F-10)
- ✅ Minor fixes: "clefs" (C-03), N-03, TypeScript block in English, checks and expected outputs of the acceptance, N-14, sources
- ✅ Cases D1 to D35 decided by the maintainer
- ✅ Decisions applied throughout the specification, typed literals written without a space (`DATE'2026-01-15'`)
- ✅ Dialect refocused: ANSI SQL over the rows of the grid for the lot 1, its extensions in the next lots; each lot types what it uses
- ⬜ Cases D1 to D35 reviewed, the behaviour of ANSI SQL first

## 4. Lots of the specification

### First slice: panel with `COUNT` and `SUM`

- ✅ Crates `store`, `pivot` and `web-engine`, taken from the engine of the benchmark: rows in columns, groups through a hash table, totals by merging the states
- ✅ Dimension: a column of the grid, its value as it is, NULL in a group of its own, its values ranked by the page in the order of the grid (C-11)
- ✅ Measures `COUNT(*)`, `COUNT(col)` and `SUM(col)` on the families `integer`, `decimal` (exact) and `float` (`DOUBLE`); NULL ignored, `SUM` without a value gives NULL, an overflow is an error
- ✅ Worker of the page, fed by a copy of the rows
- ✅ Panel under the grid of `QueryCell.vue`, in three columns: fields with their type icon, zones Rows, Columns and Measures, windowed table with its totals
- ✅ A field dropped into the measures gives `SUM` for a number, `COUNT` otherwise; a menu changes the measure
- ✅ Checks: `cargo test`, tests of the interface, screen in light and dark modes

### Next slices and lots

- ⬜ Lot 1, next slices (ANSI SQL): dialect (parsed by `sqlparser`, typing, ANSI catalog), calendar, editor, date levels, computed columns, filters, fold, caches, copy, CSV, conformance
- ⬜ Lot 2: complements (advanced sorts, detail of a cell, filter by list, measures in rows, persistence with the tab)
- ⬜ Lot 3: extensions (overflow syntax and functions of MySQL, MariaDB, PostgreSQL, DuckDB and ClickHouse, functions of Prisme's own, then a "global" Prisme dialect)
