-- BIT_OR, BIT_AND and BIT_XOR of each type (dates and times with a fraction, YEAR, ENUM, SET,
-- BIT, JSON, strings, floats, decimals, byte strings), against the operator `|` and BIT_COUNT,
-- with a fraction rounded up to the next minute.
DROP TABLE IF EXISTS ba;
CREATE TABLE ba (id INT NOT NULL PRIMARY KEY, dd DATE NULL, tm TIME(1) NULL, dt DATETIME(1) NULL, ts TIMESTAMP(1) NULL, y YEAR NULL, e ENUM('a', 'b', 'c') NULL, st SET('a', 'b', 'c') NULL, t BIT(8) NULL, js JSON NULL);
INSERT INTO ba VALUES (1, '2024-01-01', '10:00:00.6', '2024-01-01 10:00:00.5', '2024-01-01 10:00:00.4', 2024, 'c', 'a,c', b'110', '3.7'), (2, '1999-12-31', '-10:00:00.4', '1999-12-31 23:59:59.6', '1999-12-31 23:59:59.5', 1999, 'b', 'b', b'1', '"3.7"'), (3, NULL, '10:00:59.5', NULL, NULL, NULL, NULL, NULL, NULL, '18446744073709551616'), (4, NULL, '-10:00:59.6', NULL, NULL, NULL, NULL, NULL, NULL, '[1]');
SELECT id, BIT_OR(dd) AS dd, BIT_OR(tm) AS tm, BIT_OR(dt) AS dt, BIT_OR(ts) AS ts, BIT_OR(y) AS y, BIT_OR(e) AS e, BIT_OR(st) AS st, BIT_OR(t) AS t FROM ba GROUP BY id ORDER BY id;
SELECT id, dd | 0 AS dd, tm | 0 AS tm, dt | 0 AS dt, ts | 0 AS ts, y | 0 AS y, e | 0 AS e, st | 0 AS st, t | 0 AS t FROM ba ORDER BY id;
SELECT id, BIT_COUNT(tm) AS tm, BIT_COUNT(dt) AS dt, BIT_COUNT(ts) AS ts, tm + 0 AS tm_number, dt + 0 AS dt_number FROM ba ORDER BY id;
SELECT id, BIT_OR(js) AS or_js, BIT_AND(js) AS and_js, js | 0 AS js FROM ba GROUP BY id ORDER BY id;
SELECT TIME'10:00:59.5' | 0 AS time_literal, TIMESTAMP'1999-12-31 23:59:59.6' | 0 AS timestamp_literal, BIT_COUNT(TIME'10:00:59.5') AS bc_time_literal, (SELECT BIT_OR(TIME'10:00:00.6')) AS or_time_literal, CAST('10:00:59.5' AS TIME(1)) | 0 AS cast_time;
DROP TABLE ba;
DROP TABLE IF EXISTS bo;
CREATE TABLE bo (id INT NOT NULL PRIMARY KEY, s VARCHAR(30) NOT NULL, f DOUBLE NULL, d DECIMAL(30, 1) NULL);
INSERT INTO bo VALUES (1, '3.7', 3.7, 3.7), (2, '2.5', 2.5, 2.5), (3, '-3.7', -3.7, -3.7), (4, '1e3', 1000.0, NULL), (5, '18446744073709551615', 1.8446744073709552e+19, 18446744073709551615.0), (6, '9223372036854775808', 9.223372036854776e+18, 9223372036854775808.0), (7, '18446744073709551616', 1.8446744073709552e+19, 18446744073709551616.0), (8, ' 12', 12.0, 12.0), (10, '12abc', NULL, NULL), (11, '-1', -1.0, -1.0);
SELECT BIT_OR(s) AS or_s, BIT_AND(s) AS and_s, BIT_XOR(s) AS xor_s, BIT_OR(f) AS or_f, BIT_OR(d) AS or_d FROM bo WHERE id = 1;
SELECT id, BIT_OR(s) AS or_s, s | 0 AS s_operator, BIT_OR(f) AS or_f, BIT_OR(d) AS or_d FROM bo GROUP BY id, s ORDER BY id;
SELECT (SELECT BIT_OR(v) FROM (SELECT 3.7 AS v UNION ALL SELECT 0.2) u) AS or_decimal, (SELECT BIT_OR(v) FROM (SELECT '3.7' AS v UNION ALL SELECT '0.2') u) AS or_string, (SELECT HEX(BIT_OR(v)) FROM (SELECT CAST('1' AS BINARY) AS v) u) AS or_binary, (SELECT BIT_OR(v) FROM (SELECT TIME'10:00:00.6' AS v) u) AS or_time;
DROP TABLE bo;
