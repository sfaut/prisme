# Prisme: the federated engine (after the v1)

This document describes the original vision of the project, which comes after the v1. The starting idea: a client that queries **several sources at once** (SQL servers, CSV files) as if there were only one, with a neutral dialect. The **[to confirm]** marks flag a proposal not approved yet.

The v1 (see `docs/V1.md`) builds the common core: parser, AST, emitters per dialect, connectors, streaming, cancellation. The federated engine is added on top of it.

## 1. Need

- Query MySQL, MariaDB and CSV files (then other sources) in a single query, from 3 to 6 possible sources.
- Volumes: tens of millions of rows, sometimes hundreds of millions for details. Join keys are indexed in the databases.
- **Main motivation: performance.** DuckDB performs poorly as soon as several sources are joined: it fetches too much data. The engine must analyse the query, fetch the data optimally from each source, then assemble. There is therefore work on the server side and on the client side.
- Optimisation goal: move as little data as possible and limit local work.
- The result must be **consistent, stable, reproducible**, and seen by the user as if the source were single: harmonised types (integers, strings, booleans), as for a single database.
- Reality check to run early: "CSV joined to MySQL, with a filter", compared with DuckDB's time.

## 2. Input languages

- Neutral dialect, **strict ANSI as the core**, enriched when needed with the best of other engines (DuckDB, ClickHouse). Example: the functions giving the first and last day of a week. The enriched query is then compiled to the dialect of the targeted source.
- Other possible languages, which map well onto the logical plan because they are pipelines of relational operations:
  - **Pipe SQL syntax** (`FROM t |> WHERE ... |> AGGREGATE ...`). The original note said "PSQL"; this reading is an interpretation, not confirmed. **[to confirm]**
  - **PRQL**: each step maps almost one to one to a plan node. It has a notion of target dialect at compile time. Set a subset, as the specification evolves.
  - **Tutorial D** (see section 5).
- **One language per query**, chosen by the user according to their comfort. No mixing in a single query. The choice of language has no effect after parsing.
- Suggested order: enriched SQL, then pipe syntax (cheap, validates that the plan does not depend on the form of the SQL), then PRQL (a real paradigm shift).
- A document-oriented language, with nested structures, does not fit relational algebra without flattening the data or handling nested types, which touches the type model.

## 3. Architecture

Common pipeline, where **the logical plan is the pivot** and depends on no language:

1. **Parser** (chosen by the user): input dialect to AST, then plan builder to the logical plan.
2. **Analysis**: resolution of tables and columns, types, function catalog, read / write / DDL classification.
3. **Optimiser**: written once, works only on the logical plan.
4. **Physical plan**: for each join, semi-join, merge join or local hash join, depending on indexes and statistics.
5. **Execution**: subqueries sent in parallel, results streamed, minimal local assembly.
6. **Emitters**: one per source, which translates the plan (or a pushed-down subtree) into the source's dialect.

An **EXPLAIN** command is planned from the start: optimised logical plan, SQL generated for each source, what stays local. It is the best debugging and explanation tool for the user.

## 4. Optimiser

**Rewrite rules**

- **Pushdown** of filters, projections, aggregates and limits to the source.
- **Aggregation before join** (*eager aggregation*): pre-aggregate on the source side, grouping on the join keys. Valid for `SUM`, `COUNT`, `MIN`, `MAX`, and `AVG` through `SUM` and `COUNT` (the `COUNT` must be on the column, not on `*`). **Invalid** for `COUNT(DISTINCT)` and when the join or an `Ungroup` multiplies rows without correction. Not always worth it: decided by the statistics.
- **Join reordering**: start with the one that reduces the volume the most.
- **Grouping by source**: a subtree that only touches one source goes as a whole, in a single query.
- **Rewriting functions as ranges** to keep the use of indexes: `WHERE date_trunc('week', col) = X` becomes `col >= start AND col < end`. Volatile functions like `current_date` are evaluated only once by Prisme.

**Join strategies across sources**

- **Batched semi-join**: fetch the keys of the small part, then query the other source in batches (`IN` of a few thousand values). Each batch uses the key's index. Variant: Bloom filter.
- **Streaming merge join**: if the key is indexed on both sides, ask both sources for `ORDER BY key` with unbuffered reads and in parallel, then merge both streams in near-constant memory. The order returned by the sources must be identical to Prisme's comparison order (collations, position of NULLs), otherwise rows are silently skipped.
- **Local hash join**, only on already reduced results, with a memory ceiling and a spill to disk (sorted temporary files) beyond it.
- Golden rule at this scale: **never fetch raw rows**. Each source reduces its volume before Prisme touches it.
- Results always streamed to the client (pages or cursor), never fully materialised in memory.

**Statistics**

- A `COUNT(*)` over hundreds of millions of rows is slow: use `information_schema` (estimates) and index cardinality.
- Size of a CSV known, row count estimated by sampling.
- Statistics must count NULLs (rate per column) to estimate selectivity.

**Development strategy**

- Optimiser v1: fixed rules (filters, projections, grouping by source), then aggregation before join when valid, then semi-join. The merge join comes next, then cost-based choices.
- Existing building blocks to know: Apache DataFusion (Rust) and Calcite (Java) have a parser, an optimiser and rules, including aggregation pushed below joins; Trino does all of it but is heavy. Dependencies are avoided, so the model of their logical plan serves as a reference. DataFusion remains an option for the executor when the time comes.
- Most of the work is not parsing the text (a few milliseconds) but transferring and processing the rows.

## 5. Logical plan and Tutorial D

### 5.1 Plan nodes

Nodes carry an **output schema** (columns, type, nullability), a row estimate and, after optimisation, the execution source (or `local`). The duplicate mode is **explicit in the name**, without a flag to interpret.

- **Source**: `Scan` (source, table or file, columns read, CSV options, pushed filter and limit).
- **Unary**: `Filter` (boolean predicate, three-valued logic), `Project`, `Extend` (computed columns), `Rename`, `Sort` (keys, direction, position of NULLs; a sort is never assumed kept after another node), `Limit`, `Distinct` (NULLs considered equal).
- **Joins**: `InnerJoin`, `LeftJoin`, `RightJoin`, `FullJoin`, `SemiJoin` (MATCHING) and `AntiJoin` (NOT MATCHING). The output schema of a `SemiJoin` or an `AntiJoin` is that of the left side. They never multiply rows. A NULL key never matches.
- **Sets**:

| Node | SQL equivalent | Duplicates |
|---|---|---|
| `UnionAll` | `UNION ALL` | kept (sum of occurrences) |
| `UnionDistinct` | `UNION` | removed |
| `MinusAll` | `EXCEPT ALL` | max(0, n − m) occurrences |
| `MinusDistinct` | `EXCEPT` | removed |
| `IntersectAll` | `INTERSECT ALL` | min(n, m) occurrences |
| `IntersectDistinct` | `INTERSECT` | removed |

- **Aggregation and nesting**: `Aggregate` (keys and functions, optional `DISTINCT`; NULLs form a single group), `Group`, `Ungroup` (multiplies rows), `Wrap`, `Unwrap`.
- **Writes**, always at the root and to a single target: `Insert`, `Update`, `Delete`, `Export`. The read / write classification is read on the root.

**Invariants**

- The plan is a tree, without cycles.
- A rewrite never changes the output schema of the root.
- A node pushed to a source only contains functions marked "pushable" for that source.
- Derived properties (nullability, key uniqueness, order) are computed on the plan, never on the original text.

In the input languages: in SQL, a bare `UNION` stays accepted as in standard SQL (= `UnionDistinct`), with the explicit forms `UNION DISTINCT` and `EXCEPT DISTINCT` in addition. On the source side, `MinusAll` and `IntersectAll` do not exist everywhere: the catalog says per source whether the node is pushable, otherwise Prisme runs it locally (counting with a hash table).

### 5.2 Tutorial D as a vocabulary

The project sticks to **C. J. Date's theory**. Tutorial D (the teaching language of Date and Darwen's *Third Manifesto*) provides the vocabulary of the logical plan: `JOIN`, `UNION`, `MINUS`, `WHERE`, projection, `RENAME`, `EXTEND`, `SUMMARIZE`, `GROUP` / `UNGROUP` (relation-valued attributes), `WRAP` / `UNWRAP` (tuple-valued attributes), `MATCHING` and `NOT MATCHING` (the semi-join is a first-class operator), `TCLOSE` (transitive closure). Its grammar is small and regular, hence easy to parse. Reference implementations: Rel (Java) and Dee (Python).

In the plan, `Group` and `Ungroup` are real nodes, not opaque functions (`array_agg`, `UNNEST`): the optimiser can reason about them. The nested relation stays a relation (closure), so there is no parallel world for nested values.

### 5.3 Flex mode and strict mode

Two decisions forced by Date's purity:

- **Sets or bags**: Date requires relations without duplicates, SQL sources return bags. **Decision: bags internally**, to stay faithful to the sources and avoid a `DISTINCT` at the input over hundreds of millions of rows.
- **NULL**: Date rejects them. Sources are full of them, and turning a NULL into 0 or -1 distorts the meaning of the data. **Decision: NULLs are kept.**

Purity would be preferable, but the sources are too heterogeneous. Hence two modes, chosen per query:

- **Flex**: a dialect derived from Tutorial D, aligned with the SQL semantics of the sources (three-valued logic, bags, nullable attributes like `INTEGER?`, `IS NULL`, `COALESCE`, `NULLIF` operators). `UNION ALL` and `MINUS ALL` are flex extensions. To be documented precisely so as not to mislead those who know the original.
- **Strict**: a simple **verification layer** on the same plan. The analyser rejects the query if an attribute can be NULL or if the result can have duplicates, unless handled explicitly (`COALESCE`, `DISTINCT`, column declared non-null). Only the `DISTINCT` forms of set operators are accepted there. Execution is identical.
- **The strict mode is postponed** (not critical). What is critical right now: keep the **nullability** information in the types and in the plan.

## 6. Type model

- Internal types: integer, exact decimal, string, boolean, date/time, NULL, with nullability. To be set first, as everything depends on it. The points below carry the behaviour measured on MySQL 8.4.11, MariaDB 10.6.28 and MariaDB 11.8.9, and a proposal for the v1, to decide before the plan is written (`docs/DEV-DIALECTS.md`, step 0). In the v1, the server runs the whole query: Prisme compares and computes nothing itself yet.
- **Integers**: a MySQL `BIGINT UNSIGNED` exceeds the signed 64-bit integer. A computation that overflows raises an **explicit error**, without silently switching to floating point. Pushdown often solves the problem: if the sum is done on the MySQL side, only one result is left to carry.
  - Measured: `BIGINT UNSIGNED` reaches 18446744073709551615. `18446744073709551615 + 1`, `9223372036854775807 + 1` and `CAST(1 AS UNSIGNED) - 2` give the error 1690 "value is out of range" on the three servers.
  - Proposal: the plan keeps the declared type (1, 2, 3, 4 or 8 bytes, signed or not), as a transpilation keeps the result type. The integer values of the plan (literals, constants computed by Prisme) are held in an `i128`, which holds every integer of the MySQL family, signed and unsigned, without a big integer library; the earlier lead read out-of-range integers as strings. An overflow gives the explicit error of the engine. **[to decide]**
- **Decimals**: exact, never as floating point.
  - Measured: `0.1 + 0.2 = 0.3` gives 1; `1 / 3` gives `0.3333`, a `DECIMAL` with 4 decimals (`div_precision_increment`). Precision up to 65 digits on both engines; scale up to 30 on MySQL, which refuses `DECIMAL(65, 38)` ("Too big scale 38 ... Maximum is 30"), and up to 38 on MariaDB.
  - Proposal: the type carries the precision and the scale. A decimal value of the plan keeps the exact text of its literal; the representation for computing is chosen with the first computation done by Prisme (65 digits exceed the 38 digits of an `i128`). A scale above 30 towards MySQL is an explicit error. **[to decide]**
- **Booleans**: integers in the MySQL family, see section 7, point 2.
- **Strings**: `'a' = 'A'` depends on the source's collation. A single rule is to be set.
  - Measured on a column with the default collation of each server, holding `'a'`, `'a '`, `'A'`, `'e'`, `'é'`: MySQL 8.4 (`utf8mb4_0900_ai_ci`, NO PAD) finds 2 rows for `v = 'a'` and `COUNT(DISTINCT v)` gives 3; MariaDB 10.6 (`utf8mb4_general_ci`) and 11.8 (`utf8mb4_uca1400_ai_ci`), PAD SPACE, find 3 rows and give 2. The connector negotiates `utf8mb4_general_ci` for the connection, so two literals compare with it: `'a' = 'a '` gives 1 even on MySQL. `utf8mb4_0900_ai_ci` exists in MariaDB 11.8, with the same results on these examples, and not in MariaDB 10.6.
  - Proposal for the v1: the plan carries the character set and the collation of each string. Comparisons follow the collation the server applies, that of the column or of the connection; the transpiler neither changes nor emulates it. A `COLLATE` written in the query is kept when the target has the collation, otherwise it is an explicit error. The single rule for the comparisons made by Prisme itself is set with the federated engine. **[to decide]**
- **Dates**: a single time zone convention for all.
  - Measured: a `TIMESTAMP` written at `+00:00` and read at `+02:00` moves by two hours, a `DATETIME` does not (three servers). `time_zone` is `SYSTEM` by default, and the connector sets nothing. `CAST('0000-00-00' AS DATE)` gives NULL with a warning on MySQL (`NO_ZERO_DATE` in its default `sql_mode`) and `0000-00-00` on MariaDB (absent from its default `sql_mode`).
  - Proposal for the v1: distinct types `Date`, `Time`, `DateTime` (local, without time zone), `Timestamp` (instant, shown in the time zone of the session) and `Year`, with their fractional seconds. Prisme leaves the time zone of the session as it is and converts nothing; a date value of the plan keeps its text, zero dates included. The single convention is set with the federated engine, when Prisme compares instants coming from several sources. **[to decide]**
- **JSON**: MySQL has a native type, MariaDB an alias.
  - Measured: on MySQL, `CAST('{"a":1}' AS JSON) = CAST('{"a": 1}' AS JSON)` gives 1 (comparison of JSON values), `JSON_EXTRACT` returns a `JSON`, `->` and `->>` exist. In MariaDB, `JSON` is a `LONGTEXT` with a `CHECK (json_valid(...))`: `information_schema.COLUMNS` says `longtext` and `CHECK_CONSTRAINTS` gives ``json_valid(`doc`)``. `CAST(... AS JSON)`, `->` and `->>` are syntax errors on MariaDB 10.6 and 11.8, and `JSON_EXTRACT` returns a string marked `format=json`. `JSON_EQUALS` exists in MariaDB 11.8, neither in MariaDB 10.6 nor in MySQL.
  - Proposal: a `Json` type in the plan; on MariaDB, the catalog recognises a JSON column by its `json_valid` check. Towards MariaDB, `doc->'$.a'` becomes `JSON_EXTRACT(doc, '$.a')` and `doc->>'$.a'` becomes `JSON_UNQUOTE(JSON_EXTRACT(doc, '$.a'))`, their definitions in MySQL. A comparison of two JSON values is an explicit error on MariaDB 10.6; from the version that brings `JSON_EQUALS`, that function is a candidate, to check against the comparison rules of MySQL in the function catalog (`docs/DEV-DIALECTS.md`, step 7). **[to decide]**
- **Nested values**: reserve `LIST` and `STRUCT` in the model from the start, including multi-value attributes (MVA). The strict relational model (classic 1NF) forbids multiple values, but Date argues that "atomic" depends on the operators provided and defends relation-valued attributes (RVA), compatible with the relational model. A SQL array (ordered, with duplicates) is not an RVA (set without order or duplicates), which matters for the equivalence of rewrites. The "MVA" of Manticore and Sphinx is actually a list of integers. Engine v1: list (more familiar to DuckDB and ClickHouse users), keeping the door open to nested relations. Minimum to implement: `array_agg`, `UNNEST` and the membership test.
- MySQL and MariaDB have no real array type: `array_agg` becomes `JSON_ARRAYAGG`, `UNNEST` becomes `JSON_TABLE`, `list_contains` becomes `MEMBER OF` or `JSON_CONTAINS`.

## 7. NULL: list of points to settle

Reference in flex mode: the behaviour of SQL sources. **Priority**: points 1 to 6 are essential from the start (they affect the correctness of results and joins, and therefore already the v1), points 7 to 9 are decided with the function catalog, points 10 to 12 with the workspace and the optimiser.

Points 1 to 6 carry the behaviour measured on MySQL 8.4.11, MariaDB 10.6.28 and MariaDB 11.8.9, identical on the three servers, and a proposal, to decide before the plan is written (`docs/DEV-DIALECTS.md`, step 0). The other points are not settled yet. Tables of the measures: `l (k)` holds 1, 2, NULL; `r (k)` holds 1, NULL; `s1 (a)` holds NULL, NULL, NULL, 1, 1; `s2 (a)` holds NULL, 1.

1. **Equality**: comparison equality (`=`, joins: `NULL = NULL` gives unknown) and identity equality ("not distinct", `IS NOT DISTINCT FROM`, `<=>` in MySQL). Name both in the plan and say which one each node uses.
   - Measured: `NULL = NULL` gives NULL, `NULL <=> NULL` gives 1, `1 <=> NULL` gives 0. `IS NOT DISTINCT FROM` is a syntax error on the three servers: `<=>` is the only spelling of identity equality in the MySQL family.
   - Decided: two operators in the plan, `Eq` (comparison, three-valued) and `NotDistinct` (identity, never unknown). A join or a `Filter` uses the one written in the query; `Distinct`, the grouping keys of `Aggregate` and the set operators use `NotDistinct`. The MySQL and MariaDB emitters write `=` and `<=>`, the PostgreSQL and DuckDB emitters `IS NOT DISTINCT FROM`, the SQLite emitter `IS`. **[decided]**
2. **Three-valued logic**: truth tables of `AND`, `OR`, `NOT`; `Filter` rejects unknown; `CHECK` accepts unknown; an unknown `CASE WHEN` is not true.
   - Measured: `NULL AND 0` gives 0, `NULL AND 1` NULL, `NULL OR 1` 1, `NULL OR 0` NULL, `NOT NULL` NULL, `NULL XOR 1` NULL: the truth tables of Kleene. `WHERE NOT (a = 1)` drops the rows where `a` is NULL; `CASE WHEN NULL = 1` takes the `ELSE` branch; `CHECK (a > 0)` accepts NULL and refuses -1. Booleans are integers: `TRUE` is 1, `(1 = 1) + 1` gives 2, `2 AND 3` gives 1. A predicate in the select list is a `BIGINT` on MySQL and an `INT` on MariaDB.
   - Decided: a `Boolean` type with three values (true, false, unknown) and the tables of Kleene; `Filter` and `CASE WHEN` keep true only. The plan builder of the MySQL family types a predicate as `Boolean` and adds an explicit conversion where the query mixes booleans and numbers: `Boolean` to integer in `SUM(a = 1)` or `(a = 1) + 1`, number to `Boolean` (true when non-zero) in `WHERE a` or `2 AND 3`. The MySQL and MariaDB emitters write nothing for these conversions; an emitter towards an engine with a real boolean type (PostgreSQL, DuckDB) writes them. **[decided]**
3. **Joins**: a NULL key never matches (`SemiJoin` included). `AntiJoin` keeps a NULL key on the left. Pitfall: `NOT IN` with a NULL in the subquery returns nothing. Should the SQL parser reproduce it as is or translate it into an `AntiJoin`? Both are not equivalent. Option: join on identity equality.
   - Measured: `l JOIN r ON l.k = r.k` gives 1 row, with `<=>` 2 rows. `k NOT IN (SELECT k FROM r)` gives no row: per row of `l`, the predicate gives 0 for 1, NULL for 2 and NULL for NULL. `NOT EXISTS (SELECT 1 FROM r WHERE r.k = l.k)` gives 2 and NULL. `k NOT IN` a subquery without NULL gives 2 only, as the NULL of `l` is dropped; `k NOT IN` an empty subquery gives 1, 2 and NULL.
   - Decided: the plan keeps `NOT IN` as written, a three-valued `In` predicate (negated) inside a `Filter`, with its SQL semantics. It never becomes an `AntiJoin` silently, as the results differ (above). `NOT EXISTS` gives an `AntiJoin`. The optimiser turns a `NOT IN` into an `AntiJoin` only when the nullability proves both sides non-null (point 8), with a step in the report. **[decided]**
4. **Set operators**: duplicates according to identity, NULL included, for `Distinct`, `Union*`, `Minus*`, `Intersect*`. Check that the sources follow this rule, `EXCEPT ALL` included.
   - Measured: `SELECT DISTINCT a FROM s1` gives 2 rows, `SELECT NULL UNION SELECT NULL` 1 row. Between `s1` and `s2`, `INTERSECT` and `INTERSECT ALL` give NULL and 1, `EXCEPT` gives no row, `EXCEPT ALL` gives NULL, NULL and 1. The three servers follow the rule, `EXCEPT ALL` included. MySQL has `INTERSECT` and `EXCEPT` from 8.0.31.
   - Decided: `Distinct` and the six set nodes compare whole rows with `NotDistinct`, as the engines do. Towards a MySQL older than 8.0.31, `INTERSECT` and `EXCEPT` are emulated or refused, settled with the emitters. **[decided]**
5. **Aggregation**: `GROUP BY` groups all NULLs; functions ignore NULLs except `COUNT(*)`; aggregate over an empty or all-NULL set: `SUM`, `MIN`, `MAX`, `AVG` give NULL, `COUNT` gives 0; `COUNT(DISTINCT)` ignores NULLs; does `array_agg` keep NULLs? (varies across engines, to set).
   - Measured: on `s1`, `GROUP BY a` gives two groups, NULL (3 rows) and 1 (2 rows). On an empty input, `SUM`, `MIN`, `MAX` and `AVG` give NULL, `COUNT(a)` and `COUNT(*)` 0; with a `GROUP BY`, an empty input gives no row. On the three NULLs: `SUM` gives NULL, `COUNT(a)` 0, `COUNT(*)` 3. `COUNT(DISTINCT a)` gives 1; `COUNT(DISTINCT k, a)` leaves out the pairs that hold a NULL. `JSON_ARRAYAGG(a)` keeps the NULLs (`[null, null, null, 1, 1]`), `GROUP_CONCAT(a)` drops them (`1,1`). `SUM` of an `INT` is a `DECIMAL`, as is `AVG` (4 decimals); `SUM` of three `BIGINT UNSIGNED` at their maximum gives 55340232221128654845, without overflow.
   - Decided: `Aggregate` follows these rules, those of standard SQL. Without grouping keys, it gives one row even on an empty input, and its results are then nullable even on a non-null column. The list aggregate of Prisme (`array_agg`) keeps the NULLs, like `JSON_ARRAYAGG` in both engines and `array_agg` in standard SQL and PostgreSQL. The result type of each aggregate comes from the function catalog, per dialect. **[decided]**
6. **Sorting**: position of NULLs (MySQL and MariaDB first in ascending order, PostgreSQL and DuckDB last). A reference rule for Prisme, and an explicit `NULLS FIRST` / `NULLS LAST` emitted to the sources, emulated if needed. Crucial for the merge join.
   - Measured: NULLs come first in ascending order and last in descending order: NULL is the smallest value. `NULLS FIRST` and `NULLS LAST` are syntax errors on the three servers. The emulation `ORDER BY a IS NULL, a` puts the NULLs last but loses the order of the index: on an indexed column, `EXPLAIN` gives `Using index` for `ORDER BY a` and `Using index; Using filesort` for the emulation. PRQL does not say where NULLs go (`sort` in the PRQL book).
   - Decided: each key of `Sort` carries its NULL position, always explicit. The plan builder takes it from the input dialect when the query does not write it: smallest value for MySQL and MariaDB. Reference rule of Prisme, for PRQL and for the operations of Prisme (local sort, merge join): NULL is the smallest value, as in MySQL, MariaDB and SQLite. An emitter writes nothing when the target gives the same position by default or when the column is non-null. Otherwise, it writes `NULLS FIRST` or `NULLS LAST` when the target has them; towards MySQL and MariaDB, it adapts with an emulation (`ORDER BY a IS NULL, a` for NULLs last in ascending order, `ORDER BY a IS NOT NULL, a DESC` for NULLs first in descending order), and when the emulation loses the order of an index, the report states the loss. **[decided]**
7. **Functions**: a NULL argument gives NULL, except for exceptions to list (`COALESCE`, `NULLIF`, `IS NULL`, `CONCAT` versus `CONCAT_WS` versus `||`). Division by zero: NULL or error? Failed casts: error, or NULL through a `TRY_CAST` variant?
8. **Types and nullability**: propagation by each node (`LeftJoin` makes the right columns nullable, `Aggregate` without a group can produce a NULL, `UnionAll` is nullable if one side is). A `Filter` on `IS NOT NULL` makes the column non-null in the rest of the plan.
9. **Nesting**: a NULL list and an empty list are different. `Ungroup` of a NULL or empty list: zero rows, or one row with NULL (`UNNEST` versus `LEFT JOIN UNNEST`)? NULL element in a list: kept?
10. **CSV and workspace**: NULL markers on reading (see section 8) and on writing (empty, `\N`, `NA`). SQLite allows several NULLs in a `UNIQUE` column.
11. **Optimiser**: each rule preserves NULL semantics. Pushing a filter below a `LeftJoin` is only valid on the right side; turning a `LeftJoin` into an `InnerJoin` requires a filter that rejects the right NULLs; `NOT (a = b)` is not `a IS DISTINCT FROM b`.
12. **Strict mode**: postponed, but the nullability of point 8 must exist.

No `ORDER BY`: the order is not guaranteed, as in any database. **Decision: the engine does not add a deterministic sort on its own.**

## 8. CSV

Principle: **everything is text**, without inference. Typing goes through the `CAST`s written in the query rather than a modelling phase (one schema per file). The source declaration is still planned.

**Source declaration** (like a MySQL connection): path (in a folder **allowed per role**, to prevent `../../etc/...`), separator (`,` by default, adjustable, no auto-detection in v1), encoding (UTF-8 by default, BOM tolerated, others by option), header (present by default, otherwise columns `c1`, `c2`...; a duplicate header is an error), `"` quotes with doubled quote (RFC 4180), CRLF, LF or CR line endings.

**Values**

- No NULL by default: an empty cell gives an empty string.
- Option on the **scan**: `WITH EMPTY AS NULL`, and the general form `WITH NULL AS 'NA'` (`EMPTY AS NULL` is a shortcut for `NULL AS ''`). Adjustable on the source declaration, overridable in the query. Reasons: the conversion happens on reading, before filtering and joining, the optimiser knows the columns are nullable, and `CAST('' AS INTEGER)` fails whereas it passes with a NULL. One-off alternative: `NULLIF(col, '')`.
- RFC 4180 says nothing about NULL: `,,` and `,"",` are two ways of writing the same thing there. Tools diverge (PostgreSQL: `,,` gives NULL and `,"",` gives an empty string; MySQL: `\N`; DuckDB: `nullstr`; Excel and pandas: empty is NULL or NaN). Sources quoted from memory, to check. By default, `,,` and `,"",` both give an empty string.
- The "quotes = empty string, bare empty = NULL" mode (like PostgreSQL) is **postponed**, but **the reader must keep the "quoted field" information internally**, otherwise it can never restore it.
- No inference of booleans, nor quotes as a type signal: quotes carry no reliable meaning. Explicit conversion with `CAST` or `CASE`.

**Errors and execution**

- Column count different from the header: error with the line number. `ON ERROR SKIP` option to ignore and count.
- Reading as a generator, filter and projection applied while reading.
- The CSV reader follows RFC 4180 for escaping (doubled quote), without treating the backslash as an escape character by default. To be covered by tests.
- Statistics: file size, rows estimated by sampling.
- For large files, an explicit **import into the workspace** command (SQLite with indexes), to read only once a CSV joined several times.

## 9. Workspace (materialisation)

Use: analysis, with several iterations on the same dataset fetched from several sources, during one or two working days. No cache to invalidate, no automatic reuse of results (query matching).

- A workspace is a **SQLite file per user or per session**, with a disk quota and cleanup.
- The user explicitly saves a result under a name (`CREATE TABLE ws.sales AS ...`), then iterates on it with queries that are no longer federated at all: a single local source, indexed.
- The workspace is **one more source** for the optimiser, the best one: exact statistics, no network latency.
- Each dataset keeps its **provenance** (original query, sources, date), with a possible refresh.
- It is a **disposable cache**, rebuildable from the sources: no transactions, backups or migrations to handle.
- Limits: SQLite is row-oriented, so aggregations over tens of millions of rows are fine with good indexes but not as fast as a columnar engine. Batched inserts in a transaction.
- Storage is isolated behind a small interface (create, query, list, delete): it can be replaced by a columnar engine later.
- A real in-house database system is not ruled out later.

## 10. Writes, roles and security

- **One source read, one destination written.** No writes to several destinations in a single query (no use case). Useful examples: import (`INSERT INTO mysql.clients SELECT ... FROM csv.clients_2026`), export to a CSV, copy or migration from one database to another. Not atomic across two sources, the user is warned.
- **CSV**: read, plus export of a result to a new file. Appending to the end of a file would come next. `UPDATE` and `DELETE` on CSV (full rewrite of the file, lock) only if a real need appears.
- **Roles**: some users read-only, others with administration rights on the servers (deletion, updates). Classification by the AST, distinct connection accounts per role on the source side, encrypted credentials, audit log.
- **Multi-user**: workspace per user with a quota, long queries in long-lived workers, cancellation possible.

## 11. Enriched functions: catalog

- Each function of the catalog has a definition per dialect: native, emulated, or **local** (evaluated by Prisme after fetching, useful in the federated engine). The catalog tells the optimiser what can be pushed down, otherwise it would send a filter the source cannot run.
- The semantics of each function are defined once for the reference dialect (week starting on Monday or Sunday, NULL, integer division, time zones) and checked by identical tests on each source.
- Start with a small catalog (dates, strings, conditionals), then add on demand. Whether PRQL and pipe SQL inherit the reference semantics is still to be settled.

## 12. Query examples

The source names (`mysql_a`, `csv`) are examples.

**1. Enriched SQL: MySQL and CSV join**
```sql
SELECT c.name, SUM(o.amount) AS total
FROM mysql_a.orders o
JOIN csv('clients.csv') c ON c.id = o.client_id
WHERE o.order_date >= DATE '2026-01-01' AND c.country = 'FR'
GROUP BY c.name
```
Target plan: the CSV is read with the `country = 'FR'` filter applied on reading; its `id`s go in batches to MySQL (semi-join); MySQL runs `SELECT client_id, SUM(amount) ... WHERE order_date >= ... AND client_id IN (...) GROUP BY client_id` (aggregation before join); Prisme joins this small result to the CSV then redoes a `SUM` per `name`, valid for a `SUM`.

**2. The same query in flex Tutorial D**
```
SUMMARIZE (
  (mysql_a.orders WHERE order_date >= DATE('2026-01-01'))
  JOIN
  ((csv.clients WHERE country = 'FR') RENAME {id AS client_id})
) PER {name} ADD (SUM(amount) AS total)
```
The `JOIN` is natural (on the attributes with the same name), hence the `RENAME`. The logical plan is identical to that of example 1.

**3. Pipe syntax with an enriched function**
```sql
FROM mysql_a.orders
|> WHERE date_trunc('week', order_date) = date_trunc('week', current_date)
|> AGGREGATE SUM(amount) AS total GROUP BY client_id
```
`current_date` is evaluated once. The filter becomes a date range (`order_date >= '2026-09-28' AND order_date < '2026-10-05'` for a run on 2 October 2026), which lets MySQL use its index.

**4. Workspace and nesting**
```sql
CREATE TABLE ws.orders_fr AS
  SELECT o.client_id, o.product FROM mysql_a.orders o
  WHERE o.country = 'FR';

SELECT client_id, array_agg(product) AS products
FROM ws.orders_fr GROUP BY client_id;
```
The first statement is federated and writes to SQLite. The second is a local query on a single source, with a `Group` node.
