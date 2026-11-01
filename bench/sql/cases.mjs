/**
 * Cases of the comparison of the SQL engines (`docs/SQL-COMPARISON.md`): one expression each,
 * whose value and type `run.mjs` measures on every engine.
 *
 * Each case gives a shared form, run by every engine that has no form of its own; an engine form
 * replaces it where the engine spells the same question otherwise (`SIGNED` for `INTEGER` in
 * MySQL), and `null` leaves the case out for an engine where it means nothing. A shared form
 * that an engine refuses stays as it is: its error is the measure. Each expression holds on one
 * line, as `run.mjs` finds the case of an error by its line.
 */

/**
 * @typedef {'mysql' | 'postgres' | 'duckdb' | 'sqlite'} Engine
 * Families of engines: `mysql` covers MySQL and MariaDB.
 */

/**
 * @typedef {object} Case
 * @property {string} key Name of the case, `<family>.<subject>`, as the tables of the document
 *     cite it.
 * @property {{ shared: string | null } & Partial<Record<Engine, string | null>>} forms
 *     Expression run by default, and the forms of the engines that spell it otherwise.
 */

/**
 * The cases, by family, in the order of the document.
 *
 * @type {Case[]}
 */
export const CASES = [
  // Settings that the results depend on
  {
    key: 'info.collation',
    forms: {
      shared: null,
      mysql: '@@collation_connection',
      postgres: '(SELECT datcollate FROM pg_database WHERE datname = current_database())',
      duckdb: "current_setting('default_collation')",
    },
  },
  {
    key: 'info.pad_attribute',
    forms: {
      shared: null,
      // MariaDB 12.3.3 and 13.0.2 list their uca1400 collations without the character set
      mysql:
        '(SELECT PAD_ATTRIBUTE FROM information_schema.COLLATIONS WHERE COLLATION_NAME IN (@@collation_connection, SUBSTRING(@@collation_connection, 9)) LIMIT 1)',
    },
  },
  { key: 'info.sql_mode', forms: { shared: null, mysql: '@@sql_mode' } },
  { key: 'info.group_concat_max_len', forms: { shared: null, mysql: '@@group_concat_max_len' } },
  { key: 'info.default_week_format', forms: { shared: null, mysql: '@@default_week_format' } },
  {
    key: 'info.lc_time_names',
    forms: { shared: null, mysql: '@@lc_time_names', postgres: "current_setting('lc_time')" },
  },
  {
    key: 'info.div_precision_increment',
    forms: { shared: null, mysql: '@@div_precision_increment' },
  },
  {
    key: 'info.time_zone',
    forms: {
      shared: null,
      mysql: '@@time_zone',
      postgres: "current_setting('TimeZone')",
      duckdb: "current_setting('TimeZone')",
    },
  },
  {
    key: 'info.null_order',
    forms: { shared: null, duckdb: "current_setting('default_null_order')" },
  },
  {
    key: 'info.ieee_floating_point_ops',
    forms: { shared: null, duckdb: "current_setting('ieee_floating_point_ops')" },
  },
  // Comparison of texts
  { key: 'text.case', forms: { shared: "'FR' = 'fr'" } },
  { key: 'text.accent', forms: { shared: "'e' = 'é'" } },
  { key: 'text.trailing_space', forms: { shared: "'a' = 'a '" } },
  // Binary collations of MySQL and MariaDB, for the tests of the conformance on their servers
  {
    key: 'text.trailing_space_bin',
    forms: { shared: null, mysql: "'a' = 'a ' COLLATE utf8mb4_bin" },
  },
  {
    key: 'text.trailing_space_0900_bin',
    forms: { shared: null, mysql: "'a' = 'a ' COLLATE utf8mb4_0900_bin" },
  },
  {
    key: 'text.trailing_space_nopad_bin',
    forms: { shared: null, mysql: "'a' = 'a ' COLLATE utf8mb4_nopad_bin" },
  },
  { key: 'text.like_case', forms: { shared: "'ABC' LIKE 'abc'" } },
  { key: 'text.like_accent_case', forms: { shared: "'É' LIKE 'é'" } },
  { key: 'text.order', forms: { shared: "'a' < 'B'" } },
  { key: 'text.least', forms: { shared: "LEAST('a', 'B')", sqlite: "MIN('a', 'B')" } },
  { key: 'text.replace_case', forms: { shared: "REPLACE('aA', 'a', 'x')" } },
  { key: 'text.upper_sharp_s', forms: { shared: "UPPER('ß')" } },
  // Functions on texts
  {
    key: 'text.substring_zero',
    forms: { shared: "SUBSTRING('abc' FROM 0 FOR 2)", sqlite: "SUBSTR('abc', 0, 2)" },
  },
  { key: 'text.substring_comma_zero', forms: { shared: "SUBSTRING('abc', 0)" } },
  {
    key: 'text.position',
    forms: { shared: "POSITION('b' IN 'abc')", sqlite: "INSTR('abc', 'b')" },
  },
  {
    key: 'text.trim_both',
    forms: { shared: "TRIM(BOTH 'x' FROM 'xxaxx')", sqlite: "TRIM('xxaxx', 'x')" },
  },
  { key: 'text.overlay', forms: { shared: "OVERLAY('abcdef' PLACING 'XY' FROM 2 FOR 3)" } },
  { key: 'text.lpad_shorter', forms: { shared: "LPAD('abc', 2, 'x')" } },
  { key: 'text.left_negative', forms: { shared: "LEFT('abc', -1)" } },
  // Texts read as numbers
  { key: 'num.text_digits', forms: { shared: "'12' + 1" } },
  { key: 'num.text_prefix', forms: { shared: "'12abc' + 1" } },
  { key: 'num.text_letters', forms: { shared: "'abc' + 0" } },
  { key: 'num.text_exponent', forms: { shared: "'1e2' + 0" } },
  { key: 'num.text_spaces', forms: { shared: "' 12 ' + 0" } },
  { key: 'num.text_equals_zero', forms: { shared: "'abc' = 0" } },
  { key: 'num.text_vs_number', forms: { shared: "'10' > 9" } },
  { key: 'num.text_vs_text', forms: { shared: "'10' > '9'" } },
  // CAST
  {
    key: 'cast.int_letters',
    forms: { shared: "CAST('abc' AS INTEGER)", mysql: "CAST('abc' AS SIGNED)" },
  },
  {
    key: 'cast.int_prefix',
    forms: { shared: "CAST('12abc' AS INTEGER)", mysql: "CAST('12abc' AS SIGNED)" },
  },
  {
    key: 'cast.int_text_fraction',
    forms: { shared: "CAST('1.5' AS INTEGER)", mysql: "CAST('1.5' AS SIGNED)" },
  },
  {
    key: 'cast.int_decimal_fraction',
    forms: { shared: 'CAST(1.5 AS INTEGER)', mysql: 'CAST(1.5 AS SIGNED)' },
  },
  {
    key: 'cast.int_decimal_negative_half',
    forms: { shared: 'CAST(-2.5 AS INTEGER)', mysql: 'CAST(-2.5 AS SIGNED)' },
  },
  { key: 'cast.int_ansi_name', forms: { shared: "CAST('7' AS INTEGER)" } },
  { key: 'cast.decimal_letters', forms: { shared: "CAST('abc' AS DECIMAL(10,2))" } },
  { key: 'cast.decimal_half', forms: { shared: "CAST('1.005' AS DECIMAL(10,2))" } },
  { key: 'cast.decimal_overflow', forms: { shared: "CAST('9999' AS DECIMAL(3,1))" } },
  {
    key: 'cast.date_impossible',
    forms: { shared: "CAST('2026-02-30' AS DATE)", sqlite: "DATE('2026-02-30')" },
  },
  { key: 'cast.date_letters', forms: { shared: "CAST('abc' AS DATE)", sqlite: "DATE('abc')" } },
  {
    key: 'cast.date_short_parts',
    forms: { shared: "CAST('2026-1-5' AS DATE)", sqlite: "DATE('2026-1-5')" },
  },
  {
    key: 'cast.date_compact',
    forms: { shared: "CAST('20260115' AS DATE)", sqlite: "DATE('20260115')" },
  },
  {
    key: 'cast.date_from_timestamp_text',
    forms: { shared: "CAST('2026-01-15 10:30:00' AS DATE)", sqlite: "DATE('2026-01-15 10:30:00')" },
  },
  {
    key: 'cast.timestamp_from_date_text',
    forms: {
      shared: "CAST('2026-01-15' AS TIMESTAMP)",
      mysql: "CAST('2026-01-15' AS DATETIME)",
      sqlite: "DATETIME('2026-01-15')",
    },
  },
  {
    key: 'cast.timestamp_bad_hour',
    forms: {
      shared: "CAST('2026-01-15 25:00:00' AS TIMESTAMP)",
      mysql: "CAST('2026-01-15 25:00:00' AS DATETIME)",
      sqlite: "DATETIME('2026-01-15 25:00:00')",
    },
  },
  { key: 'cast.try_cast', forms: { shared: "TRY_CAST('abc' AS INTEGER)" } },
  {
    key: 'cast.varchar_decimal',
    forms: { shared: 'CAST(1.50 AS VARCHAR(10))', mysql: 'CAST(1.50 AS CHAR(10))' },
  },
  // Division
  { key: 'div.by_zero', forms: { shared: '1 / 0' } },
  { key: 'div.modulo_zero', forms: { shared: 'MOD(1, 0)', sqlite: '1 % 0' } },
  { key: 'div.integers', forms: { shared: '7 / 2' } },
  { key: 'div.third', forms: { shared: '1 / 3' } },
  { key: 'div.decimal_third', forms: { shared: '1.0 / 3' } },
  { key: 'div.negative_modulo', forms: { shared: 'MOD(-7, 3)', sqlite: '-7 % 3' } },
  { key: 'div.decimal_modulo', forms: { shared: 'MOD(10, 3.5)', sqlite: '10 % 3.5' } },
  // Integers
  {
    key: 'int.bigint_overflow',
    forms: {
      shared: 'CAST(9223372036854775807 AS BIGINT) + 1',
      mysql: '9223372036854775807 + 1',
      sqlite: '9223372036854775807 + 1',
    },
  },
  {
    key: 'int.beyond_double',
    forms: {
      shared: 'CAST(9007199254740993 AS BIGINT) + 1',
      mysql: '9007199254740993 + 1',
      sqlite: '9007199254740993 + 1',
    },
  },
  // NULL
  { key: 'null.concat_pipes', forms: { shared: "'a' || NULL" } },
  { key: 'null.concat_function', forms: { shared: "CONCAT('a', NULL)" } },
  { key: 'null.concat_ws', forms: { shared: "CONCAT_WS(',', 'a', NULL, 'b')" } },
  { key: 'null.greatest', forms: { shared: 'GREATEST(1, NULL)', sqlite: 'MAX(1, NULL)' } },
  { key: 'null.in_list', forms: { shared: '1 IN (NULL, 2)' } },
  { key: 'null.is_distinct_from', forms: { shared: 'NULL IS DISTINCT FROM NULL' } },
  {
    key: 'null.sort_asc_first',
    forms: {
      shared:
        '(SELECT x IS NULL FROM (SELECT NULL AS x UNION ALL SELECT 1) AS t ORDER BY x LIMIT 1)',
    },
  },
  {
    key: 'null.sort_desc_first',
    forms: {
      shared:
        '(SELECT x IS NULL FROM (SELECT NULL AS x UNION ALL SELECT 1) AS t ORDER BY x DESC LIMIT 1)',
    },
  },
  {
    key: 'null.sort_nulls_last',
    forms: {
      shared:
        '(SELECT x IS NULL FROM (SELECT NULL AS x UNION ALL SELECT 1) AS t ORDER BY x NULLS LAST LIMIT 1)',
    },
  },
  // Aggregates
  {
    key: 'agg.listagg',
    forms: {
      shared:
        "(SELECT LISTAGG(k, ',') WITHIN GROUP (ORDER BY k) FROM (SELECT 'b' AS k UNION ALL SELECT 'a') AS t)",
    },
  },
  {
    key: 'agg.listagg_order_inside',
    forms: {
      shared:
        "(SELECT LISTAGG(k, ',' ORDER BY k) FROM (SELECT 'b' AS k UNION ALL SELECT 'a') AS t)",
    },
  },
  {
    key: 'agg.string_agg',
    forms: {
      shared:
        "(SELECT STRING_AGG(k, ',' ORDER BY k) FROM (SELECT 'b' AS k UNION ALL SELECT 'a') AS t)",
    },
  },
  {
    key: 'agg.group_concat',
    forms: {
      shared:
        "(SELECT GROUP_CONCAT(k ORDER BY k SEPARATOR ',') FROM (SELECT 'b' AS k UNION ALL SELECT 'a') AS t)",
      sqlite:
        "(SELECT GROUP_CONCAT(k, ',' ORDER BY k) FROM (SELECT 'b' AS k UNION ALL SELECT 'a') AS t)",
    },
  },
  {
    key: 'agg.default_separator',
    forms: { shared: '(SELECT GROUP_CONCAT(k) FROM (SELECT 1 AS k UNION ALL SELECT 2) AS t)' },
  },
  {
    key: 'agg.group_concat_limit',
    forms: {
      shared:
        "(SELECT GROUP_CONCAT(k ORDER BY k SEPARATOR ',' LIMIT 2 OFFSET 1) FROM (SELECT 'b' AS k UNION ALL SELECT 'a' UNION ALL SELECT 'c') AS t)",
    },
  },
  {
    key: 'agg.cut',
    forms: {
      shared:
        "(SELECT CHAR_LENGTH(STRING_AGG(k, ',')) FROM (SELECT REPEAT('x', 600) AS k UNION ALL SELECT REPEAT('x', 600)) AS t)",
      mysql:
        "(SELECT CHAR_LENGTH(GROUP_CONCAT(k)) FROM (SELECT REPEAT('x', 600) AS k UNION ALL SELECT REPEAT('x', 600)) AS t)",
      sqlite:
        "(SELECT LENGTH(STRING_AGG(k, ',')) FROM (SELECT printf('%.600c', 'x') AS k UNION ALL SELECT printf('%.600c', 'x')) AS t)",
    },
  },
  {
    key: 'agg.distinct_case',
    forms: {
      shared:
        "(SELECT STRING_AGG(DISTINCT k, ',' ORDER BY k) FROM (SELECT 'fr' AS k UNION ALL SELECT 'FR' UNION ALL SELECT 'Fr') AS t)",
      mysql:
        "(SELECT GROUP_CONCAT(DISTINCT k ORDER BY k) FROM (SELECT 'fr' AS k UNION ALL SELECT 'FR' UNION ALL SELECT 'Fr') AS t)",
      sqlite:
        "(SELECT GROUP_CONCAT(DISTINCT k ORDER BY k) FROM (SELECT 'fr' AS k UNION ALL SELECT 'FR' UNION ALL SELECT 'Fr') AS t)",
    },
  },
  {
    key: 'agg.order_case',
    forms: {
      shared:
        "(SELECT STRING_AGG(k, ' ' ORDER BY k) FROM (SELECT 'b' AS k UNION ALL SELECT 'B' UNION ALL SELECT 'a' UNION ALL SELECT 'A') AS t)",
      mysql:
        "(SELECT GROUP_CONCAT(k ORDER BY k SEPARATOR ' ') FROM (SELECT 'b' AS k UNION ALL SELECT 'B' UNION ALL SELECT 'a' UNION ALL SELECT 'A') AS t)",
    },
  },
  {
    key: 'agg.filter',
    forms: {
      shared:
        '(SELECT SUM(k) FILTER (WHERE k > 1) FROM (SELECT 1 AS k UNION ALL SELECT 2 UNION ALL SELECT 3) AS t)',
    },
  },
  {
    key: 'agg.avg_integers',
    forms: { shared: '(SELECT AVG(k) FROM (SELECT 1 AS k UNION ALL SELECT 2) AS t)' },
  },
  // Lengths
  { key: 'len.length', forms: { shared: "LENGTH('é')" } },
  { key: 'len.char_length', forms: { shared: "CHAR_LENGTH('é')" } },
  { key: 'len.octet_length', forms: { shared: "OCTET_LENGTH('é')" } },
  { key: 'len.bytes', forms: { shared: "OCTET_LENGTH('é')", duckdb: "STRLEN('é')" } },
  { key: 'len.emoji_length', forms: { shared: "LENGTH('😀')" } },
  { key: 'len.emoji_char_length', forms: { shared: "CHAR_LENGTH('😀')" } },
  // Operators
  { key: 'pipes.text', forms: { shared: "'a' || 'b'" } },
  { key: 'pipes.number', forms: { shared: "2026 || '-T'" } },
  { key: 'text.concat_number', forms: { shared: "CONCAT(2026, '-T')" } },
  // Quoted names: an identifier that names no column, or a text
  { key: 'name.double_quotes', forms: { shared: '"x"' } },
  { key: 'name.backticks', forms: { shared: '`x`' } },
  // Weeks and days
  {
    key: 'week.extract',
    forms: {
      shared: "EXTRACT(WEEK FROM DATE '2027-01-01')",
      sqlite: "STRFTIME('%V', '2027-01-01')",
    },
  },
  { key: 'week.mysql_mode0', forms: { shared: null, mysql: "WEEK('2026-01-04')" } },
  {
    key: 'week.iso_year_week',
    forms: {
      shared: null,
      mysql: "DATE_FORMAT('2027-01-01', '%x-%v')",
      postgres: "TO_CHAR(DATE '2027-01-01', 'IYYY-IW')",
      duckdb: "STRFTIME(DATE '2027-01-01', '%G-%V')",
      sqlite: "STRFTIME('%G-%V', '2027-01-01')",
    },
  },
  {
    key: 'week.calendar_year_week',
    forms: {
      shared: null,
      mysql: "DATE_FORMAT('2027-01-01', '%Y-%v')",
      postgres: "TO_CHAR(DATE '2027-01-01', 'YYYY-IW')",
      duckdb: "STRFTIME(DATE '2027-01-01', '%Y-%V')",
      sqlite: "STRFTIME('%Y-%V', '2027-01-01')",
    },
  },
  {
    key: 'week.yearweek',
    forms: { shared: null, mysql: "YEARWEEK('2027-01-01')", duckdb: "YEARWEEK(DATE '2027-01-01')" },
  },
  {
    key: 'week.day_number_monday',
    forms: {
      shared: null,
      mysql: "DAYOFWEEK('2026-01-05')",
      postgres: "EXTRACT(DOW FROM DATE '2026-01-05')",
      duckdb: "DAYOFWEEK(DATE '2026-01-05')",
      sqlite: "STRFTIME('%w', '2026-01-05')",
    },
  },
  {
    key: 'week.iso_day_number_monday',
    forms: {
      shared: null,
      mysql: "WEEKDAY('2026-01-05')",
      postgres: "EXTRACT(ISODOW FROM DATE '2026-01-05')",
      duckdb: "ISODOW(DATE '2026-01-05')",
      sqlite: "STRFTIME('%u', '2026-01-05')",
    },
  },
  {
    key: 'week.dayname',
    forms: {
      shared: null,
      mysql: "DAYNAME('2026-01-05')",
      postgres: "TO_CHAR(DATE '2026-01-05', 'FMDay')",
      duckdb: "DAYNAME(DATE '2026-01-05')",
    },
  },
  {
    key: 'week.dayname_locale',
    forms: { shared: null, mysql: "DATE_FORMAT('2026-01-05', '%W %M', 'fr_FR')" },
  },
  // Dates
  { key: 'date.literal', forms: { shared: "DATE '2026-01-15'" } },
  { key: 'date.literal_impossible', forms: { shared: "DATE '2026-02-30'" } },
  {
    key: 'date.impossible_plus_day',
    forms: {
      shared: "CAST('2026-02-30' AS DATE) + INTERVAL '1' DAY",
      mysql: "'2026-02-30' + INTERVAL 1 DAY",
      sqlite: "DATE('2026-02-30', '+1 day')",
    },
  },
  {
    key: 'date.add_month_end',
    forms: {
      shared: "DATE '2026-01-31' + INTERVAL '1' MONTH",
      mysql: "DATE '2026-01-31' + INTERVAL 1 MONTH",
      sqlite: "DATE('2026-01-31', '+1 month')",
    },
  },
  {
    key: 'date.zero',
    forms: { shared: "CAST('0000-00-00' AS DATE)", sqlite: "DATE('0000-00-00')" },
  },
  {
    key: 'date.zero_year',
    forms: {
      shared: "EXTRACT(YEAR FROM CAST('0000-00-00' AS DATE))",
      mysql: "YEAR('0000-00-00')",
      sqlite: "STRFTIME('%Y', '0000-00-00')",
    },
  },
  { key: 'date.zero_dayname', forms: { shared: null, mysql: "DAYNAME('0000-00-00')" } },
  {
    key: 'date.zero_format',
    forms: { shared: null, mysql: "DATE_FORMAT('0000-00-00', '%Y-%m-%d')" },
  },
  {
    key: 'date.zero_datediff',
    forms: { shared: null, mysql: "DATEDIFF('0000-00-00', '2026-01-01')" },
  },
  {
    key: 'date.year_zero',
    forms: { shared: "CAST('0000-01-01' AS DATE)", sqlite: "DATE('0000-01-01')" },
  },
  {
    key: 'date.dst_hour',
    forms: {
      shared: "EXTRACT(HOUR FROM TIMESTAMP '2026-03-29 02:30:00')",
      mysql: "HOUR('2026-03-29 02:30:00')",
      sqlite: "STRFTIME('%H', '2026-03-29 02:30:00')",
    },
  },
  {
    key: 'date.equals_text',
    forms: {
      shared: "DATE '2026-01-15' = '2026-01-15'",
      sqlite: "DATE('2026-01-15') = '2026-01-15'",
    },
  },
  {
    key: 'date.equals_timestamp_text',
    forms: {
      shared: "DATE '2026-01-15' = '2026-01-15 00:00:00'",
      sqlite: "DATE('2026-01-15') = '2026-01-15 00:00:00'",
    },
  },
  {
    key: 'date.equals_timestamp',
    forms: { shared: "DATE '2026-01-15' = TIMESTAMP '2026-01-15 00:00:00'" },
  },
  {
    key: 'date.timestamp_ge_date_text',
    forms: {
      shared: "TIMESTAMP '2026-01-15 10:00:00' >= '2026-01-15'",
      sqlite: "DATETIME('2026-01-15 10:00:00') >= '2026-01-15'",
    },
  },
  {
    key: 'date.timestamp_between_date_texts',
    forms: {
      shared: "TIMESTAMP '2026-01-15 10:00:00' BETWEEN '2026-01-01' AND '2026-01-15'",
      sqlite: "DATETIME('2026-01-15 10:00:00') BETWEEN '2026-01-01' AND '2026-01-15'",
    },
  },
  {
    key: 'date.timestamp_between_dates',
    forms: {
      shared: "TIMESTAMP '2026-01-15 10:00:00' BETWEEN DATE '2026-01-01' AND DATE '2026-01-15'",
    },
  },
  { key: 'date.extract_year', forms: { shared: "EXTRACT(YEAR FROM DATE '2026-01-15')" } },
  { key: 'date.trunc_month', forms: { shared: "DATE_TRUNC('month', DATE '2026-01-15')" } },
  {
    key: 'date.month_label',
    forms: {
      shared: null,
      mysql: "DATE_FORMAT('2026-01-15', '%Y-%m')",
      postgres: "TO_CHAR(DATE '2026-01-15', 'YYYY-MM')",
      duckdb: "STRFTIME(DATE '2026-01-15', '%Y-%m')",
      sqlite: "STRFTIME('%Y-%m', '2026-01-15')",
    },
  },
  {
    key: 'date.month_label_ansi',
    forms: { shared: "CAST(DATE '2026-01-15' AS VARCHAR(7) FORMAT 'YYYY-MM')" },
  },
  {
    key: 'date.quarter_label',
    forms: {
      shared: null,
      mysql: "DATE_FORMAT('2026-04-15', '%Y-T%Q')",
      postgres: "TO_CHAR(DATE '2026-04-15', 'YYYY-\"T\"Q')",
      duckdb: "STRFTIME(DATE '2026-04-15', '%Y-T%Q')",
      sqlite: "STRFTIME('%Y-T%Q', '2026-04-15')",
    },
  },
  // Rounding
  { key: 'round.half_exact', forms: { shared: 'ROUND(2.5)' } },
  { key: 'round.half_exact_negative', forms: { shared: 'ROUND(-2.5)' } },
  {
    key: 'round.half_float',
    forms: {
      shared: 'ROUND(CAST(2.5 AS DOUBLE PRECISION))',
      mysql: 'ROUND(2.5e0)',
      sqlite: 'ROUND(2.5)',
    },
  },
  {
    key: 'round.half_float_odd',
    forms: {
      shared: 'ROUND(CAST(3.5 AS DOUBLE PRECISION))',
      mysql: 'ROUND(3.5e0)',
      sqlite: 'ROUND(3.5)',
    },
  },
  { key: 'round.decimal_places', forms: { shared: 'ROUND(0.125, 2)' } },
  {
    key: 'round.float_places',
    forms: {
      shared: 'ROUND(CAST(1.005 AS DOUBLE PRECISION), 2)',
      mysql: 'ROUND(1.005e0, 2)',
      sqlite: 'ROUND(1.005, 2)',
    },
  },
  {
    key: 'round.truncate_negative',
    forms: { shared: 'TRUNC(-1.999, 2)', mysql: 'TRUNCATE(-1.999, 2)' },
  },
  { key: 'round.tens', forms: { shared: 'ROUND(15, -1)' } },
  { key: 'round.ceil_negative', forms: { shared: 'CEIL(-1.5)' } },
  // Booleans and exact numbers
  { key: 'bool.comparison', forms: { shared: '(1 = 1)' } },
  { key: 'bool.comparison_plus', forms: { shared: '(1 = 1) + 1' } },
  { key: 'bool.true_plus_true', forms: { shared: 'TRUE + TRUE' } },
  { key: 'exact.decimal_sum', forms: { shared: '0.1 + 0.2 = 0.3' } },
  {
    key: 'exact.float_sum',
    forms: {
      shared:
        'CAST(0.1 AS DOUBLE PRECISION) + CAST(0.2 AS DOUBLE PRECISION) = CAST(0.3 AS DOUBLE PRECISION)',
      mysql: '0.1e0 + 0.2e0 = 0.3e0',
      sqlite: '0.1 + 0.2 = 0.3',
    },
  },
  // Types of the results
  { key: 'type.case_mixed', forms: { shared: "CASE WHEN TRUE THEN 1 ELSE 'a' END" } },
  { key: 'type.power', forms: { shared: 'POWER(2, 3)' } },
]

/**
 * The expression of a case for an engine: its own form, else the shared one.
 *
 * @param {Case} entry The case.
 * @param {Engine} engine The family of the engine.
 * @returns {string | null} The expression, or `null` when the case does not apply.
 */
export function expression(entry, engine) {
  return engine in entry.forms ? entry.forms[engine] : entry.forms.shared
}
