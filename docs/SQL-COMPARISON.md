# Comparison of the SQL engines

Reference of the behaviours that vary from one SQL engine to another: the SQL standard, MySQL, MariaDB, PostgreSQL, SQLite, DuckDB and ClickHouse. The dialect of the Pivoteur (ANSI SQL first, the rest borrowed from the other engines, `docs/TODO-PIVOTER.md`) decides its rules from it, and the translation between dialects finds its differences in it.

Each value is measured on the engine (`node bench/sql/run.mjs`, section 2) or quoted from a source given with its link (section 5): nothing is written from memory. The file is a reference kept up to date: a new case, a new version of an engine or a new source updates its entry, with the date of the measure.

## 1. Engines and sources

Measured on 2026-11-02:

| Engine | Versions | Where |
|---|---|---|
| MySQL | 8.0.11, 9.7.4, 26.10.0 | servers `mysql-oldest`, `mysql-lts`, `mysql-latest` of `compose.yaml` |
| MariaDB | 10.6.3, 12.3.3, 13.0.2 | servers `mariadb-oldest`, `mariadb-lts`, `mariadb-latest` of `compose.yaml` |
| PostgreSQL | 18.6 | container `prisme-probe-postgres-18.6` (image `postgres:latest` of 2026-10-06) |
| SQLite | 3.53.4 | `node:sqlite` of Node 26.7.0, in memory; Prisme bundles SQLite 3.53.2 (`libsqlite3-sys` 0.38.2) |
| DuckDB | 2.0.0 | `@duckdb/node-api` 2.0.0-r.1, in memory |

Settings the values depend on, as the engines start (cases `info.*`):

| Setting | MySQL | MariaDB | PostgreSQL | DuckDB |
|---|---|---|---|---|
| Collation of the connection | `utf8mb4_0900_ai_ci` | `utf8mb4_general_ci` (10.6.3), `utf8mb4_uca1400_ai_ci` (12.3.3, 13.0.2) | `en_US.utf8` (collation of the database) | none (`default_collation` empty) |
| Pad attribute of that collation | `NO PAD` | no column `PAD_ATTRIBUTE` (10.6.3), `PAD SPACE` (12.3.3, 13.0.2) | | |
| `sql_mode` | `ONLY_FULL_GROUP_BY, STRICT_TRANS_TABLES, NO_ZERO_IN_DATE, NO_ZERO_DATE, ERROR_FOR_DIVISION_BY_ZERO, NO_ENGINE_SUBSTITUTION` | `STRICT_TRANS_TABLES, ERROR_FOR_DIVISION_BY_ZERO, NO_AUTO_CREATE_USER, NO_ENGINE_SUBSTITUTION` | | |
| `group_concat_max_len` | 1024 | 1048576 | | |
| `default_week_format` | 0 | 0 | | |
| `div_precision_increment` | 4 | 4 | | |
| Names of days and months | `en_US` | `en_US` | `en_US.utf8` (`lc_time`) | |
| Time zone | `SYSTEM` (UTC in the container) | `SYSTEM` (UTC in the container) | `Etc/UTC` | `Europe/Paris` (the machine) |
| Order of NULL | | | | `NULLS_LAST` (`default_null_order`) |
| `ieee_floating_point_ops` | | | | `true` |

ClickHouse is compared from its documentation, without a server. The SQL standard (ISO/IEC 9075) is not free: its text is read in a public draft of SQL-92 and in the grammars of SQL:1999, SQL:2003 and SQL:2016, cited by these abbreviations:

- **SQL-92**: draft of July 1992 ("Second Informal Review Draft"), with its subclause, General Rule (GR) or Syntax Rule (SR);
- **BNF99**, **BNF2003**: the grammars of SQL:1999 and SQL:2003;
- **G2016**: the grammar of the Foundation of SQL:2016. A grammar gives the syntax, without the semantics: "absent from G2016" means absent from the grammar of the Foundation.

## 2. Measuring again

From the root of the repository, the development servers started (`docker compose up --detach --wait`):

```
node bench/sql/run.mjs
```

`bench/sql/cases.mjs` lists the cases: a key, the expression run by default, and the form of each engine that writes the same question otherwise (`CAST(x AS SIGNED)` in MySQL for `CAST(x AS INTEGER)`, `STRFTIME` in SQLite). An engine that refuses the default form keeps it: its error is the measure. The command starts the container of PostgreSQL when it is stopped, then stops it again; it installs `@duckdb/node-api` in `target/sql-comparison/` on its first run, outside the dependencies of Prisme. It prints the table of the values, one column per server, and writes the values, the types, the warnings and the errors to `target/sql-comparison/results.json`.

A new case goes into `bench/sql/cases.mjs`, then its values into this file, with the standard and ClickHouse when a source covers it.

## 3. Comparison by subject

Notation: `1` and `0` are the integers that MySQL, MariaDB and SQLite give for true and false, `true` and `false` the booleans of PostgreSQL and DuckDB. "warning" means a value given with a warning of the server. "error" is the error of the engine, with its code for MySQL and MariaDB. One value for MySQL or MariaDB means the same value on its three versions. The expression is the default form of `bench/sql/cases.mjs`; the column of ClickHouse comes from its documentation.

### 3.1 Comparison of texts

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `text.case` | `'FR' = 'fr'` | collation (SQL-92 4.2, 8.2) | `1` | `1` | `false` | `0` | `false` | false: bytes compared |
| `text.accent` | `'e' = 'é'` | collation | `1` | `1` | `false` | `0` | `false` | false: bytes compared |
| `text.trailing_space` | `'a' = 'a '` | true with `PAD SPACE`, false with `NO PAD` (SQL-92 8.2 GR 3) | `0` | `1` | `false` | `0` | `false` | false: a prefix is shorter |
| `text.trailing_space_bin` | `'a' = 'a ' COLLATE utf8mb4_bin` | | `1` | `1` | | | | |
| `text.trailing_space_0900_bin` | `'a' = 'a ' COLLATE utf8mb4_0900_bin` | | error 1273 on 8.0.11, `0` on 9.7.4 and 26.10.0 | error 1273 on 10.6.3, `0` on 12.3.3 and 13.0.2 | | | | |
| `text.trailing_space_nopad_bin` | `'a' = 'a ' COLLATE utf8mb4_nopad_bin` | | error 1273 | `0` | | | | |
| `text.like_case` | `'ABC' LIKE 'abc'` | collation | `1` | `1` | `false` | `1` | `false` | |
| `text.like_accent_case` | `'É' LIKE 'é'` | collation | `1` | `1` | `false` | `0` | `false` | |
| `text.order` | `'a' < 'B'` | collation | `1` | `1` | `true` | `0` | `false` | false: byte order |
| `text.least` | `LEAST('a', 'B')` | `GREATEST` / `LEAST`: SQL:2023 | `a` | `a` | `a` | `B` (`MIN`) | `B` | |
| `text.replace_case` | `REPLACE('aA', 'a', 'x')` | | `xA` | `xA` | `xA` | `xA` | `xA` | |
| `text.upper_sharp_s` | `UPPER('ß')` | `SS`, an example named by the standard according to PostgreSQL (section 5); its text is not read | `ß` | `ß` | `ß` | `ß` | `ẞ` (U+1E9E) | |
| `agg.distinct_case` | distinct aggregated texts of `fr`, `FR`, `Fr` | | `fr` | `fr` | `fr,Fr,FR` | `FR,Fr,fr` | `FR,Fr,fr` | |
| `agg.order_case` | aggregated texts of `b`, `B`, `a`, `A` in order | | `A a B b` | `A a B b` | `a A b B` | `A B a b` | `A B a b` | ordered by bytes; `COLLATE` makes the order case-insensitive |

The standard leaves case, accents and trailing spaces to the collation; the default collation of an implementation-defined repertoire is implementation-defined (SQL-92 4.2). MySQL and MariaDB compare by their collation, insensitive to case and accents by default, and differ on trailing spaces: the collation of MySQL is `NO PAD`, that of MariaDB `PAD SPACE` (section 1; MariaDB 10.6.3 does not show the attribute, and ignores trailing spaces as well). Under these collations, `DISTINCT` keeps one value of `fr`, `FR` and `Fr` (`fr` in the measure). PostgreSQL orders by the locale of the database (`en_US.utf8`: lower case first), and stays exact on equality. SQLite and DuckDB compare the bytes, as ClickHouse; `LIKE` of SQLite ignores the case of the ASCII letters only: `'É' LIKE 'é'` is false. The binary collation `utf8mb4_bin` ignores trailing spaces (`PAD SPACE`) on MySQL and MariaDB; the binary collations without padding are `utf8mb4_0900_bin` (MySQL 9.7.4 and 26.10.0, MariaDB 12.3.3 and 13.0.2) and `utf8mb4_nopad_bin` (MariaDB), and MySQL 8.0.11 has none.

### 3.2 Texts read as numbers

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `num.text_digits` | `'12' + 1` | error: operands of an arithmetic must be numeric (SQL-92 6.12 SR 4) | `13` (DOUBLE) | `13` (DOUBLE) | `13` (integer) | `13` | error: no best candidate for `+(STRING_LITERAL, INTEGER_LITERAL)` | not documented |
| `num.text_prefix` | `'12abc' + 1` | error | `13`, warning | `13`, warning | error: invalid input syntax for type integer | `13` | error | not documented |
| `num.text_letters` | `'abc' + 0` | error | `0`, warning | `0`, warning | error | `0` | error | |
| `num.text_exponent` | `'1e2' + 0` | error | `100` | `100` | error | `100.0` | error | |
| `num.text_spaces` | `' 12 ' + 0` | error | `12` | `12` | `12` | `12` | error | |
| `num.text_equals_zero` | `'abc' = 0` | error: text and number are not comparable (SQL-92 8.2 SR 3) | `1`, warning | `1`, warning | error | `0` | error: could not convert string `'abc'` to INT32 | |
| `num.text_vs_number` | `'10' > 9` | error | `1` | `1` | `true` | `1` | `true` | allowed; the side converted is not documented |
| `num.text_vs_text` | `'10' > '9'` | false (texts) | `0` | `0` | `false` | `0` | `false` | |

PostgreSQL reads an untyped literal with the type of the other operand (`'12'` as an integer), and refuses a text that is no integer. DuckDB refuses the arithmetic between a text literal and a number, but converts the text to compare it.

### 3.3 Conversions (`CAST`)

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `cast.int_letters` | `CAST('abc' AS INTEGER)` | exception 22018, invalid character value for cast (SQL-92 6.10 GR 3b) | `0`, warning (`AS SIGNED`) | `0`, warning | error | `0` | error | exception; `toInt32OrNull` gives NULL |
| `cast.int_prefix` | `CAST('12abc' AS INTEGER)` | exception 22018 | `12`, warning | `12`, warning | error | `12` | error | exception |
| `cast.int_text_fraction` | `CAST('1.5' AS INTEGER)` | as `CAST(1.5 AS INTEGER)` (SQL-92 6.10 GR 3b) | `1`, warning | `1`, warning | error | `1` | `2` | |
| `cast.int_decimal_fraction` | `CAST(1.5 AS INTEGER)` | rounding or truncation, implementation-defined (SQL-92 6.10 GR 3a) | `2` | `2` | `2` | `1` | `2` | `1`: truncation toward zero |
| `cast.int_decimal_negative_half` | `CAST(-2.5 AS INTEGER)` | implementation-defined | `-3` | `-3` | `-3` | `-2` | `-3` | `-2`: truncation toward zero |
| `cast.int_ansi_name` | `CAST('7' AS INTEGER)` | valid | error 1064: `INTEGER` refused, `SIGNED` expected | `7` | `7` | `7` | `7` | |
| `cast.decimal_letters` | `CAST('abc' AS DECIMAL(10,2))` | exception 22018 | `0.00`, warning | `0.00`, warning | error | `0` | error | |
| `cast.decimal_half` | `CAST('1.005' AS DECIMAL(10,2))` | implementation-defined | `1.01` | `1.01` | `1.01` | `1.005` (no decimal type) | `1.01` | |
| `cast.decimal_overflow` | `CAST('9999' AS DECIMAL(3,1))` | exception 22003, numeric value out of range (SQL-92 6.10 GR 3a) | `99.9`, warning: the largest value | `99.9`, warning | error: numeric field overflow | `9999` | error | |
| `cast.date_impossible` | `CAST('2026-02-30' AS DATE)` | exception 22018 (SQL-92 6.10 GR 9) | NULL, warning | NULL, warning | error: date/time field value out of range | `2026-03-02` (`DATE()`, day carried over) | error | not documented; `toDateOrNull` gives NULL out of the range of Date |
| `cast.date_letters` | `CAST('abc' AS DATE)` | exception 22018 | NULL, warning | NULL, warning | error | NULL | error | |
| `cast.date_short_parts` | `CAST('2026-1-5' AS DATE)` | | `2026-01-05` | `2026-01-05` | `2026-01-05` | NULL | `2026-01-05` | |
| `cast.date_compact` | `CAST('20260115' AS DATE)` | | `2026-01-15` | `2026-01-15` | `2026-01-15` | NULL | error: expected format is YYYY-MM-DD | |
| `cast.date_from_timestamp_text` | `CAST('2026-01-15 10:30:00' AS DATE)` | | `2026-01-15` | `2026-01-15` | `2026-01-15` | `2026-01-15` | `2026-01-15` | |
| `cast.timestamp_from_date_text` | `CAST('2026-01-15' AS TIMESTAMP)` | | `2026-01-15 00:00:00` (`AS DATETIME`) | `2026-01-15 00:00:00` | `2026-01-15 00:00:00` | `2026-01-15 00:00:00` | `2026-01-15 00:00:00` | |
| `cast.timestamp_bad_hour` | `CAST('2026-01-15 25:00:00' AS TIMESTAMP)` | exception 22018 | NULL, warning | NULL, warning | error | NULL | error | |
| `cast.try_cast` | `TRY_CAST('abc' AS INTEGER)` | absent from G2016 | error 1064 | error 1064 | error: syntax | error: syntax | NULL | `accurateCastOrNull`, `toInt32OrNull` give NULL |
| `cast.varchar_decimal` | `CAST(1.50 AS VARCHAR(10))` | | `1.50` (`AS CHAR(10)`) | `1.50` | `1.50` | `1.5` | `1.50` | |
| `cast.varchar_no_length` | `CAST(12 AS VARCHAR)` | `VARCHAR` takes a length (SQL-92 6.1) | `12` (`AS CHAR`) | `12` | `12` | `12` | `12` | |
| `cast.char_no_length` | `CAST('abc' AS CHAR)` | `CHAR(1)`: a length of 1 when it is omitted (SQL-92 6.1 SR 4) | `abc` | `abc` | `a` (character) | `abc` | `abc` | |
| `cast.varchar_date` | `CAST(DATE '2026-01-15' AS VARCHAR(20))` | `DATE '2026-01-15'`: the shortest literal of the value (SQL-92 6.10 GR 6e); later versions not read | `2026-01-15` (`AS CHAR(20)`) | `2026-01-15` | `2026-01-15` | `2026-01-15` (`DATE('2026-01-15')`) | `2026-01-15` | |
| `cast.varchar_short_text` | `CAST('abc' AS VARCHAR(2))` | `ab`, warning 01004, string data, right truncation (SQL-92 6.10 GR 6c) | `ab`, warning (`AS CHAR(2)`) | `ab`, warning | `ab` | `abc` | `abc` | |
| `cast.varchar_short_number` | `CAST(123 AS VARCHAR(2))` | exception 22001, string data, right truncation (SQL-92 6.10 GR 6a) | `12`, warning (`AS CHAR(2)`) | `12`, warning | `12` | `123` | `123` | |
| `cast.varchar_short_date` | `CAST(DATE '2026-01-15' AS VARCHAR(4))` | exception 22001 (SQL-92 6.10 GR 6e) | `2026`, warning (`AS CHAR(4)`) | `2026`, warning | `2026` | `2026-01-15` (`DATE('2026-01-15')`) | `2026-01-15` | |
| `cast.char_padding` | `CONCAT(CAST('a' AS CHAR(3)), '\|')` | completed with spaces (SQL-92 6.10 GR 5c); no `CONCAT` | `a\|` | `a\|` | `a  \|` | `a\|` | `a\|` | |

MySQL and MariaDB never stop on an invalid value: they give 0 or NULL, with a warning, and keep the prefix of a text that starts with digits. PostgreSQL and DuckDB raise an error, as the standard. SQLite converts what it can, without a warning. DuckDB offers `TRY_CAST` for NULL; ClickHouse, the functions `…OrNull`, `…OrZero` and `…OrDefault`.

Beyond the length of a `CAST`, the standard cuts a text with a warning, and stops on a number or a date. MySQL, MariaDB and PostgreSQL cut all three, the first two with a warning; SQLite and DuckDB ignore the length.

### 3.4 Division and integers

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `div.by_zero` | `1 / 0` | exception 22012, division by zero (SQL-92 6.12 GR 4) | NULL, warning | NULL, warning | error: division by zero | NULL | `inf` (DOUBLE) | `inf` |
| `div.modulo_zero` | `MOD(1, 0)` | `MOD`: SQL:1999 (BNF99) | NULL, warning | NULL, warning | error | NULL (`1 % 0`) | error: division by zero | exception |
| `div.integers` | `7 / 2` | exact numeric, precision and scale implementation-defined (SQL-92 6.12 SR 1d) | `3.5000` (DECIMAL) | `3.5000` (DECIMAL) | `3` (integer) | `3` | `3.5` (DOUBLE) | `3.5` (Float64) |
| `div.third` | `1 / 3` | implementation-defined | `0.3333` | `0.3333` | `0` | `0` | `0.3333333333333333` | |
| `div.decimal_third` | `1.0 / 3` | implementation-defined | `0.33333` | `0.33333` | `0.33333333333333333333` (numeric) | `0.33333333333333332` (REAL) | `0.3333333333333333` (DOUBLE) | |
| `div.negative_modulo` | `MOD(-7, 3)` | | `-1` | `-1` | `-1` | `-1` (`%`) | `-1` | |
| `div.decimal_modulo` | `MOD(10, 3.5)` | | `3.0` | `3.0` | `3.0` | `1.0` (`%` reads integers) | `3.0` | |
| `int.bigint_overflow` | `CAST(9223372036854775807 AS BIGINT) + 1` | exception 22003 (SQL-92 6.12 GR 5a) | error 1690 | error 1690 | error: bigint out of range | `9.2233720368547758e+18` (REAL) | error: overflow | wraps around, without an error |
| `int.beyond_double` | `CAST(9007199254740993 AS BIGINT) + 1` | | `9007199254740994` | `9007199254740994` | `9007199254740994` | `9007199254740994` | `9007199254740994` | |

The scale of a division of MySQL and MariaDB is that of the dividend plus `div_precision_increment` (4). DuckDB divides integers in floating point, as ClickHouse: `7 / 2` gives a DOUBLE, and `1 / 0` gives `inf`. Its `MOD(1, 0)` raises an error, whose message proposes `TRY(…)` to get NULL.

### 3.5 NULL

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `null.concat_pipes` | `'a' \|\| NULL` | NULL (SQL-92 6.13 GR 2a) | NULL, warning (`\|\|` is `OR`) | NULL, warning | NULL | NULL | NULL | NULL |
| `null.concat_function` | `CONCAT('a', NULL)` | no `CONCAT` (absent from G2016) | NULL | NULL | `a` | `a` | `a` | NULL |
| `null.concat_ws` | `CONCAT_WS(',', 'a', NULL, 'b')` | | `a,b` | `a,b` | `a,b` | `a,b` | `a,b` | |
| `null.greatest` | `GREATEST(1, NULL)` | NULL (SQL:2023, feature T054) | NULL | NULL | `1` | NULL (`MAX(1, NULL)`) | `1` | `1` since 24.12, NULL before |
| `null.in_list` | `1 IN (NULL, 2)` | unknown | NULL | NULL | NULL | NULL | NULL | |
| `null.is_distinct_from` | `NULL IS DISTINCT FROM NULL` | false (distinct predicate, BNF99) | error 1064 | error 1064 | `false` | `0` | `false` | |
| `null.sort_asc_first` | NULL first in `ORDER BY x` | implementation-defined, all NULLs on one side (SQL-92 13.1) | yes | yes | no | yes | no | no: values, then NaN, then NULL |
| `null.sort_desc_first` | NULL first in `ORDER BY x DESC` | implementation-defined | no | no | yes | no | no | no |
| `null.sort_nulls_last` | `ORDER BY x NULLS LAST` | SQL:2003 (BNF2003, absent from BNF99) | error 1064 | error 1064 | accepted | accepted | accepted | accepted |

MySQL, MariaDB and SQLite take NULL as the smallest value, PostgreSQL as the largest. DuckDB and ClickHouse put NULL last in both directions. PostgreSQL ignores NULL in `GREATEST` and `LEAST` on purpose, against the standard, which it says in its documentation.

### 3.6 Aggregates

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `agg.listagg` | `LISTAGG(k, ',') WITHIN GROUP (ORDER BY k)` | SQL:2016, feature T625; its argument is a text, a `<character value expression>` (10.9) | error 1064 | error 1305 (10.6.3), 1064 (12.3.3, 13.0.2) | error: function `listagg` does not exist | error: syntax | error: unknown ordered aggregate | no `LISTAGG`: `groupConcat` |
| `agg.listagg_order_inside` | `LISTAGG(k, ',' ORDER BY k)` | | error 1064 | error 1064 | error | error: no such function | `a,b` | |
| `agg.string_agg` | `STRING_AGG(k, ',' ORDER BY k)` | | error 1064 | error 1064 | `a,b` | `a,b` | `a,b` | `string_agg`, alias of `groupConcat`; its order is not documented |
| `agg.string_agg_number` | `STRING_AGG(k, ',' ORDER BY k)` of 2 and 1 | | error 1064 | error 1064 | error: function `string_agg(integer, unknown)` does not exist | `1,2` | `1,2` | |
| `agg.group_concat` | `GROUP_CONCAT(k ORDER BY k SEPARATOR ',')` | | `a,b` | `a,b` | error: syntax | `a,b` (`GROUP_CONCAT(k, ',' ORDER BY k)`) | error: syntax | `group_concat`, alias of `groupConcat` |
| `agg.default_separator` | `GROUP_CONCAT(k)` of 1 and 2 | | `1,2` | `1,2` | error: function `group_concat` does not exist | `1,2` | `1,2` | |
| `agg.group_concat_limit` | `GROUP_CONCAT(k ORDER BY k SEPARATOR ',' LIMIT 2 OFFSET 1)` of `b`, `a`, `c` | | error 1064 | `b,c` | error: syntax | error: syntax | error: syntax | `limit` argument of `groupConcat`, without offset |
| `agg.cut` | length of two texts of 600 characters aggregated | `ON OVERFLOW ERROR` by default, exception 22001 | `1024`, warning (`group_concat_max_len`) | `1201` | `1201` | `1201` | `1201` | its `limit` argument drops the values beyond |
| `agg.filter` | `SUM(k) FILTER (WHERE k > 1)` | SQL:2003, feature T612 | error 1064 | error 1064 | `5` | `5` | `5` | accepted; `sumIf` |
| `agg.avg_integers` | `AVG(k)` of 1 and 2 | exact numeric, precision and scale implementation-defined (SQL-92 6.5 GR) | `1.5000` (DECIMAL) | `1.5000` (DECIMAL) | `1.5000000000000000` (numeric) | `1.5` (REAL) | `1.5` (DOUBLE) | |
| `agg.avg_decimal` | `AVG(k)` of 1.25 and 2.50 | | `1.875000` (DECIMAL) | `1.875000` (DECIMAL) | `1.8750000000000000` (numeric) | `1.875` (REAL) | `1.875` (DOUBLE) | |
| `agg.avg_rounded` | `AVG(k)` of 1, 2 and 2 | | `1.6667` | `1.6667` | `1.6666666666666667` | `1.6666666666666667` | `1.6666666666666667` | |
| `agg.avg_rounded_negative` | `AVG(k)` of -1, -2 and -2 | | `-1.6667` | `-1.6667` | `-1.6666666666666667` | `-1.6666666666666667` | `-1.6666666666666667` | |
| `agg.avg_half` | `AVG(k)` of 1 and 31 times 0 | | `0.0313` | `0.0313` | `0.03125000000000000000` | `0.03125` | `0.03125` | |
| `agg.avg_no_value` | `AVG(k)` of NULL | | NULL | NULL | NULL | NULL | NULL | |
| `agg.avg_distinct` | `AVG(DISTINCT k)` of 1, 1 and 2 | | `1.5000` | `1.5000` | `1.5000000000000000` | `1.5` | `1.5` | |
| `agg.sum_distinct` | `SUM(DISTINCT k)` of 1, 1 and 2 | | `3` | `3` | `3` | `3` | `3` | |
| `agg.sum_distinct_scale` | `SUM(DISTINCT k)` of 1.0 and 1.00 | | `1.00` | `1.00` | `1.0` | `1.0` | `1.00` | |
| `agg.count_distinct_scale` | `COUNT(DISTINCT k)` of 1.0 and 1.00 | | `1` | `1` | `1` | `1` | `1` | |
| `agg.distinct_nan` | `COUNT(DISTINCT k)` of two `NaN` | | error 1064 (8.0.11), `1`, warning (9.7.4, 26.10.0) | error 1064 | `1` | `1` | `1` | |
| `agg.distinct_negative_zero` | `COUNT(DISTINCT k)` of `0e0` and `-0e0` | | `2` (8.0.11), `1` (9.7.4, 26.10.0) | `2` (10.6.3), `1` (12.3.3, 13.0.2) | `1` | `1` | `1` | |
| `agg.max_nan` | `MAX(k)` of `NaN` and 1 | | error 1064 (8.0.11), `1`, warning (9.7.4, 26.10.0) | error 1064 | `NaN` | `1.0` | `nan` | |
| `agg.min_nan` | `MIN(k)` of `NaN` and 1 | | error 1064 (8.0.11), `0`, warning (9.7.4, 26.10.0) | error 1064 | `1` | `0.0` | `1.0` | |
| `agg.min_text` | `MIN(k)` of `b`, `B`, `a`, `A` | | `a` | `a` | `a` | `A` | `A` | |
| `agg.max_text` | `MAX(k)` of `b`, `B`, `a`, `A` | | `b` | `b` | `B` | `b` | `b` | |

On exact numbers, MySQL and MariaDB give `AVG` at the scale of the column + 4 (`div_precision_increment`), rounded away from zero, a tie included (`agg.avg_half`). `DISTINCT` compares numbers by value: `1.0` and `1.00` are one value on every engine. `MIN` and `MAX` of texts follow the collation of the connection on MySQL, MariaDB and PostgreSQL, the code points on SQLite and DuckDB. Only PostgreSQL and DuckDB read a `NaN`: it is one value under `DISTINCT`, and comes after every number; MySQL reads it as 0 with a warning, and SQLite as 0. `0` and `-0` are one value, except on MySQL 8.0.11 and MariaDB 10.6.3.

The standard `LISTAGG` aggregates a text, takes a literal separator and `WITHIN GROUP`, both required, and ignores NULL; none of the engines measured accepts this form. DuckDB accepts `LISTAGG` with its `ORDER BY` inside the parentheses, as `STRING_AGG`.

### 3.7 Lengths

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `len.length` | `LENGTH('é')` | no `LENGTH` (absent from G2016 as a function) | `2` (bytes) | `2` | `1` | `1` | `1` | `2` (bytes) |
| `len.char_length` | `CHAR_LENGTH('é')` | `1` (SQL-92 6.6) | `1` | `1` | `1` | error: no such function | `1` | `1` (`lengthUTF8`) |
| `len.octet_length` | `OCTET_LENGTH('é')` | `2` (SQL-92 6.6) | `2` | `2` | `2` | `2` | error: no best candidate for `octet_length(STRING_LITERAL)` | `2` |
| `len.bytes` | bytes of `'é'`, in the form of each engine | `2` (SQL-92 6.6) | `2` | `2` | `2` | `2` | `2` (`STRLEN`) | `2` (`length`) |
| `len.emoji_length` | `LENGTH('😀')` | | `4` | `4` | `1` | `1` | `1` | |
| `len.emoji_char_length` | `CHAR_LENGTH('😀')` | `1` | `1` | `1` | `1` | error | `1` | |

### 3.8 Functions on texts

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `pipes.text` | `'a' \|\| 'b'` | `ab` (SQL-92 6.13) | `0`, warnings: `\|\|` is `OR` | `0`, warnings | `ab` | `ab` | `ab` | `ab` |
| `pipes.number` | `2026 \|\| '-T'` | error: the operands are texts (SQL-92 6.13 SR 1) | `1` (`OR`), warning on 9.7.4 and 26.10.0 | `1` (`OR`), warning on 10.6.3 | `2026-T` | `2026-T` | `2026-T` | |
| `text.concat_number` | `CONCAT(2026, '-T')` | no `CONCAT` | `2026-T` | `2026-T` | `2026-T` | `2026-T` | `2026-T` | |
| `text.substring_zero` | `SUBSTRING('abc' FROM 0 FOR 2)` | `a`: positions 0 and 1, of which only 1 exists (SQL-92 6.7 GR 1) | empty | empty | `a` | `a` (`SUBSTR`) | `a` | empty: an offset of 0 gives an empty string |
| `text.substring_comma_zero` | `SUBSTRING('abc', 0)` | | empty | empty | `abc` | `abc` | `abc` | empty |
| `text.position` | `POSITION('b' IN 'abc')` | `2` (SQL-92 6.6) | `2` | `2` | `2` | `2` (`INSTR`) | `2` | |
| `text.trim_both` | `TRIM(BOTH 'x' FROM 'xxaxx')` | `a` (SQL-92 6.7) | `a` | `a` | `a` | `a` (`TRIM(x, 'x')`) | `a` | |
| `text.overlay` | `OVERLAY('abcdef' PLACING 'XY' FROM 2 FOR 3)` | `aXYef` (BNF99) | error 1064 | error 1064 | `aXYef` | error: syntax | `aXYef` | |
| `text.lpad_shorter` | `LPAD('abc', 2, 'x')` | | `ab` | `ab` | `ab` | error: no such function | `ab` | |
| `text.left_negative` | `LEFT('abc', -1)` | | empty | empty | `ab`: all but the last | error: no such function | `ab` | |

`||` is `OR` in MySQL and MariaDB under their default `sql_mode` (section 1); MySQL 9.7.4 and 26.10.0 warn that this synonym is deprecated. PostgreSQL, SQLite and DuckDB write a number joined to a text by `||` as a text.

### 3.9 Booleans

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `bool.comparison` | `(1 = 1)` | `TRUE` (BOOLEAN, SQL:1999, feature T031) | `1` (BIGINT) | `1` (INT) | `true` | `1` | `true` | `1` (UInt8) |
| `bool.comparison_plus` | `(1 = 1) + 1` | error: arithmetic on numbers only | `2` | `2` | error: no operator `boolean + integer` | `2` | error | |
| `bool.true_plus_true` | `TRUE + TRUE` | error | `2` | `2` | error | `2` | error | not documented |

### 3.10 Exact numbers and rounding

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `exact.decimal_sum` | `0.1 + 0.2 = 0.3` | true: exact literals | `1` | `1` | `true` | `0` (REAL) | `true` | |
| `exact.float_sum` | the same in DOUBLE PRECISION | | `0` | `0` | `false` | `0` | `false` | |
| `exact.product_scale` | `0.12 * 0.123` | scale S1 + S2 (SQL-92 6.12 SR 1c): 5 | `0.01476` | `0.01476` | `0.01476` | `0.014759999999999999` (REAL) | `0.01476` (DECIMAL(7,5)) | |
| `exact.product_scale_wide` | `CAST(1.5 AS DECIMAL(38,20)) * CAST(1.5 AS DECIMAL(38,20))` | scale 40 | `2.25`, 30 decimals | `2.25`, 38 decimals | `2.25`, 40 decimals | `2.25` (REAL) | error: scale 40 needed, beyond the DECIMAL type | |
| `round.half_exact` | `ROUND(2.5)` | no `ROUND` (absent from G2016) | `3` | `3` | `3` | `3.0` | `3` | `3` for a Decimal: away from zero |
| `round.half_exact_negative` | `ROUND(-2.5)` | | `-3` | `-3` | `-3` | `-3.0` | `-3` | `-3` for a Decimal |
| `round.half_float` | `ROUND(2.5)` in DOUBLE PRECISION | | `2` | `2` | `2` | `3.0` | `3.0` | `2`: to even for a Float |
| `round.half_float_odd` | `ROUND(3.5)` in DOUBLE PRECISION | | `4` | `4` | `4` | `4.0` | `4.0` | `4` |
| `round.decimal_places` | `ROUND(0.125, 2)` | | `0.13` | `0.13` | `0.13` | `0.13` | `0.13` | |
| `round.float_places` | `ROUND(1.005, 2)` in DOUBLE PRECISION | | `1.00` (8.0.11), `1` (9.7.4, 26.10.0) | `1.00` | error: no `round(double precision, integer)` | `1.0` | `1.0` | |
| `round.truncate_negative` | `TRUNC(-1.999, 2)` | | `-1.99` (`TRUNCATE`) | `-1.99` | `-1.99` | error: `TRUNC` takes one argument | `-1.99` | |
| `round.tens` | `ROUND(15, -1)` | | `20` | `20` | `20` | `15.0`: negative digits read as 0 | `20` | |
| `round.ceil_negative` | `CEIL(-1.5)` | `CEILING`: SQL:2003 (BNF2003) | `-1` | `-1` | `-1` | `-1.0` | `-1` | |
| `type.power` | `POWER(2, 3)` | `POWER`: SQL:2003 (BNF2003) | `8` (DOUBLE) | `8` (DOUBLE) | `8` (double precision) | `8.0` | `8.0` (DOUBLE) | |

The standard rounds only in `CAST` and assignments, rounding or truncating as the implementation chooses (SQL-92 4.6, 6.10). In floating point, `ROUND(1.005, 2)` gives 1 to two decimals wherever it runs (PostgreSQL has no `ROUND` with digits for a `DOUBLE PRECISION`), as the double nearest 1.005 lies below it; the three versions of MySQL write the same value differently.

### 3.11 Dates

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `date.literal` | `DATE '2026-01-15'` | valid (SQL-92 5.3) | `2026-01-15` | `2026-01-15` | `2026-01-15` | error: syntax | `2026-01-15` | not documented |
| `date.literal_impossible` | `DATE '2026-02-30'` | syntax error: the natural rules of the Gregorian calendar (SQL-92 5.3 SR 22) | error 1525 | error 1525 | error | error: syntax | error | |
| `date.impossible_plus_day` | `CAST('2026-02-30' AS DATE) + INTERVAL '1' DAY` | exception 22018 | NULL, warning | NULL, warning | error | `2026-03-03` | error | |
| `date.add_month_end` | `DATE '2026-01-31' + INTERVAL '1' MONTH` | exception 22008, datetime field overflow: the day is kept, and 31 February is invalid (SQL-92 6.14 GR 3) | `2026-02-28` | `2026-02-28` | `2026-02-28 00:00:00` (timestamp) | `2026-03-03` (`DATE(…, '+1 month')`) | `2026-02-28 00:00:00` (TIMESTAMP) | |
| `date.date_add_month_end` | `DATE_ADD(DATE '2026-01-31', INTERVAL 1 MONTH)` | no `DATE_ADD` | `2026-02-28` | `2026-02-28` | error: syntax | error: syntax | `2026-02-28 00:00:00` (TIMESTAMP) | |
| `date.zero` | `CAST('0000-00-00' AS DATE)` | exception 22007: years 0001 to 9999 (SQL-92 6.1, table 10) | NULL, warning | `0000-00-00` | error | NULL | error | not documented |
| `date.zero_year` | `YEAR('0000-00-00')` | | NULL, warning (BIGINT on 8.0.11, YEAR on 9.7.4 and 26.10.0) | `0` | error | NULL | error | |
| `date.zero_dayname` | `DAYNAME('0000-00-00')` | | NULL, warning | NULL, warning | | | | |
| `date.zero_format` | `DATE_FORMAT('0000-00-00', '%Y-%m-%d')` | | NULL, warning | `0000-00-00` | | | | |
| `date.zero_datediff` | `DATEDIFF('0000-00-00', '2026-01-01')` | | NULL, warning | NULL, warning | | | | |
| `date.year_zero` | `CAST('0000-01-01' AS DATE)` | exception 22007 | `0000-01-01` | `0000-01-01` | error: out of range | `0000-01-01` | `0001-01-01 (BC)` | Date32 accepts `0000-01-01`; Date starts on 1970-01-01 |
| `date.time_beyond_day` | `TIME '25:00:00'` | invalid: hours 00 to 23 (SQL-92 6.1, table 10) | `25:00:00` (TIME) | `25:00:00` | error: field value out of range | NULL (`TIME()`) | error: time field value out of range | |
| `date.dst_hour` | `EXTRACT(HOUR FROM TIMESTAMP '2026-03-29 02:30:00')` | | `2` (`HOUR`) | `2` | `2` | `02` (`STRFTIME('%H')`) | `2` | |
| `date.equals_text` | `DATE '2026-01-15' = '2026-01-15'` | error: a date and a text are not comparable (SQL-92 4.5) | `1` | `1` | `true` | `1` (two texts) | `true` | allowed |
| `date.equals_timestamp_text` | `DATE '2026-01-15' = '2026-01-15 00:00:00'` | error | `1` | `1` | `true` | `0` (two texts) | `true` | |
| `date.equals_timestamp` | `DATE '2026-01-15' = TIMESTAMP '2026-01-15 00:00:00'` | error: comparable only with the same fields (SQL-92 4.5) | `1` | `1` | `true` | error: syntax | `true` | |
| `date.timestamp_ge_date_text` | `TIMESTAMP '2026-01-15 10:00:00' >= '2026-01-15'` | error | `1` | `1` | `true` | `1` | `true` | |
| `date.timestamp_between_date_texts` | `TIMESTAMP '2026-01-15 10:00:00' BETWEEN '2026-01-01' AND '2026-01-15'` | error | `0` | `0` | `false` | `0` | `false` | |
| `date.timestamp_between_dates` | the same, bounds `DATE '…'` | error | `0` | `0` | `false` | error: syntax | `false` | |
| `date.extract_year` | `EXTRACT(YEAR FROM DATE '2026-01-15')` | `2026` (SQL-92 6.6) | `2026` | `2026` | `2026` (numeric) | error: syntax | `2026` | `2026` |
| `date.trunc_month` | `DATE_TRUNC('month', DATE '2026-01-15')` | absent from G2016 | error 1046: read as a stored function | error 1305: no such function | `2026-01-01 00:00:00+00` (timestamp with time zone) | error: syntax | `2026-01-01 00:00:00` (TIMESTAMP) | `DATE_TRUNC`, alias of `dateTrunc`: a Date for a month |
| `date.month_label` | the month `2026-01` as a text | `CAST(d AS VARCHAR(7) FORMAT 'YYYY-MM')` (SQL:2016) | `2026-01` (`DATE_FORMAT`, `%Y-%m`) | `2026-01` | `2026-01` (`TO_CHAR`, `YYYY-MM`) | `2026-01` (`STRFTIME`) | `2026-01` (`STRFTIME`) | `formatDateTime`, alias `DATE_FORMAT`, in the style of MySQL; `%M` is the name of the month since 23.4, the minutes before |
| `date.month_label_ansi` | `CAST(DATE '2026-01-15' AS VARCHAR(7) FORMAT 'YYYY-MM')` | valid (SQL:2016, G2016) | error 1064 | error 1064 | error: syntax | error: syntax | error: syntax | |
| `date.quarter_label` | the quarter `2026-T2` as a text | | `2026-TQ` (`DATE_FORMAT`, `%Y-T%Q`): `%Q` writes a `Q` | `2026-TQ` | `2026-T2` (`TO_CHAR`, `YYYY-"T"Q`) | NULL (`STRFTIME`, `%Q`) | error: unrecognized format `%Q` (`STRFTIME`) | `%Q` of `formatDateTime`: the quarter, 1 to 4 |

Every engine measured compares a date with a text by reading the text as a date (SQLite compares two texts), and a date with a timestamp as two instants: a date upper bound excludes the hours of its day. The standard refuses both comparisons without a `CAST`.

### 3.12 Weeks and days

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `week.extract` | `EXTRACT(WEEK FROM DATE '2027-01-01')` | no `WEEK` field (SQL-92, G2016) | `0` (mode 0) | `0` | `53` (ISO) | `53` (`STRFTIME('%V')`) | `53` | ISO 8601 week, 1 to 53 |
| `week.mysql_mode0` | `WEEK('2026-01-04')`, a Sunday | | `1` | `1` | | | | `toWeek`, modes 0 to 9 |
| `week.iso_year_week` | ISO year and week of 2027-01-01 | | `2026-53` (`%x-%v`) | `2026-53` | `2026-53` (`IYYY-IW`) | `2026-53` (`%G-%V`) | `2026-53` (`%G-%V`) | |
| `week.calendar_year_week` | calendar year and ISO week of 2027-01-01 | | `2027-53` (`%Y-%v`) | `2027-53` | `2027-53` (`YYYY-IW`) | `2027-53` (`%Y-%V`) | `2027-53` (`%Y-%V`) | |
| `week.iso_zero_date` | ISO year and week of `0000-00-00` (`%x-%v`) | | NULL, warning | `0001-01` | | | | |
| `week.iso_zero_day` | ISO year and week of `2026-03-00` (`%x-%v`) | | `2026-09` | `2026-09` | | | | |
| `week.iso_zero_month` | ISO year and week of `2026-00-15` (`%x-%v`) | | `2026-613566755` | `2026-613566755` | | | | |
| `week.iso_year_zero` | ISO year and week of `0000-01-01` (`%x-%v`) | | `4294967295-52` | `4294967295-52` | | | | |
| `week.yearweek` | `YEARWEEK('2027-01-01')` | | `202652` (mode 0) | `202652` | | | `202653` (ISO) | |
| `week.day_number_monday` | number of Monday 2026-01-05 | | `2` (`DAYOFWEEK`, Sunday = 1) | `2` | `1` (`EXTRACT(DOW)`, Sunday = 0) | `1` (`%w`) | `1` (`DAYOFWEEK`, Sunday = 0) | `DOW` |
| `week.iso_day_number_monday` | ISO number of Monday 2026-01-05 | | `0` (`WEEKDAY`, Monday = 0) | `0` | `1` (`ISODOW`) | `1` (`%u`) | `1` (`ISODOW`) | `ISODOW` |
| `week.dayname` | name of the day of 2026-01-05 | | `Monday` (`DAYNAME`) | `Monday` | `Monday` (`TO_CHAR`, `FMDay`) | | `Monday` (`DAYNAME`) | |
| `week.dayname_locale` | `DATE_FORMAT('2026-01-05', '%W %M', 'fr_FR')` | | error 1582: wrong number of arguments | `lundi janvier` | | | | |

`%Y-%v` (calendar year, ISO week) gives `2027-53` for 1 January 2027, which belongs to week 53 of 2026: the year of an ISO week is `%x` in MySQL, `IYYY` in PostgreSQL, `%G` in SQLite and DuckDB.

MySQL and MariaDB give nothing usable for the ISO week of a date without a day in the calendar: `0000-00-00` differs from one to the other, `2026-00-15` gives a week out of range, and the year -1 of `0000-01-01` comes back as an unsigned underflow (`4294967295`). The precisions "Année" and "Semaine" of the Pivoteur give NULL for such a date, and `-001-S52` for `0000-01-01` (`crates/pivot/src/dates.rs`).

### 3.13 Types of results

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `type.case_mixed` | `CASE WHEN TRUE THEN 1 ELSE 'a' END` | error: the types of the branches must be comparable (SQL-92 6.9 SR 7, 9.3) | `1` (VARCHAR) | `1` (VARCHAR) | error: invalid input syntax for type integer | `1` | `1` (INTEGER) | error 386: no common supertype |

### 3.14 Quoted names

| Case | Expression | Standard | MySQL | MariaDB | PostgreSQL | SQLite | DuckDB | ClickHouse |
|---|---|---|---|---|---|---|---|---|
| `name.double_quotes` | `"x"` | a delimited identifier (SQL-92 5.2) | `x`: a text | `x`: a text | error: column `x` does not exist | error: no such column, with the hint of single quotes | error: column `x` not found | a name; texts take single quotes only |
| `name.backticks` | `` `x` `` | | error 1054: unknown column | error 1054 | error: syntax | error: no such column | error: column `x` not found | a name |

The expression names no column: an error on the column shows that the engine reads a name. MySQL and MariaDB read double quotes as a text under their default `sql_mode`; SQLite, DuckDB and ClickHouse accept both quotes for a name, PostgreSQL only the double quotes.

### 3.15 Origin of the columns of a result

What the metadata of a result tell of the column a result column reads, measured on 2026-11-01 by the test `tells_the_origin_of_the_columns_of_a_result` of `crates/connectors/tests/mysql.rs`, on the six servers of `compose.yaml`, and by `mysql --column-type-info`. This is the protocol of MySQL and MariaDB, without a case in `bench/sql/cases.mjs`, which compares expressions; the other engines have no connector yet, and are not measured.

| Column of the result | `org_table`, `org_name` | `PRI_KEY_FLAG` |
|---|---|---|
| `id` of `SELECT id, v FROM t`, alias included | `t`, `id` | yes, on each column of a composite key |
| `t.id` through a `JOIN` | `t`, `id` | yes; its values can then repeat, or be NULL through a `LEFT JOIN` |
| `id` of a view | the view, `id` | yes |
| `id` of a derived table or of a CTE | MySQL: `t`, `id`; MariaDB: the alias, `id` | yes |
| `id` of `SELECT id, COUNT(*) … GROUP BY id` | `t`, `id` | yes, except on MySQL 8.0.11 |
| `id + 1` | empty | no |
| `id` of a `UNION` | empty | no |
| A column of a `UNIQUE` index, or of another index | `t`, the column | no |

`UNIQUE_KEY_FLAG` and `MULTIPLE_KEY_FLAG` tell the other indexes. A foreign key has no flag of its own: a column `p_id` referencing a table and a column `amount` under a plain index both give `MULTIPLE_KEY_FLAG`, and only `information_schema.KEY_COLUMN_USAGE` tells them apart.

## 4. Differences between versions of one engine

| Engine | Subject | Versions |
|---|---|---|
| MySQL | type of `YEAR('0000-00-00')` | BIGINT on 8.0.11, YEAR on 9.7.4 and 26.10.0 |
| MySQL | writing of `ROUND(1.005e0, 2)` | `1.00` on 8.0.11, `1` on 9.7.4 and 26.10.0 |
| MySQL | `PRI_KEY_FLAG` of a column under `GROUP BY` | absent on 8.0.11, present on 9.7.4 and 26.10.0 |
| MariaDB | default collation | `utf8mb4_general_ci` on 10.6.3, `utf8mb4_uca1400_ai_ci` on 12.3.3 and 13.0.2 |
| MariaDB | error of `LISTAGG … WITHIN GROUP` | 1305 (no such function) on 10.6.3, 1064 (syntax) on 12.3.3 and 13.0.2 |
| ClickHouse | `%M` of `formatDateTime` | the minutes before 23.4, the name of the month since |
| ClickHouse | NULL in `greatest` and `least` | NULL before 24.12, ignored since (`least_greatest_legacy_null_behavior` restores the old behaviour) |
| ClickHouse | `groupConcat` | since 24.8 |

## 5. Sources

SQL standard, read on 2026-11-01 and 2026-11-02:

- [SQL-92, draft of July 1992](https://gist.github.com/zeroidentidad/e3d046f4c678c668cc8428c0fe39d1af), also [at Carnegie Mellon](https://www.contrib.andrew.cmu.edu/~shadow/sql/sql1992.txt): subclauses 4.2, 4.5, 4.6, 5.3, 6.1, 6.5, 6.6, 6.7, 6.9, 6.10, 6.12, 6.13, 6.14, 8.2, 9.3, 13.1, 22.1.
- [Grammar of SQL:1999](https://ronsavage.github.io/SQL/sql-99.bnf.html) and [of SQL:2003](https://ronsavage.github.io/SQL/sql-2003-2.bnf.html): `IS DISTINCT FROM`, `OVERLAY`, `NULLS FIRST | LAST`, `FILTER`, `CEILING`, `BIGINT`.
- [Grammar of the Foundation of SQL:2016](https://jakewheat.github.io/sql-overview/sql-2016-foundation-grammar.html): functions present and absent, `CAST … FORMAT`, `LISTAGG` (10.9).
- [LISTAGG](https://modern-sql.com/feature/listagg), [FILTER](https://modern-sql.com/feature/filter) and [GREATEST / LEAST](https://modern-sql.com/caniuse/greatest-least), on modern-sql.com.
- [SQL:2023 is finished: here is what's new](https://peter.eisentraut.org/blog/2023/04/04/sql-2023-is-finished-here-is-whats-new), P. Eisentraut, and [SQL:2023](https://en.wikipedia.org/wiki/SQL:2023), [SQL:2016](https://en.wikipedia.org/wiki/SQL:2016), [Null (SQL)](https://en.wikipedia.org/wiki/Null_(SQL)) on Wikipedia.

PostgreSQL: [conditional expressions](https://www.postgresql.org/docs/current/functions-conditional.html) (`GREATEST` and `LEAST` against the standard), and [a commit on full case mapping](https://postgresql.org/message-id/E1tYwFg-0029Dk-Lm%40gemulon.postgresql.org), read on 2026-11-02, which says that the standard names `ß` uppercased as `SS`.

ClickHouse, current documentation, read on 2026-11-01:

- [comparison functions](https://clickhouse.com/docs/sql-reference/functions/comparison-functions), [ORDER BY](https://clickhouse.com/docs/sql-reference/statements/select/order-by), [operators](https://clickhouse.com/docs/sql-reference/operators), [syntax](https://clickhouse.com/docs/sql-reference/syntax);
- [arithmetic functions](https://clickhouse.com/docs/sql-reference/functions/arithmetic-functions), [rounding functions](https://clickhouse.com/docs/sql-reference/functions/rounding-functions), [type conversion functions](https://clickhouse.com/docs/sql-reference/functions/type-conversion-functions), [conditional functions](https://clickhouse.com/docs/sql-reference/functions/conditional-functions);
- [string functions](https://clickhouse.com/docs/sql-reference/functions/string-functions), [array functions](https://clickhouse.com/docs/sql-reference/functions/array-functions), [splitting and merging functions](https://clickhouse.com/docs/sql-reference/functions/splitting-merging-functions);
- [date and time functions](https://clickhouse.com/docs/sql-reference/functions/date-time-functions), [Date](https://clickhouse.com/docs/sql-reference/data-types/date), [Date32](https://clickhouse.com/docs/sql-reference/data-types/date32), [Bool](https://clickhouse.com/docs/sql-reference/data-types/boolean), [Array](https://clickhouse.com/docs/sql-reference/data-types/array);
- [groupConcat](https://clickhouse.com/docs/sql-reference/aggregate-functions/reference/groupconcat), [groupArray](https://clickhouse.com/docs/sql-reference/aggregate-functions/reference/grouparray), [combinators](https://clickhouse.com/docs/sql-reference/aggregate-functions/combinators), [filtered aggregates](https://clickhouse.com/docs/resources/support-center/knowledge-base/queries-sql/filtered-aggregates).

Left out, found only on third-party sites: the value of `'12' + 1` in ClickHouse, the side converted by `'10' > 9`, and `CAST('2026-02-30' AS Date)`.
