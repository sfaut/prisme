-- PERIOD_ADD and PERIOD_DIFF (rule `function.period`): the forms written by
-- Prisme against the functions of the other dialect, row by row; then the
-- period 0 that MySQL refuses where MariaDB reads the period otherwise, and
-- the readings of a negative period and of months beyond 32 bits, which
-- change in MySQL 8.0.16. Run on MySQL 8.0.11, 8.0.15, 8.0.16 and 8.4,
-- MariaDB 10.6 and 11.8: `to_mariadb` of MariaDB against `native` of MySQL,
-- `to_mysql` of MySQL against `mariadb_native`.
DROP TABLE IF EXISTS pa;
CREATE TABLE pa (id INT, p BIGINT, m BIGINT);
INSERT INTO pa VALUES (1, 202401, 1), (2, 202401, -1), (3, 999912, 1), (4, 1000001, 1), (5, 1000012, -13), (6, 20240506, 1), (7, 20240512, -1200), (8, 2147483601, 1), (9, 4294967201, 1), (10, 99999999912, 7), (11, 9223372036854775801, 1), (12, 7001, 0), (13, 101, 1), (14, 10001, -1200), (15, 202401, 2147483647), (16, 10001, -1), (17, 1000001, -118800), (18, NULL, 1), (19, 1000001, NULL);
SELECT id, PERIOD_ADD(p, m) AS native, IF(p > 999912, (p DIV 100 * 12 + p MOD 100 - 1 + m) DIV 12 * 100 + (p DIV 100 * 12 + p MOD 100 - 1 + m) MOD 12 + 1, PERIOD_ADD(p, m)) AS to_mariadb FROM pa ORDER BY id;
DROP TABLE IF EXISTS pd;
CREATE TABLE pd (id INT, p BIGINT, q BIGINT);
INSERT INTO pd VALUES (1, 202401, 202301), (2, 1000001, 202401), (3, 20240506, 202401), (4, 202401, 20240506), (5, 9223372036854775801, 101), (6, 999912, 101), (7, 10001, 7001), (8, NULL, 202401), (9, 1000001, NULL);
SELECT id, PERIOD_DIFF(p, q) AS native, PERIOD_DIFF(p, q) + IF(p > 999912, p DIV 100 * 12 + p MOD 100 - 1, 0) - IF(q > 999912, q DIV 100 * 12 + q MOD 100 - 1, 0) AS to_mariadb FROM pd ORDER BY id;
DROP TABLE IF EXISTS pe;
CREATE TABLE pe (id INT, p BIGINT, q BIGINT);
INSERT INTO pe VALUES (1, 202401, 202301), (2, 1000001, 202401), (3, 0, 202401), (4, -202404, 202401), (5, 20240506, 7001), (6, 202401, 99999999912), (7, 999913, 202401), (8, 4294967295, 101), (9, -1, 0), (10, NULL, 202401), (11, 1000001, NULL);
SELECT id, PERIOD_DIFF(IF(p NOT BETWEEN 1 AND 999912, 7001, p), IF(q NOT BETWEEN 1 AND 999912, 7001, q)) - IF(p NOT BETWEEN 1 AND 999912, 23640, 0) + IF(q NOT BETWEEN 1 AND 999912, 23640, 0) AS to_mysql FROM pe ORDER BY id;
SELECT id, PERIOD_DIFF(p, q) AS mariadb_native FROM pe ORDER BY id;
SELECT PERIOD_ADD(IF(1000001 NOT BETWEEN 1 AND 999912, 1000001 * 0, 1000001), 1) AS trap_literal;
SELECT id, PERIOD_ADD(IF(p NOT BETWEEN 1 AND 999912, p * 0, p), m) AS trap FROM pa WHERE id = 4;
SELECT id, PERIOD_ADD(IF(p NOT BETWEEN 1 AND 999912, p * 0, p), m) AS trap FROM pa WHERE id IN (18, 19) ORDER BY id;
SELECT id, PERIOD_ADD(IF(p NOT BETWEEN 1 AND 999912 OR m < -23640 - PERIOD_DIFF(p, 7001), p * 0, p), m) AS trap FROM pa WHERE id IN (1, 2, 12, 13, 14, 16, 18, 19) ORDER BY id;
SELECT PERIOD_ADD(IF(m < -24288, 202401 * 0, 202401), m) AS sum_trap FROM (SELECT -30000 AS m) x;
SELECT id, PERIOD_ADD(IF(p NOT BETWEEN 1 AND 999912 OR m < -23640 - PERIOD_DIFF(p, 7001), p * 0, p), m) AS invalid_null FROM (SELECT 1 AS id, 202413 AS p, NULL AS m) x;
SELECT PERIOD_ADD(202401, -24288) AS zero_sum, PERIOD_ADD(202401, -30000) AS negative_sum;
SELECT PERIOD_ADD(-202404, 1) AS negative_period;
SELECT PERIOD_DIFF(-202404, 202401) AS negative_diff;
SELECT PERIOD_ADD(202401, 4294967296) AS months_2_32, PERIOD_ADD(202401, 2147483648) AS months_2_31;
