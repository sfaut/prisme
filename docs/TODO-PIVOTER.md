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
- ✅ Cases D1 to D35 reviewed, the behaviour of ANSI SQL first: `||` and `LISTAGG` take texts, a date compares with a date only, `GREATEST` and `LEAST` give NULL on NULL, `UPPER('ß')` gives `SS`, an end of month that does not exist stops the computation; a date without a day in the calendar stops the computation under a precision (A7)
- ✅ Departures from ANSI SQL kept in `CAST`: `VARCHAR` and `CHAR`, its synonym, take no length and give the whole text, `SUBSTRING` or `LEFT` cutting it (D33); a date gives `2026-01-15`, as every engine measured (D36)

## 4. Lots of the specification

### First slice: panel with `COUNT` and `SUM`

- ✅ Crates `store`, `pivot` and `web-engine`, taken from the engine of the benchmark: rows in columns, groups through a hash table, totals by merging the states
- ✅ Dimension: a column of the grid, its value as it is, NULL in a group of its own, its values ranked by the page in the order of the grid (C-11)
- ✅ Measures `COUNT(*)`, `COUNT(col)` and `SUM(col)` on the families `integer`, `decimal` (exact) and `float` (`DOUBLE`); NULL ignored, `SUM` without a value gives NULL, an overflow is an error
- ✅ Worker of the page, fed by a copy of the rows
- ✅ Panel under the grid of `QueryCell.vue`, in three columns: fields with their type icon, zones Rows, Columns and Measures, windowed table with its totals
- ✅ A field dropped into the measures gives `SUM` for a number, `COUNT` otherwise; a menu changes the measure
- ✅ Checks: `cargo test`, tests of the interface, screen in light and dark modes
- ✅ Measure `COUNT(DISTINCT col)` on any column: its distinct values as the levels group them, NULL left out; a total counts those of its whole group
- ✅ A measure, a level of the rows and a level of the columns take a name, shown in their zone and in the table (F-12, F-17): the levels in the corner, those of the columns on the row of their headers
- ✅ Checks of these additions: `cargo test`, tests of the interface, screen in light and dark modes
- ✅ Measures `SUM(DISTINCT col)`, `AVG(col)` and `AVG(DISTINCT col)` on numbers, `MIN(col)` and `MAX(col)` on any column: numbers told apart and compared by value, an exact average at the scale of its column + 4, rounded away from zero (D16, D17), texts compared by code points (D7), a total reading the values of its whole group
- ✅ Checks of these measures: `cargo test`, tests of the interface, screen in light and dark modes
- ✅ The editor, the results, the Pivoteur and the console can all be folded at once; folding a panel leaves the others as they are
- ✅ The configuration is saved with its tab: it survives a page reload and a refresh; a new run gives each new tab that of the tab of the same text, else of the same rank; an element follows its column by name, the element of a lost column leaves, `COUNT(*)` stays
- ✅ A column of a primary key dropped into the measures gives `COUNT(*)`, a column of a foreign key `COUNT(col)`: the metadata of the result name the column of a table it reads (`docs/SQL-COMPARISON.md`, section 3.15), and the catalog tells its foreign keys
- ✅ A dragged element frames in blue every zone it passes over; released outside the zones, it leaves its zone, in place of the entries "Retirer", "Monter" and "Descendre" of its menu; the aggregates of a measure read counts, sums, averages, extremes
- ✅ The menu of a level of a `date` or `datetime` column sets its precision: ISO year (`2026`), quarter (`2026-T2`), month (`2026-01`), ISO week (`2026-S53`), date (`2026-01-15`). The engine groups the dates, in the order of time; a date without a day in the calendar keeps its fields, and gives NULL for the ISO year and week; a cell without a date stops the computation. A column that holds no dates any more gives its values as they are
- ✅ Checks of the precision: `cargo test`, tests of the interface, screen in light and dark modes, on a `datetime`, a `date` and a `time` column; the message of a cell without a date is not reachable yet, the connectors of MySQL and MariaDB always writing a date
- ✅ First slice documented: `README.md`, `docs/DEVELOPER.md` (section "Pivoteur") and the specification (requirements, section "Lots", acceptance)
- ✅ The forbidden cursor no longer blinks over the zones while a field is dragged: `dragenter` is accepted as `dragover` is, and the room between two zones belongs to the upper one

### Sort of the table

- ✅ The name of a level, in the corner, sorts its values, ascending or descending, and gives its axis back to its labels when values rank it (C-11)
- ✅ The header of a column ranks the lines by its values, a total included, the label of a line ranks the columns by its own, by their first measure: ascending, descending, none (C-13). The hierarchy stays, a total stays last, a tie keeps the order of the labels, an empty cell ranks like NULL
- ✅ A sort computes no aggregate: the same request lays the table out again, without a blank between two windows
- ✅ The sorts are saved with the tab, and last as long as the levels of the other axis
- ✅ Checks: `cargo test`, tests of the interface, screen in light and dark modes, reload and new run

### Next slices and lots

- ⬜ A date without a day in the calendar stops the computation under a precision (A7, D25), with the value and the number of its row; `crates/pivot/src/dates.rs`, `README.md` and `docs/DEVELOPER.md` follow
- ⬜ Filters of lot 1, decided in the specification (F-19, F-21, F-44): a source column by the list of its values with a search, a measure at a level chosen in the filter's menu (by default the deepest of the rows), an expression. Totals count the groups that remain, unlike `WITH ROLLUP … HAVING` (measured on MariaDB 12.3.3 and MySQL 9.7.4, to record in `bench/sql/cases.mjs` and `docs/SQL-COMPARISON.md` with this slice)
- ⬜ Lot 1, next slices (ANSI SQL): dialect (parsed by `sqlparser`, typing, ANSI catalog), calendar, editor, nested date levels, computed columns, filters, fold, caches, copy, CSV, conformance
- ⬜ Lot 2: complements (detail of a cell, measures in rows)
- ⬜ Lot 3: extensions (overflow syntax and functions of MySQL, MariaDB, PostgreSQL, DuckDB and ClickHouse, functions of Prisme's own, then a "global" Prisme dialect)
