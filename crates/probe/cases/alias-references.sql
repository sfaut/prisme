-- How each server resolves a name of `ORDER BY`, `GROUP BY` and `HAVING`
-- that is an alias of the select list, a column of the source or both: a
-- name alone, then inside an expression, a function, a subquery, an
-- aggregate; the alias of an aggregate, of a window function; a name
-- qualified by its table.
DROP TABLE IF EXISTS ar;
CREATE TABLE ar (id INT NOT NULL PRIMARY KEY, a INT NOT NULL, b INT NOT NULL);
INSERT INTO ar VALUES (1, 3, 10), (2, 2, 30), (3, 1, 20);
-- An alias that names no column of the source.
SELECT a AS x FROM ar ORDER BY x;
SELECT a AS x FROM ar ORDER BY x + 0;
SELECT a AS x FROM ar ORDER BY -x;
SELECT a AS x FROM ar ORDER BY ABS(x - 2), id;
SELECT a AS x FROM ar ORDER BY (SELECT x);
SELECT a AS x FROM ar ORDER BY (SELECT x + 0);
SELECT a AS x, COUNT(*) AS n FROM ar GROUP BY x ORDER BY n, x;
SELECT a AS x, COUNT(*) AS n FROM ar GROUP BY x + 0 ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x HAVING x = 1;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x HAVING x + 0 = 1;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY a % 2 HAVING n > 1;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY a % 2 HAVING n + 0 > 1;
SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x ORDER BY s + 0;
SELECT a AS x FROM ar ORDER BY SUM(x);
SELECT a AS x FROM ar GROUP BY a ORDER BY MAX(x);
SELECT a AS x, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY r + 0;
SELECT a AS x FROM ar WHERE x > 1;
SELECT a AS x FROM ar ORDER BY ar.x;
-- An alias that is also the name of another column of the source.
SELECT a AS b FROM ar ORDER BY b;
SELECT a AS b FROM ar ORDER BY b + 0;
SELECT a AS b FROM ar ORDER BY ar.b;
SELECT a AS b FROM ar ORDER BY (SELECT b);
SELECT a AS b, COUNT(*) AS n FROM ar GROUP BY b ORDER BY b;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY b + 0 ORDER BY 1;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2 HAVING b = 1;
SELECT a % 2 AS b, MAX(b) AS m FROM ar GROUP BY a % 2 HAVING MAX(b) > 20;
-- Two items of the same alias.
SELECT a AS x, b AS x FROM ar ORDER BY x;
SELECT a AS x, b AS x FROM ar ORDER BY x + 0;
SELECT a AS x, b AS x FROM ar GROUP BY x ORDER BY 1;
-- An alias that names another column, inside an expression: `b` read as
-- the alias sorts the rows 3, 2, 1, read as the column 1, 3, 2.
SELECT id, a AS b FROM ar ORDER BY b;
SHOW WARNINGS;
SELECT id, a AS b FROM ar ORDER BY b + 0;
SHOW WARNINGS;
SELECT id, a AS b FROM ar ORDER BY -b DESC;
SELECT id, a AS b FROM ar ORDER BY CONCAT(b);
SELECT id, a AS b FROM ar ORDER BY CASE WHEN b > 2 THEN 0 ELSE 1 END, b;
SELECT id, a AS b FROM ar ORDER BY (SELECT b);
SELECT id, a AS b FROM ar ORDER BY (SELECT b + 0);
SELECT id, a AS b FROM ar ORDER BY ar.b;
-- In `GROUP BY` and `HAVING`: `b` read as the alias gives two groups, read
-- as the column three.
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY b ORDER BY 1, 2;
SHOW WARNINGS;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY b + 0 ORDER BY 1, 2;
SHOW WARNINGS;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY ABS(b) ORDER BY 1, 2;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2 HAVING b + 0 = 1;
SHOW WARNINGS;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2, b HAVING b = 1 ORDER BY 1, 2;
SELECT COUNT(a) AS b FROM ar GROUP BY b HAVING b = 1 ORDER BY 1;
SHOW WARNINGS;
SELECT b AS a, COUNT(*) AS n FROM ar GROUP BY a ORDER BY 1, 2;
-- An alias of an aggregate or of a window inside an expression.
SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x HAVING s + 0 > 30 ORDER BY 1;
SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x ORDER BY -s;
SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x HAVING ABS(s) > 30 ORDER BY 1;
SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x HAVING (SELECT s) > 30 ORDER BY 1;
SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x HAVING SUM(s) > 30;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY -r;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar GROUP BY id, r;
SELECT id, a AS x, ROW_NUMBER() OVER (ORDER BY x) AS r FROM ar ORDER BY id;
SELECT id, a AS x, ROW_NUMBER() OVER (PARTITION BY x ORDER BY id) AS r FROM ar ORDER BY id;
-- The names of a set operation and of a query between parentheses.
SELECT a AS x FROM ar UNION ALL SELECT b FROM ar ORDER BY x;
SELECT a AS x FROM ar UNION ALL SELECT b FROM ar ORDER BY x + 0;
SELECT a AS x FROM ar UNION ALL SELECT b FROM ar ORDER BY -x;
SELECT a AS b FROM ar UNION ALL SELECT b FROM ar ORDER BY b + 0;
(SELECT id, a AS b FROM ar) ORDER BY b + 0;
(SELECT id, a AS b FROM ar ORDER BY b + 0 LIMIT 3);
-- An alias in a subquery of the select list, of `WHERE`, of a join.
SELECT a AS x, (SELECT x + 1) AS y FROM ar ORDER BY x;
SELECT a AS x, x + 1 AS y FROM ar ORDER BY x;
SELECT a AS x FROM ar WHERE (SELECT x) > 1;
SELECT a AS x FROM ar AS p JOIN ar AS q ON x = q.a ORDER BY 1;
-- A window read through its alias, by the expression of a key: MySQL 8.0.16
-- to 8.0.28 number a row twice.
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY r;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY r + 0, id;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY ROW_NUMBER() OVER (ORDER BY b) + 0;
SELECT id, ROW_NUMBER() OVER w AS r FROM ar WINDOW w AS (ORDER BY b) ORDER BY r + 0;
SELECT id, SUM(b) OVER (ORDER BY id) AS s FROM ar ORDER BY s + 0;
SELECT id, RANK() OVER (ORDER BY b) AS r FROM ar ORDER BY -r;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY CONCAT(r);
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r, ROW_NUMBER() OVER (ORDER BY b) + 0 AS q FROM ar ORDER BY id;
-- An alias in a window: the name of a column, an aggregate.
SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY b) AS r FROM ar ORDER BY id;
SELECT id, a AS x, ROW_NUMBER() OVER (ORDER BY x + 0) AS r FROM ar ORDER BY id;
SELECT a % 2 AS x, SUM(b) AS s, RANK() OVER (ORDER BY s) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, SUM(b) AS s, RANK() OVER (ORDER BY x DESC) AS r FROM ar GROUP BY x ORDER BY x;
SELECT id, a AS x, SUM(b) OVER (PARTITION BY x % 2) AS s FROM ar ORDER BY id;
-- A qualified name, an expression of a name of both, an alias written after
-- the window, in the `WINDOW` clause, in a window of `ORDER BY`, as an
-- argument; the alias of a window, of an aggregate.
SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY ar.b) AS r FROM ar ORDER BY id;
SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY b + 0) AS r FROM ar ORDER BY id;
SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY -b) AS r FROM ar ORDER BY id;
SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY (SELECT b)) AS r FROM ar ORDER BY id;
SELECT id, ROW_NUMBER() OVER (ORDER BY x) AS r, a AS x FROM ar ORDER BY id;
SELECT id, ROW_NUMBER() OVER (ORDER BY x + 0) AS r, a AS x FROM ar ORDER BY id;
SELECT id, a AS x, ROW_NUMBER() OVER (ORDER BY -x) AS r FROM ar ORDER BY id;
SELECT id, a AS x, ROW_NUMBER() OVER (ORDER BY (SELECT x)) AS r FROM ar ORDER BY id;
SELECT id, a AS x, SUM(id) OVER (PARTITION BY x) AS s FROM ar ORDER BY id;
SELECT id, a AS x, ROW_NUMBER() OVER w AS r FROM ar WINDOW w AS (ORDER BY x) ORDER BY id;
SELECT id, a AS x, ROW_NUMBER() OVER w AS r FROM ar WINDOW w AS (ORDER BY x + 0) ORDER BY id;
SELECT id, a AS b, ROW_NUMBER() OVER w AS r FROM ar WINDOW w AS (ORDER BY b) ORDER BY id;
SELECT id, a AS b, ROW_NUMBER() OVER w AS r FROM ar WINDOW w AS (ORDER BY -b) ORDER BY id;
SELECT id, a AS x FROM ar ORDER BY ROW_NUMBER() OVER (ORDER BY x);
SELECT id, a AS x FROM ar ORDER BY ROW_NUMBER() OVER (ORDER BY x + 0);
SELECT id, a AS b FROM ar ORDER BY ROW_NUMBER() OVER (ORDER BY b);
SELECT id, a AS b FROM ar ORDER BY ROW_NUMBER() OVER (ORDER BY -b);
SELECT id, a AS x, LAG(x) OVER (ORDER BY id) AS l FROM ar ORDER BY id;
SELECT id, a AS x, SUM(x + 0) OVER () AS s FROM ar ORDER BY id;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r, SUM(id) OVER (ORDER BY r) AS s FROM ar ORDER BY id;
SELECT id, ROW_NUMBER() OVER (ORDER BY b) AS r, SUM(id) OVER (ORDER BY r + 0) AS s FROM ar ORDER BY id;
SELECT id, ROW_NUMBER() OVER (ORDER BY r) AS r FROM ar ORDER BY id;
SELECT a % 2 AS x, SUM(b) AS s, RANK() OVER (ORDER BY s + 0) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, SUM(b) AS s, RANK() OVER (ORDER BY -x) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n, SUM(n) OVER () AS s FROM ar GROUP BY a % 2 ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n, SUM(n + 0) OVER () AS s FROM ar GROUP BY a % 2 ORDER BY x;
-- The alias of an aggregate inside an expression of a window and of
-- `ORDER BY`, of values that differ (1 and 2 rows, 30 and 20).
SELECT a % 2 AS x, COUNT(*) AS n, RANK() OVER (ORDER BY n + 0) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n, RANK() OVER (ORDER BY -n) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n, RANK() OVER (ORDER BY n + 0, -x) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n, RANK() OVER (ORDER BY COUNT(*) + 0, -x) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, COUNT(*) AS n, RANK() OVER (PARTITION BY n + 0 ORDER BY x) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, MAX(b) AS m, RANK() OVER (ORDER BY m + 0 DESC) AS r FROM ar GROUP BY x ORDER BY x;
SELECT a % 2 AS x, MAX(b) AS m, RANK() OVER w AS r FROM ar GROUP BY x WINDOW w AS (ORDER BY -m) ORDER BY x;
SELECT COUNT(*) AS n, RANK() OVER (ORDER BY n + 0) AS r FROM ar;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x ORDER BY n + 0 DESC, x;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x ORDER BY -n, x;
SELECT a % 2 AS x, MAX(b) AS m FROM ar GROUP BY x ORDER BY m + 0, x;
-- A qualified name in `HAVING`.
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2, b HAVING ar.b = 10 ORDER BY 1, 2;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2, ar.b HAVING b = 1 ORDER BY 1, 2;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2, b HAVING b + 0 = 1 ORDER BY 1, 2;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2, b HAVING (SELECT b) = 1 ORDER BY 1, 2;
SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY b, a % 2 HAVING MAX(b) = 10 ORDER BY 1, 2;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY a % 2 HAVING b = 10;
SELECT b AS x, COUNT(*) AS n FROM ar GROUP BY b HAVING x = 10;
SELECT b AS x, COUNT(*) AS n FROM ar GROUP BY x HAVING b = 10;
-- The alias of the grouping column itself, in another case, of another
-- table: read as the column.
SELECT b AS b, COUNT(*) AS n FROM ar GROUP BY b HAVING b = 10;
SELECT b AS B, COUNT(*) AS n FROM ar GROUP BY b HAVING b + 0 = 10;
SELECT ar.b AS b, COUNT(*) AS n FROM ar GROUP BY b HAVING b = 10;
SELECT p.b AS b, COUNT(*) AS n FROM ar AS p JOIN ar AS q ON q.id = p.id GROUP BY p.b, q.b HAVING b = 10;
-- Two aliases of a name that is also a column.
SELECT a AS b, id AS b FROM ar ORDER BY b + 0;
SELECT a AS b, id AS b FROM ar ORDER BY b;
SELECT a % 2 AS b, id % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2, id % 2 HAVING b = 1 ORDER BY 1, 2;
-- The outer `ORDER BY` of a query between parentheses.
(SELECT id, a AS b FROM ar) ORDER BY b;
(SELECT id, a AS b FROM ar) ORDER BY -b;
(SELECT id, a AS x FROM ar) ORDER BY x + 0;
(SELECT id, a AS b FROM ar) ORDER BY ar.b;
(SELECT id, a AS b FROM ar) ORDER BY (SELECT b);
(SELECT id, a AS b FROM ar) ORDER BY b + 0 LIMIT 2;
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY b + 0;
(SELECT id, a AS b FROM ar ORDER BY id LIMIT 3) ORDER BY b + 0;
(SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x) ORDER BY s + 0, x;
(SELECT a % 2 AS x, SUM(b) AS s FROM ar GROUP BY x) ORDER BY -x;
((SELECT id, a AS b FROM ar)) ORDER BY b + 0;
-- A name of the select list or of the source only, an inner `ORDER BY`, a
-- set operation, a table alias, a grouped query.
(SELECT id, a AS x FROM ar) ORDER BY -x;
(SELECT id, a AS b FROM ar) ORDER BY CONCAT(b);
(SELECT id, a AS b FROM ar) ORDER BY b DESC;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY -b;
(SELECT id, a AS b FROM ar WHERE id > 0) ORDER BY -b;
(SELECT id, a AS b FROM ar AS p) ORDER BY -b;
(SELECT id, a AS b FROM ar AS p) ORDER BY -p.b;
(SELECT id, a FROM ar) ORDER BY -b;
(SELECT id, a FROM ar) ORDER BY b;
(SELECT id FROM ar) ORDER BY a;
(SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0) ORDER BY -b;
(SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY a % 2) ORDER BY -n;
(SELECT a % 2 AS b, COUNT(*) AS n FROM ar GROUP BY a % 2) ORDER BY -b;
-- An inner `ORDER BY` without `LIMIT`, of another order than both readings.
(SELECT id, a AS b FROM ar ORDER BY id DESC) ORDER BY -b;
(SELECT id, a AS b FROM ar ORDER BY id DESC) ORDER BY b + 0;
(SELECT id, a AS x FROM ar ORDER BY id DESC) ORDER BY -x;
(SELECT id, a AS b FROM ar ORDER BY -id) ORDER BY b;
-- A grouped query of three groups: a grouping key alone, inside an
-- expression, an aggregate, a column of the source; `DISTINCT`, `HAVING`.
(SELECT a AS x, COUNT(*) AS n FROM ar GROUP BY a) ORDER BY x DESC;
(SELECT a AS x, COUNT(*) AS n FROM ar GROUP BY a) ORDER BY -x;
(SELECT a AS x, COUNT(*) AS n FROM ar GROUP BY a) ORDER BY x + 0 DESC;
(SELECT a AS x, COUNT(*) AS n FROM ar GROUP BY x) ORDER BY -x;
(SELECT a AS x FROM ar GROUP BY a) ORDER BY -x;
(SELECT a AS x, MAX(b) AS m FROM ar GROUP BY a) ORDER BY -m;
(SELECT a AS x, MAX(b) AS m FROM ar GROUP BY a) ORDER BY m + 0;
(SELECT a AS x, MAX(b) AS m FROM ar GROUP BY a) ORDER BY -MAX(b);
(SELECT a AS x, MAX(b) AS m FROM ar GROUP BY a) ORDER BY -a;
(SELECT a AS x, MAX(b) AS m FROM ar GROUP BY a) ORDER BY a DESC;
(SELECT a AS b, MAX(b) AS m FROM ar GROUP BY a) ORDER BY -b;
(SELECT a AS x FROM ar GROUP BY a HAVING x > 0) ORDER BY -x;
(SELECT DISTINCT a AS x FROM ar) ORDER BY -x;
(SELECT DISTINCT a AS x FROM ar) ORDER BY x DESC;
-- A column of the source only, inside an expression; a qualified alias.
(SELECT id FROM ar) ORDER BY a + 0;
(SELECT id FROM ar) ORDER BY -b;
(SELECT id, a AS x FROM ar) ORDER BY -b;
(SELECT id, a AS x FROM ar) ORDER BY ar.x;
-- A window, a position, an aggregate of the outer `ORDER BY`.
(SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY id DESC) AS r FROM ar) ORDER BY -r;
(SELECT id, a AS x FROM ar) ORDER BY ROW_NUMBER() OVER (ORDER BY x);
(SELECT id, a AS x FROM ar) ORDER BY ROW_NUMBER() OVER (ORDER BY -x);
(SELECT id, a AS b FROM ar) ORDER BY 2 DESC;
(SELECT id, a AS b FROM ar) ORDER BY SUM(b);
-- Inside a derived table, a common table expression, a subquery.
SELECT * FROM ((SELECT id, a AS b FROM ar) ORDER BY -b LIMIT 1) AS d;
WITH c AS ((SELECT id, a AS b FROM ar) ORDER BY -b LIMIT 1) SELECT * FROM c;
SELECT (SELECT id FROM ((SELECT id, a AS b FROM ar) ORDER BY -b LIMIT 1) AS d) AS v;
SELECT id FROM ar WHERE id IN ((SELECT id FROM ar) ORDER BY -b);
-- A grouping key, or an item, that is an expression.
(SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x) ORDER BY x DESC;
(SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x) ORDER BY x + 0 DESC;
(SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x) ORDER BY -n;
(SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY a % 2) ORDER BY -x;
(SELECT a + 0 AS x, COUNT(*) AS n FROM ar GROUP BY x) ORDER BY -x;
(SELECT a + 0 AS x, COUNT(*) AS n FROM ar GROUP BY a + 0) ORDER BY x DESC;
(SELECT a + 0 AS x FROM ar GROUP BY x) ORDER BY -x;
(SELECT a + 0 AS x FROM ar GROUP BY a) ORDER BY -x;
(SELECT a + 0 AS x FROM ar) ORDER BY -x;
(SELECT DISTINCT a + 0 AS x FROM ar) ORDER BY -x;
(SELECT a % 2 AS x FROM ar GROUP BY x) ORDER BY -x;
(SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x HAVING n > 0) ORDER BY -x;
SELECT * FROM (SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x) AS d ORDER BY -x;
SELECT a % 2 AS x, COUNT(*) AS n FROM ar GROUP BY x ORDER BY -x;
-- A subquery of the outer `ORDER BY`: a name of the result, of the source.
(SELECT id, a AS x FROM ar) ORDER BY (SELECT -x);
(SELECT id FROM ar) ORDER BY (SELECT -a);
(SELECT id, a AS b FROM ar) ORDER BY (SELECT -b);
(SELECT id, a AS b FROM ar) ORDER BY (SELECT b + 0);
(SELECT id, a AS b FROM ar) ORDER BY (SELECT MAX(b) FROM ar AS q WHERE q.id = b);
-- `WITH` between the parentheses.
(WITH c AS (SELECT id, a, b FROM ar) SELECT id, a AS b FROM c) ORDER BY b + 0;
(WITH c AS (SELECT id, a, b FROM ar) SELECT id FROM c) ORDER BY a;
