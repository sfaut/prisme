-- Type of CASE, IF, IFNULL, COALESCE and UNION over an unsigned integer and NULL: MariaDB
-- gives a DECIMAL where MySQL keeps the integer (measured with a literal NULL branch). In a
-- CREATE TABLE ... AS SELECT, MySQL 8.0.11 and 8.4.11 type CASE WHEN ... THEN 4294967295
-- ELSE NULL END as an INT and refuse the value (error 1264): the literal is left out.
DROP TABLE IF EXISTS cu_src;
DROP TABLE IF EXISTS cu_out;
CREATE TABLE cu_src (tu TINYINT UNSIGNED NULL, iu INT UNSIGNED NULL, bu BIGINT UNSIGNED NULL, bn BIGINT UNSIGNED NOT NULL, i INT NULL);
INSERT INTO cu_src VALUES (1, 2, 18446744073709551615, 4, 5);
CREATE TABLE cu_out AS SELECT
  CASE WHEN i > 0 THEN tu ELSE NULL END AS case_tu_null,
  CASE WHEN i > 0 THEN iu ELSE NULL END AS case_iu_null,
  CASE WHEN i > 0 THEN bu ELSE NULL END AS case_bu_null,
  CASE WHEN i > 0 THEN bn ELSE NULL END AS case_bn_null,
  CASE WHEN i > 0 THEN bu END AS case_bu_no_else,
  CASE WHEN i > 0 THEN NULL ELSE bu END AS case_null_bu,
  CASE WHEN i > 0 THEN bu ELSE bu END AS case_bu_bu,
  CASE WHEN i > 0 THEN bu ELSE iu END AS case_bu_iu,
  CASE WHEN i > 0 THEN bu ELSE i END AS case_bu_i,
  CASE WHEN i > 0 THEN iu ELSE i END AS case_iu_i,
  CASE WHEN i > 0 THEN tu ELSE i END AS case_tu_i,
  CASE WHEN i > 0 THEN bu ELSE CAST(NULL AS UNSIGNED) END AS case_bu_cast_null,
  IF(i > 0, bu, NULL) AS if_bu_null,
  IF(i > 0, iu, NULL) AS if_iu_null,
  IFNULL(bu, NULL) AS ifnull_bu_null,
  COALESCE(bu, NULL) AS coalesce_bu_null,
  COALESCE(iu, NULL) AS coalesce_iu_null,
  NULLIF(bu, 0) AS nullif_bu,
  CASE WHEN i > 0 THEN 18446744073709551615 ELSE NULL END AS case_lit_null
FROM cu_src;
SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'cu_out' ORDER BY ORDINAL_POSITION;
SELECT * FROM cu_out;
DROP TABLE cu_out;
CREATE TABLE cu_out AS SELECT bu AS u FROM cu_src UNION ALL SELECT NULL;
SELECT 'union' AS form, COLUMN_TYPE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'cu_out';
DROP TABLE cu_out;
CREATE TABLE cu_out AS SELECT iu AS u FROM cu_src UNION ALL SELECT i FROM cu_src;
SELECT 'union iu i' AS form, COLUMN_TYPE FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'cu_out';
DROP TABLE cu_out;
DROP TABLE cu_src;
