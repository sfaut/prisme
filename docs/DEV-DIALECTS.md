# Implementation plan of the dialects

This document orders the work on the dialects of the v1, step by step: the parsers of the input dialects (MySQL, MariaDB, PRQL), the logical plan, the emitters of the output dialects (MySQL, MariaDB), the audit of the transpilation and the beautifier. The decisions themselves are in `docs/V1.md` (sections 2.1, 2.2, 2.9 and 2.10) and in `docs/QUESTIONS.md`; this document cites them and says in which order they are built.

Each step ends on a state usable end to end, with its tests and its documents updated. The **[to decide]** marks and the numbered points ("point 39") refer to the open points of `docs/QUESTIONS.md` (section 5).

## 1. Decisions the plan relies on

- **Input dialects** of the v1: MySQL, MariaDB, PRQL, plus "Selon source de données" (according to the data source), which is the native SQL of the target. The choice is a setting of the user's profile, "Dialecte préféré" (Preferred dialect), which the editor tab can override (`docs/V1.md`, section 2.1).
- **Versions**: the user chooses the version of the dialect they write in (MySQL 8.0.19, MariaDB 10.6...). The versions offered are those where the dialect changed (additions, removals), listed with their changes in the registry of the versions (step 2). With "Selon source de données", the version is the server's.
- **Each dialect stays in its domain**: a parser accepts only the SQL valid in its dialect and its version, and an emitter writes only SQL valid in the target dialect and version. Prisme adds no extension to a dialect: `ORDER BY a NULLS LAST`, absent from MySQL, is an error of the MySQL parser, even though the plan can represent it and the MySQL emitter can emulate it for another input dialect.
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

Decided autonomously, to be reviewed by the maintainer:

- the lexical differences are not steps of the report: case of the keywords, spaces and line breaks, quote character of an identifier, synonyms of an operator (`!=` and `<>`, `&&` and `AND`, `%` and `MOD`, `!` and `NOT`), canonical spelling of a literal of the same value. The emitter writes a literal as in the input when the output reads it the same way (same SQL mode for the quotes and the backslashes);
- in the MySQL family, a column without alias takes its name from the text of its expression: when the text written differs from the text read (`a+1` written `a + 1`), an alias keeps the name, with a step of the report;
- the comments are written again before the first construct that follows them in the input.

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

Done: `crates/types` (types, values, `i128` integers, decimals as exact text), `crates/plan` (nodes, expressions typed with their nullability, origin), conversion of the result types of the MySQL connector (`plan_types`), and the queries of step 4 built by hand (`crates/plan/tests/example.rs`). Left to the steps that need them: the relations of the catalog of Prisme (step 8); the full outer join, `USING` and `NATURAL`, row constructors (`(a, b) IN (...)`) and multi-table writes, with the complete grammar (step 5); the other written forms, with the parsers that read them.

### Step 2. Registry of the versions

The user chooses the version of the dialect they write in (point 43): Prisme knows the period of validity of each keyword, construct and function.

- **Content**, per dialect: each construct of the grammar (`WITH`, `WITH RECURSIVE`, `VALUES` and `TABLE` statements, row alias of `INSERT ... ON DUPLICATE KEY UPDATE`, `RETURNING`...), each keyword and each function, with the version it appeared in (`since`), the version it was removed in (`until`) and its source (reference manual or release notes of the engine); a construct can add a comment (`comment`), its context: what it does, how it was established. The keywords followed this model first; the registry extends it to the constructs and the functions.
- **Examples**, each confirmed by its source when it enters the registry: common table expressions, recursive included, from MySQL 8.0.1; `VALUES` and `TABLE` statements from MySQL 8.0.19; `INTERSECT` and `EXCEPT` from MySQL 8.0.31; the `UUID` type from MariaDB 10.7.
- **Versions offered** to the user: those where the dialect changed (a version that adds or removes something), derived from the registry. The registry starts with the most recent and most widely used versions, MySQL 8.0 and later, MariaDB 10.6 and later; older versions can be added afterwards (point 51).
- **Storage**: a single data file per dialect, `crates/dialects/reference/<dialect>.yaml`, the only source of what Prisme knows of the dialect (keywords, constructs, then functions), edited directly, read in the diffs and embedded in the executable (point 50). A fact that depends on the version carries its period (`since`, `until`) and cites the document of that version that attests it.
- **Use**: the lexer and the parser accept a keyword or a construct only within its period, for the chosen version, otherwise an explicit error ("`VALUES` statement: from MySQL 8.0.19, chosen version 8.0.18"); the emitter checks the version of the server the same way; executable comments are resolved against the chosen version (point 41). Each construct added to a parser in the next steps enters the registry with its versions.
- **Tests**: each entry has its test (accepted from `since`, refused before); the entries are checked on real servers of the versions available in Docker (ignored tests).

Done for the constructs (`crates/dialects/src/versions.rs`, `crates/dialects/reference/*.yaml`): floor at the first GA release of each series, MySQL 8.0.11 and MariaDB 10.6.3 (release notes); the constructs of the MySQL family that depend on the version, each with its source; the versions offered derived from them; the messages of refusal. The built-in functions have their periods in the same files (step 7), which add the versions where a function appears or disappears to the versions offered. Each file gives the last version inspected of each section (`inspected`), up to which the section is complete: an update starts from the release notes of the later versions. The checks on real servers go with the conformance suite (steps 4 and 6), on the versions available in Docker.

### Step 3. Lexer of the MySQL family

- Tokens with their position (byte offsets, line and column), comments kept as tokens with their position.
- Set by the dialect (MySQL or MariaDB), its version (registry, step 2) and the SQL mode read on connection (`ANSI_QUOTES`, `NO_BACKSLASH_ESCAPES`, `PIPES_AS_CONCAT`; `docs/QUESTIONS.md`, point 30).
- Executable comments (`/*! ... */`, `/*!80000 ... */`, `/*M! ... */`) are code for the server, depending on its version. Prisme resolves their condition (version of the server towards the same dialect, version of the input dialect otherwise, point 43): the content is then code, given to the parser and translated with the rest of the query, or a plain comment. A content that cannot be translated is reported (point 41).
- Tests: units, oracle (tokens of `sqlparser-rs` on the corpus, listed differences aside), performance budget.

Done (`crates/dialects/src/lexer.rs`): tokens with their position in bytes, line and column computed for the messages (`LineIndex`); comments as tokens; SQL mode read from `@@sql_mode` (`SqlMode`, combination modes expanded); executable comments resolved against the version given by the caller, six-digit versions read as MySQL 8.0.34 and later do (six digits followed by a whitespace) and as MariaDB does (six digits). Oracle: the tokens of a corpus cut like those of `sqlparser-rs`; listed difference: an unquoted user variable keeps its dots (`@a.b`), as in MySQL. Budget: 1 MB on a single line, about 240,000 tokens, in a fraction of a second.

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

Done: transpiler (`crates/dialects`: `sql`, `build`, `emit`, `report`, `transpile`), server (`crates/server/src/transpilation.rs`) and interface (list of the dialects in the profile and on each editor, report folded above the result, preview window; light and dark modes checked). Decided autonomously, to be reviewed by the maintainer:

- the columns of the tables come from the metadata of `SELECT * FROM t`, prepared on the connection that runs the query, without running it: its temporary tables are seen, a view is not computed, and the types are those the connector already reads from a result (`plan_type`);
- the interface sends the dialect with each run (`dialect` of the `run` message), resolved from the tab and the profile; the server checks it against the registry;
- the report comes in a `transpiled` message, before the result, and is saved with it;
- the preview (`POST /api/transpile`) reads the columns on a new connection placed on the database of the session: the temporary tables of the session are not seen there;
- the editor highlights and analyses the text in the dialect it is written in, with its version; the syntax errors underlined as you type still come from `sqlparser-rs` until step 10, the parser of Prisme giving its own error when the query runs.

### Step 5. Complete MySQL DML grammar

- `SELECT`: options (`DISTINCT`, `DISTINCTROW`, `HIGH_PRIORITY`, `STRAIGHT_JOIN`, `SQL_NO_CACHE`, `SQL_CALC_FOUND_ROWS`...), all the expressions with their precedence (`DIV`, `MOD`, `XOR`, `<=>`, `REGEXP`, `SOUNDS LIKE`, `MEMBER OF`, `->`, `->>`, `COLLATE`, `BINARY`, `:=`, `@variable`, `@@variable`, `INTERVAL`, `CASE`, `CAST`, `CONVERT`, `EXTRACT`; literals with charset introducers, hexadecimal and bit literals, `DATE '...'`), `FROM` (all the joins, `USING`, `NATURAL`, derived tables, `LATERAL`, `JSON_TABLE`, index hints, `PARTITION`), `GROUP BY ... WITH ROLLUP`, `HAVING`, `WINDOW` and window functions, the forms of `LIMIT`, set operations with parentheses, recursive common table expressions, `TABLE t`, locking clauses (`FOR UPDATE`, `FOR SHARE`, `NOWAIT`, `SKIP LOCKED`, `LOCK IN SHARE MODE`), `INTO` (variables, `OUTFILE` and `DUMPFILE`, which are writes), optimizer hints `/*+ ... */`.
- `INSERT` and `REPLACE` (`VALUES`, `SET`, `SELECT`, `TABLE`, `ON DUPLICATE KEY UPDATE` with `VALUES()` or a row alias), multi-table `UPDATE` and `DELETE`, with `ORDER BY` and `LIMIT` (point 34).
- Type of a user variable (`@x`): read with `SELECT @x, @y` on the connection that runs the query, just before running it, after the previous queries of the batch; each query of a batch is therefore transpiled just before it runs (point 44).
- Constructs with no effect on the result (hints, options such as `SQL_NO_CACHE`) are annotations of the plan: written towards the same dialect, left out towards another one, with a step in the report (point 42).
- **Corpus**: queries written for Prisme, grouped by family, each with its expected status per dialect and version, in `crates/dialects/tests/corpus/`; the queries of the conformance suite are part of it. No copy of the test suites of MySQL or MariaDB (GPL license).
- **Tests**: oracle `sqlparser-rs`; acceptance confirmed by the real server (`PREPARE` on a test schema, Docker); round trip with the same results on MySQL.
- Done when the whole MySQL DML corpus passes the invariants of section 2.4.

Progress, first increment: sources, set operations and subqueries. Read, built and written towards MySQL and MariaDB: joins (comma, `JOIN`, `INNER`, `CROSS`, `STRAIGHT_JOIN`, `LEFT` / `RIGHT [OUTER]`, `NATURAL`, `ON`, `USING`), sources between parentheses, derived tables (`LATERAL`, column list), `PARTITION`, index hints, set operations (`UNION`, `EXCEPT`, `INTERSECT`, with `ALL` / `DISTINCT` and parentheses), `TABLE t`, subqueries (scalar, `IN`, `EXISTS`, `ANY` / `SOME` / `ALL`, correlated), rows (`(a, b)`, `ROW(a, b)`). The grammar follows what MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8 accept, measured on the servers:

- the comma joins loosest (`a, b JOIN c ON x` is `a, (b JOIN c ON x)`); a join reads its right side greedily; an inner join without condition whose right side is a join joins the leftmost table of that side (`a JOIN b JOIN c ON x` is `(a JOIN b) JOIN c ON x`); `LEFT` and `RIGHT` need `ON` or `USING`; `NATURAL` reads a single table; `STRAIGHT_JOIN` takes `ON` only;
- `USING` and `NATURAL` merge the columns of the same name: the merged column answers to its name alone and comes first in `*`, taken from the left side (from the right side for a right join, whose columns then come first); each side keeps its own column, reached by its qualifier, `t.*` included;
- `INTERSECT` binds tighter than `UNION` and `EXCEPT`; before MySQL 8.0.31, `INTERSECT` and `EXCEPT` are not reserved, hence aliases;
- `<=>` has no `ANY` or `ALL`; a row compares with `= ANY` and `<> ALL` only, the forms of `IN` and `NOT IN`; `ROW` holds two values at least.

New rules of the report: `table.select` (`TABLE t` written `SELECT * FROM t` towards MariaDB and MySQL before 8.0.19), `derived.columns` (column list of a derived table emulated by a common table expression towards MariaDB before 11.7.1), `set.nested` (set operation between parentheses as a right operand, emulated by a derived table towards MySQL before 8.0.22; an error when its columns have the same names), and the errors `set.operator` (`INTERSECT` and `EXCEPT` towards MySQL before 8.0.31), `derived.lateral` (towards MariaDB), `derived.outer-reference` (column of an enclosing query in a derived table, towards MariaDB and MySQL before 8.0.14), `join.kind` (semi-joins and anti-joins of other dialects). Decided autonomously, to be reviewed by the maintainer:

- the merged column of `USING` and `NATURAL` takes the type of the side it comes from, not null after an inner join; the side of an outer join that may find no match is nullable in the expressions of the query;
- `UNION DISTINCT` and `UNION`, `SOME` and `ANY`, `ROW(a, b)` and `(a, b)` are lexical synonyms: the plan does not keep them;
- `INTERSECT`, `EXCEPT`, `LATERAL` and the outer references of a derived table are explicit errors towards the outputs without them, until their emulations (`EXISTS` with `<=>` for `INTERSECT` and `EXCEPT`, for example) are written;
- subqueries of several columns compared with a row (`(a, b) = (SELECT x, y ...)`) are refused by the builder, "not supported yet"; so is `JSON_TABLE` by the parser. The several levels of `ORDER BY` and `LIMIT` of MySQL before 8.0.31 are not modeled.

Progress, second increment: function calls, aggregation and windows. Read, built and written towards MySQL and MariaDB: function calls (`f(a)`, `db.f(a)`, a reserved word before `(` such as `LEFT`), aggregate functions (`COUNT(*)`, `DISTINCT`, `GROUP_CONCAT` with `ORDER BY` and `SEPARATOR`), `GROUP BY` (expressions, positions, aliases, `WITH ROLLUP`), `HAVING`, window functions (`OVER w`, `OVER (w ...)`, `PARTITION BY`, `ORDER BY`, `ROWS` / `RANGE` frames) and the `WINDOW` clause. Measured on the servers:

- the names of some built-in functions touch `(`: `SUM (a)` calls a stored function, `COUNT (*)` is a syntax error (the list is in the function catalog, step 7);
- `GROUP BY` and `HAVING` read a column of the source before an alias of the select list (`SELECT x AS k ... GROUP BY k` groups on the column `k`), `ORDER BY` an alias first; a key that reads an aggregate function is refused;
- `ORDER BY` with `WITH ROLLUP`: MySQL since 8.0.12, never MariaDB;
- `OVER w` uses the window as defined, its frame included; a window that completes another one (`OVER (w ORDER BY a)`, `WINDOW v AS (w ...)`) adds an order when `w` has none and a frame, never a partition, and `w` has no frame; MySQL reads the window a definition completes anywhere in the `WINDOW` clause, MariaDB before it;
- MariaDB computes `RANK`, `DENSE_RANK`, `PERCENT_RANK`, `CUME_DIST`, `LAG` and `LEAD` over an ordered window only, and its `LAG` and `LEAD` have no default value;
- types: `COUNT` is a `BIGINT` not null; `SUM` of an exact number is a `DECIMAL` of 22 more digits, `AVG` a `DECIMAL` of 4 more digits and 4 more of scale, both a `DOUBLE` otherwise; `MIN` and `MAX` keep the type; `BIT_AND`, `BIT_OR` and `BIT_XOR` are `BIGINT UNSIGNED` not null; the ranks are `BIGINT UNSIGNED` in MySQL, `BIGINT` in MariaDB; `LAG`, `LEAD`, `FIRST_VALUE`, `LAST_VALUE` and `NTH_VALUE` widen the integers to `BIGINT` in MySQL and keep the type in MariaDB; `PERCENT_RANK` and `CUME_DIST` are not null in MySQL only.

A grouped block (`GROUP BY`, or an aggregate function in its select list, its `HAVING` or its `ORDER BY`) gives `Project(Sort(Window(Filter(Aggregate(...)))))`: the expressions above the aggregation read its columns, `#key1`, `#aggregate1`..., and those of the window functions, `#window1`...; a part equal to a key reads the key; a column neither grouped nor aggregated reads any value of its group, an implicit `ANY_VALUE` written as the column alone, which the MySQL family accepts when the column depends on the keys or without `ONLY_FULL_GROUP_BY` (the default of MariaDB), a setting of the server the transpiler leaves to the target. The emitter writes these columns back as what they stand for, and each key of `GROUP BY` in the form written.

New rules of the report: `window.order` (window ordered by `NULL`, which keeps all the rows peers, for the ranking and offset functions towards MariaDB), `window.definitions` (named windows written in the order MariaDB reads them), and the errors `window.default` (`LAG` and `LEAD` with a default value, towards MariaDB), `rollup.order` (`WITH ROLLUP` with `ORDER BY`, towards MariaDB and MySQL before 8.0.12). Decided autonomously, to be reviewed by the maintainer:

- the scalar functions are typed unknown and nullable until the types of the function catalog (step 7); `JSON_ARRAYAGG` and `JSON_OBJECTAGG`, a `JSON` in MySQL and a text in MariaDB, are refused across;
- `HAVING` without grouping, `*` in a grouped query, `GROUP BY ... ASC` / `DESC` (MariaDB), `LIMIT` in `GROUP_CONCAT`, `RESPECT NULLS`, `FROM FIRST`, `WITHIN GROUP` and the functions whose arguments have a syntax of their own (`CAST`, `TRIM`, `EXTRACT`...) are "not supported yet"; `GROUPING()` is a scalar function of MySQL;
- an aggregate function in a subquery whose arguments read only an enclosing query belongs to that query in the MySQL family; the plan gives it to the subquery, and a subquery of a grouped block reads the columns of the source: the text written is the same, the plan approximates;
- an alias read by `HAVING` is written as its expression; `OVER (w)` without additions is written `OVER w`, the same window;
- the names `#key`, `#aggregate` and `#window` followed by digits are reserved by Prisme for these columns: a column of the source with such a name is refused.

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

Progress, first increment: the catalog of MySQL and MariaDB. The `functions` section of each reference file (`crates/dialects/reference/*.yaml`, read by the `builtins` module) lists the built-in functions by family, each family with the page of the manual that describes it: 396 functions of MySQL, 486 of MariaDB, from the reference manuals of MySQL 8.4 and MariaDB and the inventory of the check servers (MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8). Each function carries its period and its source when it depends on the version, its form, and its translation to the other dialect: `same`, another name, a rule of Prisme, or an error with its reason, from the measurements on the four servers (type, nullability and value of each function over a set of arguments, `CREATE TABLE ... AS SELECT`). Measured on the servers:

- a call is to a built-in function of the dialect in its version, or to a stored function: a qualified name, a name the version does not know, the name of a function of the grammar between backticks (`` `LEFT`(a) ``, while `` `ABS`(a) `` is the built-in function), or followed by a space for the names that touch `(` (`NOW ()`, `SUM (a)`, while `AVG (a)` and `CONCAT (a)` stay built-in), unless the SQL mode `IGNORE_SPACE` is set; a stored function has no window (`f(a) OVER ()` is a syntax error);
- most functions give the same results in both dialects; the others are rules: `NOW`, `CURRENT_TIMESTAMP`, `SYSDATE` and `FROM_UNIXTIME` give a `TIMESTAMP` in MariaDB from 11.7.1 (MDEV-15751), `LOCALTIME` a `TIME` from 11.6.2 (MDEV-34829), `UUID()` a `UUID` from 10.7 (MDEV-26992); `UNIX_TIMESTAMP` and `FROM_UNIXTIME` have other ranges (MySQL gives 0 where MariaDB gives `NULL`); `LPAD` and `RPAD` with an empty padding, `BIT_COUNT` of a binary string, `GREATEST` and `LEAST` over numbers and strings, `EXTRACT` from a `TIME`, `TIME_TO_SEC` of a fractional time, the optional arguments of `AES_ENCRYPT` and `AES_DECRYPT`, the regular expressions (ICU in MySQL, PCRE in MariaDB) and `ADD_MONTHS` of MariaDB differ.

The measurements go through the development tool `probe` (`crates/probe`, `docs/DEVELOPER.md`): `expr` compares the behaviours of expressions, `functions` checks the reference file against a server, of a version newer than the one inspected included.

New rules of the report: the errors `function.absent` (function the output lacks), `function.version` (function outside its period in the version of the server), `function.untranslatable` (with its reason), `function.unstudied` (translation not studied yet) and `function.stored` (stored function named like a built-in function of the output, also a rewrite when a space or backticks keep calling it), the rewrites `function.name`, `function.quoted` and `function.spaced`; each rule of a function (`function.now`, `function.pad`...) is an explicit error until written. Decided autonomously, to be reviewed by the maintainer:

- the catalog lives in the reference file of each dialect, with one translation key per other dialect; a verdict comes from the measurements on the servers, the documentation giving the periods;
- the internal functions of MySQL (`CAN_ACCESS_...`, `INTERNAL_...`, `GET_DD_...`), which refuse users, are left out, as are the functions of the versions newer than the servers measured (MariaDB 12);
- `WEIGHT_STRING` (collation of each server), `COERCIBILITY` (another value for a literal in MariaDB 11.8), `CURRENT_ROLE` (another format), `JSON_ARRAYAGG` and `JSON_OBJECTAGG` are untranslatable;
- the JSON, spatial and replication functions, `CAST`, `CONVERT`, `DEFAULT` and `VALUES` wait for their study: "not studied yet";
- a stored function named like a built-in function of the output keeps calling the database with a space before `(` or between backticks, according to the grammar of the output, otherwise the call is refused: its database must be named.

Rules written, each with its tests (the emulations checked on the servers):

- `function.bit-count`: `BIT_COUNT` counts the bits of an integer, of a date or a time read as a number, of the index of an `ENUM` alike in both dialects. A byte string: MySQL counts the bits of its bytes, MariaDB reads it as a number; towards MariaDB, each hex digit of `HEX(x)` replaced by as many `x` as its bits, then `LENGTH`. A character string: MySQL reads an integer, its fraction dropped, MariaDB a decimal, rounded; towards MariaDB, `CAST(x AS SIGNED)`, measured equal to the reading of MySQL. A number from 2^63: MySQL counts the bits of 2^63 - 1, MariaDB those of the number taken as unsigned, of 2^64 - 1 from 2^64; towards MariaDB, `LEAST(x, 9223372036854775807)`. Towards MySQL, a string or a large number is read as MariaDB does: `CAST(x AS DECIMAL(65, 30))` for a string, rounded, bounded to `[-2^63, 2^64 - 1]` by `LEAST` and `GREATEST`, then taken as the unsigned integer of the same bits by `(v + 2^64) % 2^64` cast to `UNSIGNED`, as `CAST` of a negative decimal to `UNSIGNED` gives 0 in MySQL 8.0.11 and wraps in 8.4; a string of more than 30 fractional digits is rounded at the 30th first. A floating point number is an error (decided autonomously: MySQL rounds half to even, MariaDB half away from zero, and `BIT_COUNT(9223372036854775808e0)` gives 63, 3 in MariaDB), as are a `JSON` and a type unknown. Checked on the servers, each emulation against the function of the input, MySQL 8.0.11 and 8.4 with MariaDB 10.6 and 11.8.
- `function.from-unixtime`: `FROM_UNIXTIME(n)` gives the date and time `n` seconds after the epoch within the range of `UNIX_TIMESTAMP` (below), NULL out of it, a `TIMESTAMP` in MariaDB from 11.7.1. Towards a wider range, `IF(n < cap, n, NULL)`; towards MariaDB from 11.7.1, the epoch rebuilt by `FROM_UNIXTIME(1) - INTERVAL 1 SECOND` and a `CAST` to the `DATETIME` of the fractional digits of `n` (0 for an integer, the scale of a decimal, 6 for a float or a string, measured; an error for a type unknown); towards MySQL from MariaDB 11.7.1, `NULLIF(n, 0)`. An integer of at most 4 bytes and a numeric literal bound `n`; beyond the range of the output, an error.
- `function.greatest`: `GREATEST` and `LEAST` compare numbers, strings, dates and times alike in both dialects, and a mix otherwise. A string with a number: as strings in MySQL, as `DOUBLE` in MariaDB; towards MariaDB, `CONCAT(n)` converts the numbers to strings, towards MySQL `s + 0e0` the strings to numbers. A date with a string: as dates, the result a string in MySQL, a date in MariaDB; towards MariaDB, `CONCAT(GREATEST(...))`, towards MySQL a `CAST` of the strings to `DATE`, or to the `DATETIME` of their fractional digits; each string a literal valid date, as MySQL leaves an invalid date out of the comparison where MariaDB takes it as a zero date or NULL. A time with a string or a number: as strings in MySQL, as times in MariaDB; towards MariaDB, `CONCAT` of the times and the numbers, towards MySQL a `CAST` of the strings to `TIME` (literal valid times; a number is an error). Integers or decimals with a `NULL` literal: their type in MySQL, a `DOUBLE` in MariaDB; towards MariaDB, the `NULL` becomes `IF(FALSE, x, NULL)`, `x` the first other argument, never evaluated; towards MySQL, `NULL + 0e0`. The context matters: MariaDB compares strings as numbers when the call is read as a number (`LEAST('10', '9') + 0` gives 9, 10 in MySQL), MySQL reads a date compared with a string as a number of the date, and compares a time with a string as numbers then. The emitter knows whether an expression is read as a value of its type: a column of the result of the statement, a key of `ORDER BY` or `GROUP BY` (`Emitter::column`); elsewhere, strings towards MariaDB are compared within `CONCAT(...)`, which reads the result as a string, and the other cases that depend on the context are errors (decided autonomously: the context of an operand is known only for these places for now). A `YEAR`, a `BIT`, a `JSON` and a type unknown, among which the result of a function, are errors. Checked on the servers, each emulation against the function of the input, MySQL 8.0.11 and 8.4 with MariaDB 10.6 and 11.8.
- `function.now`: `NOW`, `CURRENT_TIMESTAMP` and `SYSDATE` give a `TIMESTAMP` in MariaDB from 11.7.1, `LOCALTIME` a `TIME` from 11.6.2 (constructs `timestamp_now` and `time_localtime` of the reference file), a `DATETIME` in MySQL; towards those versions, `NOW`, `CURRENT_TIMESTAMP` and `LOCALTIME` become `LOCALTIMESTAMP`, the same instant of the statement, and `SYSDATE(n)` becomes `CAST(SYSDATE(n) AS DATETIME(n))` (decided autonomously: the cast makes the column nullable in the metadata, its values stay non-null); from MariaDB, `LOCALTIME` of those versions becomes `CURRENT_TIME`, and the `TIMESTAMP` of the others the `DATETIME` of MySQL, of the same value in the time zone of the session.
- `function.pad`: `LPAD` and `RPAD` without a pad pad with spaces in MariaDB, MySQL needs the pad (`' '` added). With an empty pad, beyond the length of the string, they give NULL in MariaDB and MySQL before 8.0.13, an empty string in MySQL from 8.0.13 and the string unchanged from 9.3.0 (constructs `pad_empty_string` and `pad_unchanged` of the reference file, read in the source of MySQL and measured on 8.0.11, 8.4 and 9.3.0); a literal pad other than the empty string goes through as is, another pad is an error when the two servers differ (decided autonomously: an emulation would evaluate the string, the length and the pad several times).
- `function.time-to-sec`: `TIME_TO_SEC` of a value that may have a fraction of seconds (a string, a decimal, a time with fractional digits) gives a whole number in MySQL, a decimal in MariaDB; towards MariaDB, `TIME_TO_SEC(x) DIV 1`; towards MySQL, an error (decided autonomously: an emulation of the fraction would not keep the scale of the result).
- `function.unix-timestamp`: `UNIX_TIMESTAMP(x)` gives the seconds from the epoch until 2038-01-19 03:14:07 UTC, 3001-01-18 23:59:59 in MySQL from 8.0.28, 2106-02-07 06:28:15 in MariaDB from 11.5.1, the epoch left out in MariaDB from 11.7.1 (constructs `unixtime_3001`, `unixtime_2106` and `unixtime_epoch_null`); out of the range, 0 in MySQL, NULL in MariaDB, and MySQL also gives 0 in the second after the epoch, its fraction dropped. Towards MariaDB, `IFNULL(u, 0)` (`u` the call written in the output), or a `CASE` that keeps `u` from 1 to the end of the range of MySQL and gives 0 otherwise to a non-null `x`; towards MySQL, a `CASE` that keeps the positive `u` within the range of MariaDB, gives 0 at the epoch, and computes the second after it as `UNIX_TIMESTAMP(x + INTERVAL 1 SECOND) - 1` (`TRUNCATE` to the digits of a literal string). A `TIMESTAMP` of the output and a valid literal date from 1971 to 2037 go through as written. Beyond the range of the output (MySQL from 8.0.28 towards MariaDB, MariaDB from 11.5.1 towards MySQL before 8.0.28), an error (decided autonomously: MariaDB computes no instant beyond 2106; an emulation by `TIMESTAMPDIFF` from its last second would be exact in a time zone of fixed offset and in the named zones, whose tables stop in 2037, never in the `SYSTEM` time zone of a system with daylight saving time). The argument that an emulation repeats must give the same value on each evaluation (neither subquery, nor stored or nondeterministic function), otherwise an error; the steps of a repeated expression are recorded once. Both rules checked on the servers, each emulation against the function of the input, at the bounds of the ranges, in UTC and at +05:00.
- `function.uuid`: `UUID()` gives a value of type `UUID` in MariaDB from 10.7.1 (construct `uuid_type`), of the same text as the `VARCHAR(36)` of MySQL but compared only with UUIDs; towards those versions, `CAST(UUID() AS CHAR(36))`; towards MySQL, the string of the same text.

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

## 6. Feedback

Difficulties met, their solutions, and the particularities of the servers and versions found along the way, for the next implementations. Each entry says how it was established; the measures go through the tool `probe` (`docs/DEVELOPER.md`, section "Measurements on real servers").

### 6.1 Method

- **Measure, then read the source.** The manuals leave out the edge cases (empty pad of `LPAD`, first second after the epoch of `UNIX_TIMESTAMP`): `probe expr` measures them on several versions at once, the source of the server explains them and dates them (`LPAD` read in `sql/item_strfunc.cc` of MySQL 8.0.13 and in Bug #117227).
- **Bracket a change between two images.** The images are published per patch version (`mysql:8.0.27`, `mysql:8.0.28`): the version of a change is measured on both sides. Some versions have no image (MariaDB 11.7.0 and 11.7.1, the first 11.7 image being 11.7.2): the `since` then comes from the fix version of the ticket, confirmed on the neighbouring images.
- **Check each emulation against the function of the input.** A unit test only checks the text written. The emulation is run on the output server and the function on the input server, value by value, at the bounds of the ranges and beyond, in UTC and in another time zone (+05:00). This check found the first second after the epoch of `UNIX_TIMESTAMP` and the scale given by `x + INTERVAL 1 SECOND`, which the unit tests could not see.
- **Compare the values, then the types.** The same function gives decimals of other precisions on the two servers (`UNIX_TIMESTAMP('…00.5')`: `DECIMAL(12,1)` in MySQL, `DECIMAL(17,1)` in MariaDB). The scale is kept by the emulations; the precision waits for the result types of the functions.
- **Read a type through `CREATE TABLE ... AS SELECT`**, the value from the column created. A user variable converts some values (a binary string read as `0x0` in MySQL 8.4). MariaDB 10.6 refuses the `CREATE TABLE ... AS SELECT` of a `JSON` column (error 1054, "Unknown column in CHECK"): `probe` then reports the type as unknown.
- **Classify the names of functions by the error of a call**, made inside `SELECT 0 + ...` so that the select modifiers (`ALL`, `DISTINCT`, `STRAIGHT_JOIN`, `SQL_*_RESULT`) are not taken for functions: 1305, no function of that name; 1582, another number of arguments; 1064, syntax of its own; 1630, a stored function named like a built-in one, which is also the error of a quoted built-in name (`` `NOW`() ``). The spatial constructors of MariaDB (`POINT`...) answer only with their exact number of arguments: a name is built-in when any form of the call reaches the function.

### 6.2 Emitter

- **The rules of the reference files apply between dialects only.** Between two versions of a dialect, a function goes through as written, even when its behaviour changed (`UNIX_TIMESTAMP` from MySQL 8.0.19 to MySQL 8.4); a rule written for both directions only meets the pairs MySQL → MariaDB and MariaDB → MySQL.
- **An emulation that repeats an argument** (`CASE WHEN UNIX_TIMESTAMP(x) < ... THEN UNIX_TIMESTAMP(x) ...`) needs an argument that gives the same value on each evaluation: neither subquery, nor stored function, nor function of `VOLATILE` (`emit/rules.rs`, `repeatable`); otherwise an error. The steps of the repeated argument are recorded once.
- **The types of the results of functions are unknown in the plan** for now: `NOW()` is nullable, of a type unknown. A rule that depends on the type takes the form that is right for any value (`UNIX_TIMESTAMP(NOW())` keeps the test of the fraction), or gives an error when it cannot (the fractional digits of the cast of `FROM_UNIXTIME`).
- **`NULLIF` evaluates its first argument twice** when it differs from the second, on both servers (`SET @c = 0` then `NULLIF((@c := @c + 1), 0)` gives 2, measured on MySQL 8.0.11 and 8.4, MariaDB 10.6 and 11.8): an emulation that writes it repeats its argument.
- **Avoid `LEAST` and `GREATEST` to bound an argument** in an emulation: their comparison of a string with a number differs between the servers (section 6.3).
- **The value of a call can depend on the expression that reads it.** A server asks an expression for a string, a number or a date depending on its parent, and some functions compute differently for each: `LEAST('10', '9')` gives `'10'`, `LEAST('10', '9') + 0` gives 9 in MariaDB (10 in MySQL), as does `ABS(...)`, `SUM(...)`, or a scalar subquery read as a number. Measure each rule inside `+ 0`, `CONCAT(...)` and `CAST(... AS SIGNED)` as well as alone. `Emitter::column` tells a rule whether its call is read as a value of its type; a string function around the call (`CONCAT(x)`) makes the server read it as a string whatever its parent.
- **`CONCAT(x)` converts a number to a string that gives way to the collation of a column**, as the implicit conversion of MySQL does; `CAST(x AS CHAR)` takes the coercibility of a column in MySQL and MariaDB 10.6, that of a literal in MariaDB 11.8 (`COERCIBILITY()` measured).
- **Prisme leaves the session time zone at the default of the server.** The tables of the named time zones stop in 2037 (the last offset applies beyond), the `SYSTEM` time zone follows the rules of the system, daylight saving time included: no emulation computes an instant beyond the range of MariaDB (2106) exactly in every time zone.

### 6.3 Particularities of the servers and versions

- **Range of `UNIX_TIMESTAMP` and `FROM_UNIXTIME`**: until 2038-01-19 03:14:07 UTC before MySQL 8.0.28 and MariaDB 11.5.1, 3001-01-18 23:59:59.999999 in MySQL from 8.0.28 (on 64-bit platforms), 2106-02-07 06:28:15 in MariaDB from 11.5.1, the epoch left out in MariaDB from 11.7.1. Out of it, `UNIX_TIMESTAMP` gives 0 in MySQL (with a warning for a zero date), NULL in MariaDB, and `FROM_UNIXTIME` NULL on both (MariaDB adds the warning "Truncated incorrect unixtime value").
- **First second after the epoch**: MySQL gives 0 with the scale of the argument (`UNIX_TIMESTAMP('1970-01-01 00:00:00.5')` gives `0.0` in UTC), MariaDB the fraction (`0.5`).
- **Zero dates**: `UNIX_TIMESTAMP` of a zero `TIMESTAMP` column gives 0 on every server; of a zero `DATETIME` or string, 0 in MySQL, NULL in MariaDB.
- **MariaDB 11.7.1** (MDEV-15751): `NOW()`, `CURRENT_TIMESTAMP`, `SYSDATE()` and `FROM_UNIXTIME()` give a `TIMESTAMP`; `FROM_UNIXTIME(0)` gives NULL, `FROM_UNIXTIME(1) - INTERVAL 1 SECOND` the epoch as a `DATETIME`; `IF(c, <DATETIME>, <TIMESTAMP>)` gives a `DATETIME`. **MariaDB 11.6.2**: `LOCALTIME` gives a `TIME`. **MariaDB 10.7.1**: `UUID()` gives a `UUID`.
- **Fractional digits of `FROM_UNIXTIME(n)`**, the same on both servers: 0 for an integer, a boolean, NULL, a date or a time without fraction; the scale of a decimal, 6 at most; those of a `DATETIME(n)` or `TIME(n)`; 6 for a float or a string. `x + INTERVAL 1 SECOND` gives 6 fractional digits to a string `x`.
- **Cast of a string with a fraction to `DATETIME`**: MySQL rounds (`CAST('1970-01-01 00:00:00.5' AS DATETIME)` gives `00:00:01`), MariaDB truncates (`00:00:00`). No rule yet: to handle with `CAST` (special syntax, step 7).
- **`LPAD` and `RPAD` with an empty pad**, beyond the length of the string: NULL in MariaDB and in MySQL before 8.0.13, an empty string from 8.0.13, the string unchanged from 9.3.0. MariaDB pads with spaces without a third argument, MySQL refuses it (error 1582).
- **`TIME_TO_SEC`** of a value with a fraction: a whole number in MySQL, a decimal in MariaDB.
- **`BIT_COUNT`** of a byte string counts the bits of its bytes in MySQL (a feature of the bit operations on byte strings of MySQL 8.0), reads it as a number in MariaDB; of a character string, MySQL reads an integer, its fraction dropped (as `CAST(x AS SIGNED)`), MariaDB a decimal, rounded (as `CAST(x AS DECIMAL(65, 30))`). Its result is a `BIGINT` in MySQL, an `INT` in MariaDB.
- **`CAST` of a negative decimal to `UNSIGNED`** gives 0 in MySQL 8.0.11 and in MariaDB, the integer of the same bits in MySQL 8.4 (`CAST(-3.7 AS UNSIGNED)`); of a negative integer, the integer of the same bits everywhere.
- **`LEAST` and `GREATEST` of a string and a number** compare them as strings in MySQL, as numbers in MariaDB: `LEAST('300', 2147483648)` gives `2147483648` in MySQL 8.0.11 and 8.4, `300` in MariaDB 10.6 and 11.8. A date with a string compares as dates in both, the result a `VARCHAR` in MySQL, a nullable `DATE` in MariaDB; a time with a string or a number compares as strings in MySQL, as times in MariaDB, of the fractional digits of the string (`LEAST(t, '9:00:00.5')` is a `TIME(1)`). An `ENUM` or a `SET` with a number compares as strings in MySQL, by its index in MariaDB. A `NULL` literal among integers or decimals makes the result a `DOUBLE` in MariaDB only. A `YEAR` with an integer gives an `INT` in MySQL, a `DECIMAL` in MariaDB; a `BIT` with a `BIT`, an `INT UNSIGNED` in MySQL, a `BIT` in MariaDB.
- **Type of `UNIX_TIMESTAMP`**: `INT(11)` in MySQL 8.0.11, `BIGINT` in 8.0.27 and later, `BIGINT(17)` in MariaDB.
- **Names read as built-in calls by the lexer** of both servers, absent from the reference files for now: `SQL_TSI_DAY`, `SQL_TSI_HOUR`... (synonyms of `DAY()`...), `CHARACTER` (`CHAR`), `ROW`, `BINARY`.
- **MySQL 9.x**: the vector functions (`STRING_TO_VECTOR`, `VECTOR_DIM`, `VECTOR_TO_STRING`) are absent from `reference/mysql.yaml`, whose functions are inspected up to 8.4.11.
