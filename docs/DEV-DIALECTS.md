# Implementation plan of the dialects

This document orders the work on the dialects of the v1, step by step: the parsers of the input dialects (MySQL, MariaDB, PRQL), the logical plan, the emitters of the output dialects (MySQL, MariaDB), the audit of the transpilation and the beautifier. The decisions themselves are in `docs/V1.md` (sections 2.1, 2.2, 2.9 and 2.10) and in `docs/QUESTIONS.md`; this document cites them and says in which order they are built.

Each step ends on a state usable end to end, with its tests and its documents updated. The **[to decide]** marks and the numbered points ("point 39") refer to the open points of `docs/QUESTIONS.md` (section 5).

## 1. Decisions the plan relies on

- **Input dialects** of the v1: MySQL, MariaDB, PRQL, plus "Selon source de données" (according to the data source), which is the native SQL of the target. The choice is a setting of the user's profile, "Dialecte préféré" (Preferred dialect), which the editor tab can override (`docs/V1.md`, section 2.1).
- **Versions**: the user chooses the version of the dialect they write in (MySQL 8.0.19, MariaDB 10.6...). The versions offered are those where the dialect changed (additions, removals), listed with their changes in the registry of the versions (step 2). With "Selon source de données", the version is the server's.
- **Output dialects** of the v1: MySQL and MariaDB, the engines Prisme has a connector for. PostgreSQL, SQLite and DuckDB come with their connectors (`docs/V1.md`, section 4).
- **Transpilation**: parser (input dialect to AST), plan builder (AST to logical plan), emitter (logical plan to output dialect). The logical plan is the single pivot; a query whose input dialect is the output dialect goes through it too.
- **Everything is written by Prisme**: one parser per dialect, the plan builders, the emitters, the PRQL parser (the whole language, after the PRQL book, https://prql-lang.org/book/). `sqlparser-rs` becomes a dev-dependency, an oracle of the tests.
- **Scope**: the DML (`SELECT`, `INSERT`, `UPDATE`, `DELETE`), plus the catalog and session statements (`SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`, `SET`, queries on the catalog of the engine). DDL is set aside.
- **Never a silent approximate translation** (rule 2 of `CLAUDE.md`): each construct is translated natively, emulated by Prisme (result type and use of indexes kept, except the position of NULLs in a sort, whose loss of index is reported), or refused with an explicit error. A catalog or session statement is translated as far as needed; when the target gives less than the same request written in its own dialect, the report states the loss of precision.
- **Audit**: every rewrite and every emulation is listed, with its position in the input; every untranslatable construct is listed with its error.
- **Settings of the server**: a result that depends on the settings of the server or of the data (collation, `sql_mode`, time zone) follows the target server. The transpiler translates the language and neither emulates nor reports these settings: on the same rows, `WHERE v = 'a'` keeps `'a '` on MariaDB (PAD SPACE collation by default) and leaves it out on MySQL 8 (`utf8mb4_0900_ai_ci`, NO PAD) (`docs/FEDERATED.md`, section 6).
- **Beautifier**: generated from the plan, comments kept (`docs/V1.md`, section 2.9).

## 2. Target architecture

### 2.1 Crates and modules

| Place | Role |
|---|---|
| `crates/types` (new) | Internal types with their nullability (`docs/FEDERATED.md`, section 6). |
| `crates/plan` (new) | Nodes of the logical plan (`docs/FEDERATED.md`, section 5.1) and their expressions, with the origin of each node (section 2.3 below). Depends on no dialect. |
| `crates/dialects` | Lexers, ASTs and parsers of the input dialects, plan builders, emitters of the output dialects, function catalog, transpilation report. |
| `crates/splitter` | Unchanged at first; later on the lexer of the dialect (`docs/QUESTIONS.md`, point 30). |
| `crates/server` | Chooses the input and output dialects, transpiles, sends the report, runs the output. |
| `web/` | Choice of the dialect (profile, tab), preview of the transpilation, highlighting per input dialect. |

Proposed modules of `crates/dialects`:

- `versions`: registry of the changes of each dialect, version by version (constructs, keywords, functions), source of the versions offered to the user;
- `lexer`: lexer of the SQL dialects, set by the dialect, its version and the SQL mode;
- `sql::ast`: AST common to the SQL dialects, with the nodes specific to a dialect;
- `mysql`: grammar common to MySQL and MariaDB; `mariadb`: what only MariaDB has;
- `prql`: lexer, AST and parser of PRQL;
- `build`: plan builders, from the SQL AST and from the PRQL AST;
- `emit`: SQL emitter common to the output dialects, one module per output dialect for its differences;
- `catalog`: functions of each dialect and version (native, emulated, unsupported);
- `transpile`: entry point and report.

The modules become crates if the build time requires it (measures in `docs/PERF.md`).

### 2.2 Data flow and report

The processing of a query is the diagram of `docs/V1.md`, section 2.10. Main types, as a sketch:

```rust
/// Dialect the user writes in: the native SQL of the target, or a language with its version.
pub enum InputDialect { Source, Chosen { language: Language, version: Version } }

/// Language of an input dialect.
pub enum Language { MySql, MariaDb, Prql }

/// Dialect of the server that runs the query, with its version.
pub struct OutputDialect { pub engine: Engine, pub version: Version }

/// Result of a transpilation, sent to the interface before running.
pub struct Transpilation {
    /// Query in the output dialect.
    pub sql: String,
    /// Rewrites, emulations and losses of precision, each with its rule and its position in the input.
    pub steps: Vec<Step>,
    /// Untranslatable constructs, each with its position and its reason.
    pub errors: Vec<Untranslatable>,
}
```

Each rule of translation (structural rewrite or function of the catalog) has a stable identifier, cited by the report and by its test (rule 8 of `CLAUDE.md`). The report is a type generated for the interface by `ts-rs`.

### 2.3 Origin of the plan nodes

Each node and each expression of the plan keeps its **origin**: its position in the input (for the audit, the comments and the errors) and the syntactic form written by the user when several forms have the same meaning (`VALUES ROW (...)` or `VALUES (...)`, `<=>` or `IS NOT DISTINCT FROM`, `LIMIT a, b` or `LIMIT b OFFSET a`, alias with or without `AS`). The emitter writes the original form when the output dialect accepts it; otherwise it writes another form and records a step in the report. Hence:

- a query transpiled to its own dialect comes out with its forms, and its report holds only the rewrites for optimisation;
- every difference between the input and the output is a step of the report, none is silent.

Decided (point 39), with a reservation: the principle can evolve. A rewrite for optimisation (a range in place of a function, to keep the use of an index) changes the form even towards the same dialect; it is then a step of the report, like any other difference.

### 2.4 Invariants, checked at each step

1. **Round trip**: for each dialect and each query of the corpus, parsing the output again gives the same plan (fixpoint), and the report of a transpilation to the same dialect holds only rewrites for optimisation.
2. **Same results**: on a real server, the query and its transpilation return the same rows (conformance suite).
3. **Comments**: the same comments, with the same text, before and after.
4. **No silent approximation**: every difference between input and output comes from an identified rule, with its test and its line in the report.
5. **Oracle**: on the corpus, the parser of Prisme accepts and refuses the same queries as `sqlparser-rs`, and the trees agree, except for listed differences (MariaDB, versions, known gaps of `sqlparser-rs` such as `JSON_TABLE`). The trees are compared through a canonical printing: `Display` of `sqlparser-rs` on one side, a test printer of the AST of Prisme in the same form on the other.
6. **Performance**: time linear in the size of the query; budget measured on the long single-line query of `docs/PERF.md`.

## 3. Steps

### Step 0. Decisions before the code

Settled before step 1, as the types and the plan depend on them, from the behaviour measured on MySQL 8.4.11, MariaDB 10.6.28 and MariaDB 11.8.9. Done.

- NULL, points 1 to 6 of `docs/FEDERATED.md`, section 7 (`docs/QUESTIONS.md`, point 12): the two equalities, three-valued logic, joins, set operators, aggregation, sorting;
- the internal types of the v1 (`docs/FEDERATED.md`, section 6): integers (unsigned `BIGINT` included), exact decimals, booleans, strings, dates, JSON.

### Step 1. Types and plan

- `crates/types`: integer types (sizes, signed or not), exact decimal, floating point, string, binary, boolean, date and time types, JSON, unknown type; nullability. Conversion from the column types the MySQL connector reads (`crates/connectors/src/mysql/column_type.rs`).
- `crates/plan`, the nodes the DML needs: `Scan`, `Values`, `Filter`, `Project`, `Extend`, `Aggregate`, `Sort`, `Limit` (count and offset), `Distinct`, the joins (inner, left, right, cross; full later), the six set operations, common table expressions (recursive included), window functions; `Insert`, `Update`, `Delete` at the root; for step 8, the relations of the catalog of Prisme, read by `Scan`, and the session nodes (`Use`, `Set`, `Explain`). Expressions: column references, literals, operators with explicit semantics (comparison equality and identity equality), calls to functions of the catalog, `CASE`, `CAST`, subqueries (scalar, `IN`, `EXISTS`).
- Origin of the nodes (section 2.3); output schema of each node. The column types come from the catalog of the connection, read on demand for the tables of the query, just before transpiling, on the connection of the session (`SHOW COLUMNS` also sees its temporary tables); the type of an expression the plan cannot derive is asked to the server through the metadata of a prepared statement, without running it (point 44).
- Tests: construction, output schemas, nullability rules decided in step 0.
- Done when the plan represents by hand the queries of step 4.

### Step 2. Registry of the versions

The user chooses the version of the dialect they write in (point 43): Prisme knows the period of validity of each keyword, construct and function.

- **Content**, per dialect: each construct of the grammar (`WITH`, `WITH RECURSIVE`, `VALUES` and `TABLE` statements, row alias of `INSERT ... ON DUPLICATE KEY UPDATE`, `RETURNING`...), each keyword and each function, with the version it appeared in (`since`), the version it was removed in (`until`) and its source (reference manual or release notes of the engine). The `keywords/*.yaml` files already follow this model for the keywords; the registry extends it to the constructs and the functions.
- **Examples**, each confirmed by its source when it enters the registry: common table expressions, recursive included, from MySQL 8.0.1; `VALUES` and `TABLE` statements from MySQL 8.0.19; `INTERSECT` and `EXCEPT` from MySQL 8.0.31; the `UUID` type from MariaDB 10.7.
- **Versions offered** to the user: those where the dialect changed (a version that adds or removes something), derived from the registry. The registry starts with the most recent and most widely used versions, MySQL 8.0 and later, MariaDB 10.6 and later; older versions can be added afterwards (point 51).
- **Storage**: data files in the repository, one per dialect, edited directly, read in the diffs and embedded in the executable, on the model of `crates/dialects/keywords/*.yaml` (point 50).
- **Use**: the lexer and the parser accept a keyword or a construct only within its period, for the chosen version, otherwise an explicit error ("`VALUES` statement: from MySQL 8.0.19, chosen version 8.0.18"); the emitter checks the version of the server the same way; executable comments are resolved against the chosen version (point 41). Each construct added to a parser in the next steps enters the registry with its versions.
- **Tests**: each entry has its test (accepted from `since`, refused before); the entries are checked on real servers of the versions available in Docker (ignored tests).

### Step 3. Lexer of the MySQL family

- Tokens with their position (byte offsets, line and column), comments kept as tokens with their position.
- Set by the dialect (MySQL or MariaDB), its version (registry, step 2) and the SQL mode read on connection (`ANSI_QUOTES`, `NO_BACKSLASH_ESCAPES`, `PIPES_AS_CONCAT`; `docs/QUESTIONS.md`, point 30).
- Executable comments (`/*! ... */`, `/*!80000 ... */`, `/*M! ... */`) are code for the server, depending on its version. Prisme resolves their condition (version of the server towards the same dialect, version of the input dialect otherwise, point 43): the content is then code, given to the parser and translated with the rest of the query, or a plain comment. A content that cannot be translated is reported (point 41).
- Tests: units, oracle (tokens of `sqlparser-rs` on the corpus, listed differences aside), performance budget.

### Step 4. First vertical slice: the example of the dialects

Goal: `WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds`, written in MySQL, runs on MariaDB as `WITH ds (c) AS (VALUES (123), (234), (345)) SELECT * FROM ds`, and the report shows the rewrite with its position.

- **Parser**: minimal MySQL grammar: `WITH` with a column list, `VALUES` and `VALUES ROW`, `SELECT` (columns, `*`, expressions with literals, arithmetic, comparisons, `AND`, `OR`, `NOT`), `FROM` a table or a common table expression, `WHERE`, `ORDER BY`, `LIMIT`. Any other construct gives the error "not supported yet by the parser of Prisme", with its position.
- **Plan builder** and **emitters** MySQL and MariaDB for this subset, aware of the versions: the `VALUES` statement from MySQL 8.0.19, table value constructors in every MariaDB version covered (10.6 and later).
- **Entry point** `transpile(input, input_dialect, output_dialect) -> Transpilation`; the current `Dialect` enum (MySQL, MariaDB) becomes the output dialect, with the server version.
- **Server**:
  - profile setting, migration `users.preferred_dialect` (`TEXT NOT NULL DEFAULT 'source'`: `source`, or a dialect with its version, such as `mysql 8.0.19`, checked by the server against the registry);
  - override per editor tab, migration `editors.dialect` (`TEXT NOT NULL DEFAULT ''`, same values, the empty string meaning "the value of the profile");
  - route `POST /api/transpile` (preview without running);
  - when the input dialect is not "Selon source de données": transpilation, then the current read / write classification and row limit applied to the output (temporary, until step 10), then running; the report goes with the result. "Selon source de données" keeps the current path until step 10, so that the default setting changes nothing.
- **Interface**: choice of the dialect and of its version in the profile dialog and on the editor tab (PRQL offered from step 9), transpiled query and report in the result tab, preview button. Light and dark modes checked (rule 10 of `CLAUDE.md`).
- **Tests**: units, round trip, route, interface (Vitest), and on a MariaDB server (Docker): the MySQL query of the example returns its three rows.

### Step 5. Complete MySQL DML grammar

- `SELECT`: options (`DISTINCT`, `DISTINCTROW`, `HIGH_PRIORITY`, `STRAIGHT_JOIN`, `SQL_NO_CACHE`, `SQL_CALC_FOUND_ROWS`...), all the expressions with their precedence (`DIV`, `MOD`, `XOR`, `<=>`, `REGEXP`, `SOUNDS LIKE`, `MEMBER OF`, `->`, `->>`, `COLLATE`, `BINARY`, `:=`, `@variable`, `@@variable`, `INTERVAL`, `CASE`, `CAST`, `CONVERT`, `EXTRACT`; literals with charset introducers, hexadecimal and bit literals, `DATE '...'`), `FROM` (all the joins, `USING`, `NATURAL`, derived tables, `LATERAL`, `JSON_TABLE`, index hints, `PARTITION`), `GROUP BY ... WITH ROLLUP`, `HAVING`, `WINDOW` and window functions, the forms of `LIMIT`, set operations with parentheses, recursive common table expressions, `TABLE t`, locking clauses (`FOR UPDATE`, `FOR SHARE`, `NOWAIT`, `SKIP LOCKED`, `LOCK IN SHARE MODE`), `INTO` (variables, `OUTFILE` and `DUMPFILE`, which are writes), optimizer hints `/*+ ... */`.
- `INSERT` and `REPLACE` (`VALUES`, `SET`, `SELECT`, `TABLE`, `ON DUPLICATE KEY UPDATE` with `VALUES()` or a row alias), multi-table `UPDATE` and `DELETE`, with `ORDER BY` and `LIMIT` (point 34).
- Type of a user variable (`@x`): read with `SELECT @x, @y` on the connection that runs the query, just before running it, after the previous queries of the batch; each query of a batch is therefore transpiled just before it runs (point 44).
- Constructs with no effect on the result (hints, options such as `SQL_NO_CACHE`) are annotations of the plan: written towards the same dialect, left out towards another one, with a step in the report (point 42).
- **Corpus**: queries written for Prisme, grouped by family, each with its expected status per dialect and version, in `crates/dialects/tests/corpus/`; the queries of the conformance suite are part of it. No copy of the test suites of MySQL or MariaDB (GPL license).
- **Tests**: oracle `sqlparser-rs`; acceptance confirmed by the real server (`PREPARE` on a test schema, Docker); round trip with the same results on MySQL.
- Done when the whole MySQL DML corpus passes the invariants of section 2.4.

### Step 6. MariaDB parser and emitter, first translation pairs

- What only MariaDB has: `RETURNING` (`INSERT`, `REPLACE`, `DELETE`), system versioning (`FOR SYSTEM_TIME AS OF`, `BETWEEN`, `FROM ... TO`, `ALL`), sequences (`NEXT VALUE FOR`, `NEXTVAL()`, `PREVIOUS VALUE FOR`, `LASTVAL()`), `LIMIT ... ROWS EXAMINED`, `OFFSET ... FETCH`, and the other differences of `docs/V1.md`, section 2.2, checked version by version in the documentation of both engines. The Oracle mode of MariaDB (`sql_mode=ORACLE`) is refused explicitly.
- MariaDB emitter, aware of the versions; translation MySQL to MariaDB and MariaDB to MySQL, one rule per difference, each with its test.
- First list of the untranslatable constructs (`docs/QUESTIONS.md`, point 32), for example `RETURNING` or `FOR SYSTEM_TIME` towards MySQL.
- **Tests**: conformance on both servers: each query of the corpus written for one engine runs on the other with the same results, or gives the expected error.

### Step 7. Function catalog

- Format: for each function, its names per dialect, its signature, its result type and nullability; for each output dialect and version, its level: native, emulated, local (later, step 13) or unsupported. Proposal: names, versions and levels in a data file per dialect (`yaml_serde` is already a dependency), emulations in Rust on the plan, never on the text.
- First families: dates and times, strings, conditionals, numbers, JSON (a native type in MySQL, a text in MariaDB), regular expressions (different libraries in MySQL and MariaDB: their semantics are compared, function by function).
- Emulations keep the result type and the use of indexes (rewriting as a range, `docs/V1.md`, section 2.1).
- Functions of the user (stored functions of the database): native on their own server, known through the catalog of the connection.
- Editor: emulated and untranslatable functions underlined (`docs/QUESTIONS.md`, point 28).
- **Tests**: each function of the catalog, on both servers, results compared, NULL included.

### Step 8. Catalog and session statements

Prisme translates them as far as needed (point 40). In the v1, between MySQL and MariaDB, and from PRQL; the other engines add their forms with their connector.

- **Relations of the catalog of Prisme**, with a fixed schema (databases, tables, columns, indexes, views, routines...), read by `Scan` nodes like tables. The introspection forms of each dialect map onto them: `SHOW TABLES`, `SHOW COLUMNS`, `DESCRIBE` and the `information_schema` views in MySQL and MariaDB; later `sqlite_schema` and the `PRAGMA` of SQLite, `pg_catalog` in PostgreSQL, the `duckdb_*()` functions of DuckDB.
- The emitter writes them with the means of the target, in the form that loses the least information and accepts the rest of the query (filters, joins, sorts): a query on `information_schema` in MySQL and MariaDB, or `SHOW` when the input used it and the target accepts it (origin, section 2.3; point 49).
- Example, once SQLite is an input dialect: `SELECT name FROM sqlite_schema WHERE type = 'table'` on a MySQL connection becomes a query on `information_schema.tables` (base tables of the current database). The columns MySQL cannot give in the same query (`rootpage`; `sql`, the text of creation, which MySQL gives table by table with `SHOW CREATE TABLE`) are listed in the report as a loss of precision.
- Session statements: `USE`, `SET` (session variables, `NAMES`, `TRANSACTION`) translated by meaning, towards the setting of the same meaning in the target, otherwise an explicit error. `EXPLAIN` of a DML query: the `EXPLAIN` of the target on the transpiled query; its output differs from one engine to another, which the report states.
- The other `SHOW` (`VARIABLES`, `STATUS`, `PROCESSLIST`, `GRANTS`, `WARNINGS`, `CREATE TABLE`...): one rule each, with its test.
- Report: a step kind "loss of precision", which says what the same request written in the dialect of the target would give in addition.
- **Tests**: on a test schema, the relations of the catalog give the same rows on MySQL and MariaDB; each rule has its test.

### Step 9. PRQL

- Lexer and parser of the whole language, chapter by chapter of the book: transforms (`from`, `select`, `derive`, `filter`, `sort`, `take`, `join`, `group`, `aggregate`, `window`, `loop`) and relation functions (`append`, `remove`, `intersect`), declarations (`let`, functions, modules), literals (dates `@2026-01-01`, ranges, f-strings, r-strings, s-strings), operators (`==`, `??`, `~=`, `//`, `**`...), `case`, the `prql` header, the standard library (`std`).
- Plan builder with the semantics of the book (`docs/QUESTIONS.md`, point 16), expressed by the explicit nodes of the plan: `== null` becomes a test of identity, `/` a division with a real result, `//` an integer division.
- Splitting of a PRQL text: PRQL has no separator character, a relation is delimited by its syntax (`let`, parentheses around a pipeline, braces of a tuple). The editor finds the declarations and the main query with the PRQL parser, never with the SQL splitter. Each relation is a query of the editor: Ctrl+Enter runs the relation under the cursor with the relations it depends on, Ctrl+Shift+Enter runs every relation, one result tab each (point 47).
- S-strings (raw SQL inside PRQL): read by the parser of the output dialect, explicit error if it does not understand them (point 45).
- `target` of the `prql` header: a target different from the engine of the connection is an explicit error (point 46).
- Editor: highlighting and completion of PRQL; the keywords route takes the input dialects.
- **Tests**: the examples of the book (their license is checked before copying them into the repository; otherwise examples written for Prisme), each compared with an equivalent SQL query: same plan, or same results on both servers.
- The PRQL choice appears in the profile and on the tab.

### Step 10. Analyses and native SQL on the transpiler

- Syntax errors of the editor (`crates/server/src/analyze.rs`) given by the parsers of Prisme.
- Read / write classification on the root of the plan, in place of `classify`; check of the session settings (`session_setting_refusal`) on the parser of Prisme.
- Row limit as a `Limit` node of the plan (N + 1), written by the emitter, in place of the insertion in the text (`limit` module).
- Statements outside the DML that the sessions accept today (`SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`, `SET`): through the plan (step 8).
- "Selon source de données" goes through the transpiler (`docs/QUESTIONS.md`, point 36). Condition: corpus and conformance suite passing for MySQL and MariaDB.
- **Tests**: the current tests of `classify`, `limit` and `syntax` carried over to the new implementation, and passing.

### Step 11. Beautifier from the plan

Postponed: the beautifier is taken up again once the transpilation is in place (steps 1 to 10). The points below are the starting point of that work.

- Emitter with a layout mode: the rules of `docs/V1.md`, section 2.9 (capitals, `AS` before aliases, function names, layout), comments attached to the nodes (paragraph "Comments" of the same section).
- Statements outside the plan (DDL, stored programs, `DELIMITER` blocks): the current token-based path is not kept for them; the rest is postponed with the beautifier (point 48).
- **Tests**: the current tests of `beautify` and `layout` carried over; comment invariant; idempotence (beautifying a beautified query changes nothing).

### Step 12. `sqlparser-rs` out of the executable

- `sqlparser` moves from `[dependencies]` to `[dev-dependencies]` of `crates/dialects`; `cargo tree` confirms that the executable no longer depends on it.
- The module comments that cite `sqlparser` are updated; `docs/V1.md`, section 3.2, already describes it as a dev-dependency.

### Step 13. Audit and execution plan

- Full preview before running: query in the output dialect, steps of the report, errors, each one linked to its position in the input (selection in the editor).
- Execution plan: tree of the plan, each node marked "server" or "Prisme (Rust)". In the v1, every node runs on the server, except the local emulations; first local node: computed columns (`Extend`) evaluated by Prisme on the rows read. Scope to set (`docs/QUESTIONS.md`, point 33).
- Splitting a query and delegating its subqueries come with the federated engine (`docs/FEDERATED.md`).

### After these steps

PostgreSQL, SQLite and DuckDB (connector, parser, emitter, catalog for each), DDL in the plan, Tutorial D, the pipe SQL syntax, then the federated engine.

## 4. Tests

| Level | Content | Where it runs |
|---|---|---|
| Units | Each module (lexer, parsers, plan builders, emitters, catalog) | `cargo test` |
| Corpus | Queries per family, expected status per dialect and version | `cargo test` |
| Oracle | Same acceptance and same trees as `sqlparser-rs`, listed differences aside | `cargo test` |
| Round trip | Fixpoint of the plan, empty report towards the same dialect, comments kept | `cargo test` |
| Real servers | Acceptance (`PREPARE`) and same results after transpilation | `cargo test -- --include-ignored` (Docker) |
| Conformance | Same query towards each output dialect, results compared (rule 8 of `CLAUDE.md`) | `cargo test -- --include-ignored` (Docker) |
| Performance | Budget on long queries | measures recorded in `docs/PERF.md` |

## 5. Open points raised by this plan

Recorded in `docs/QUESTIONS.md`, section 5, "Dialects and transpilation":

- 39: origin of the plan nodes (position and syntactic form), decided, may evolve;
- 40: statements outside the DML, decided: translated as far as needed, loss of precision reported;
- 41: executable comments of MySQL and MariaDB, decided: transpiled as far as possible, otherwise reported;
- 42: constructs with no effect on the result (hints, options), decided: left out towards another dialect, reported;
- 43: version of an input dialect, decided: chosen by the user, from the registry of the versions;
- 44: types when the catalog does not give them, decided: columns read on demand, prepared statement metadata, user variables read with `SELECT @x` before running;
- 45: PRQL s-strings, decided: read by the parser of the output dialect;
- 46: `target` of the PRQL header, decided: error if it differs from the engine of the connection;
- 47: what Ctrl+Enter and Ctrl+Shift+Enter run in a PRQL text, decided: the relation under the cursor with its dependencies, every relation;
- 48: beautifier of the statements outside the plan, current path not kept, the rest postponed until the transpilation is in place;
- 49: form of the catalog statements towards MySQL and MariaDB, decided: `information_schema`, `SHOW` when the input used it;
- 50: storage of the registry of the versions, decided: data files in the repository;
- 51: oldest versions covered by the registry, decided: MySQL 8.0, MariaDB 10.6.
