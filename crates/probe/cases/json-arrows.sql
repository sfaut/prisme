-- The JSON arrows of MySQL, `column->path` and `column->>path`: the forms
-- each server reads (a column, qualified or not, then a single string), the
-- names it gives their columns, their values on a text and on a JSON. They
-- stand for JSON_EXTRACT and JSON_UNQUOTE(JSON_EXTRACT()); MariaDB reads
-- none of them.
DROP TABLE IF EXISTS ja;
CREATE TABLE ja (b VARCHAR(50) NULL, j JSON NULL);
INSERT INTO ja VALUES ('{"a": "x", "n": [1, 2]}', '{"a": "x", "n": [1, 2]}');
SELECT b->'$.a', b->>'$.a', j->'$.n', j->>'$.n', j->>'$.a' FROM ja;
SELECT b -> '$.a', b ->> '$.a' FROM ja;
SELECT ja.b->'$.a', probe.ja.b->'$.a', `b`->'$.a' FROM ja;
SELECT b->"$.a" FROM ja;
SELECT b->'$' '.a' FROM ja;
SELECT b->_utf8mb4'$.a' FROM ja;
SELECT b->N'$.a' FROM ja;
SELECT b->X'24' FROM ja;
SELECT (b)->'$.a' FROM ja;
SELECT b->'$.n'->'$[0]' FROM ja;
SELECT b->CONCAT('$', '.a') FROM ja;
SELECT JSON_OBJECT('a', 1)->'$.a';
SELECT -b->'$.n[0]', b->'$.n[0]' + 1, b->>'$.a' LIKE 'x%', NOT b->'$.n[0]' FROM ja;
SELECT b AS x FROM ja ORDER BY x->'$.a';
SELECT b->>'$.a' COLLATE utf8mb4_bin FROM ja;
SELECT b->'$.zz', b->>'$.zz', b->'$..' FROM ja;
SELECT d.b->'$.a' FROM (SELECT b FROM ja) AS d;
SELECT b->'$.a' FROM ja GROUP BY b->'$.a';
