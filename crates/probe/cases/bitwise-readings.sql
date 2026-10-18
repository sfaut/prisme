-- Bit operators of strings, floats and decimals, by expression (literal, column of a table,
-- of a derived table, of a view, string function, COALESCE) and against BIT_COUNT, which
-- reads its argument the same way (crates/probe/cases/bitwise-numbers.txt); a JSON column.
DROP VIEW IF EXISTS bo_view;
DROP TABLE IF EXISTS bo;
CREATE TABLE bo (id INT NOT NULL PRIMARY KEY, s VARCHAR(30) NOT NULL, b VARBINARY(30) NOT NULL, f DOUBLE NULL, d DECIMAL(30, 1) NULL, j JSON NULL);
INSERT INTO bo (id, s, b, f, d) VALUES (1, '3.7', '3.7', 3.7, 3.7), (2, '2.5', '2.5', 2.5, 2.5), (3, '-3.7', '-3.7', -3.7, -3.7), (4, '1e3', '1e3', 1000.0, NULL), (5, '18446744073709551615', '18446744073709551615', 1.8446744073709552e+19, 18446744073709551615.0), (6, '9223372036854775808', '9223372036854775808', 9.223372036854776e+18, 9223372036854775808.0), (7, '18446744073709551616', '18446744073709551616', 1.8446744073709552e+19, 18446744073709551616.0), (8, ' 12', ' 12', 12.0, 12.0), (9, '
12', '
12', 12.0, 12.0), (10, '12abc', '12abc', NULL, NULL), (11, '-1', '-1', -1.0, -1.0);
UPDATE bo SET j = CASE id WHEN 1 THEN '3.7' WHEN 2 THEN '"3.7"' WHEN 7 THEN '18446744073709551616' WHEN 11 THEN '-1' END;
CREATE VIEW bo_view AS SELECT id, s FROM bo;
SELECT id, HEX(s) AS s_hex, s | 0 AS col, BIT_COUNT(s) AS bc_col, CONCAT(s) | 0 AS concat_, BIT_COUNT(CONCAT(s)) AS bc_concat, COALESCE(s) | 0 AS coalesce_, BIT_COUNT(COALESCE(s)) AS bc_coalesce, ~s AS not_col, s << 0 AS shl_col, s & 18446744073709551615 AS and_col FROM bo ORDER BY id;
SELECT q.id, q.s | 0 AS derived, BIT_COUNT(q.s) AS bc_derived, v.s | 0 AS view_, BIT_COUNT(v.s) AS bc_view FROM (SELECT id, s FROM bo) q JOIN bo_view v ON v.id = q.id ORDER BY q.id;
SELECT id, b | 0 AS bin_number, BIT_COUNT(b | 0) AS bc_bin_number, f | 0 AS float_, BIT_COUNT(f) AS bc_float, d | 0 AS decimal_, BIT_COUNT(d) AS bc_decimal FROM bo ORDER BY id;
SELECT '3.7' | 0 AS l0, '2.5' | 0 AS l1, '-3.7' | 0 AS l2, '1e3' | 0 AS l3, '18446744073709551615' | 0 AS l4, '9223372036854775808' | 0 AS l5, '18446744073709551616' | 0 AS l6, ' 12' | 0 AS l7, '
12' | 0 AS l8, '12abc' | 0 AS l9, '-1' | 0 AS l10, BIT_COUNT('3.7') AS l11, BIT_COUNT('2.5') AS l12, BIT_COUNT('-3.7') AS l13, BIT_COUNT('1e3') AS l14, BIT_COUNT('18446744073709551615') AS l15, BIT_COUNT('9223372036854775808') AS l16, BIT_COUNT('18446744073709551616') AS l17, BIT_COUNT(' 12') AS l18, BIT_COUNT('
12') AS l19, BIT_COUNT('12abc') AS l20, BIT_COUNT('-1') AS l21;
SELECT id, j | 0 AS json_ FROM bo ORDER BY id;
DROP VIEW bo_view;
DROP TABLE bo;
