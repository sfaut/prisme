-- How each server reads `HAVING` without `GROUP BY`: the names it reads
-- (a column, an alias, an aggregate function), and where it filters
-- (before or after the window functions, `DISTINCT`, `LIMIT`).
DROP TABLE IF EXISTS gf;
CREATE TABLE gf (id INT NOT NULL PRIMARY KEY, a INT NOT NULL, b INT NULL);
INSERT INTO gf VALUES (1, 1, 10), (2, 1, 20), (3, 2, NULL), (4, 3, 40);
-- `HAVING` without grouping: a column, an alias, a column outside the
-- select list, an alias that names a column, an aggregate function.
SELECT id, a FROM gf HAVING a > 1 ORDER BY id;
SELECT id, a AS x FROM gf HAVING x > 1 ORDER BY id;
SELECT id FROM gf HAVING a > 1 ORDER BY id;
SELECT id, a AS b FROM gf HAVING b > 1 ORDER BY id;
SELECT id, a AS b FROM gf HAVING b + 0 > 1 ORDER BY id;
SELECT id, a AS b FROM gf HAVING gf.b > 15 ORDER BY id;
SELECT id, b FROM gf HAVING b IS NULL;
SELECT * FROM gf HAVING a > 1 ORDER BY id;
SELECT gf.* FROM gf HAVING b > 15 ORDER BY id;
SELECT COUNT(*) AS n FROM gf HAVING n > 1;
SELECT COUNT(*) AS n FROM gf HAVING n > 10;
SELECT id, COUNT(*) AS n FROM gf HAVING n > 1;
SELECT a FROM gf HAVING COUNT(*) > 1;
SELECT id FROM gf HAVING id = MAX(id);
SELECT 1 AS x HAVING x = 1;
SELECT 1 AS x FROM DUAL HAVING x = 2;
SELECT 1 AS x FROM gf HAVING x = 1;
-- Where it filters: with `WHERE`, `LIMIT`, `DISTINCT`, a window function,
-- a subquery, a qualified name, an alias of the table.
SELECT id, a FROM gf WHERE id > 1 HAVING a < 3 ORDER BY id;
SELECT id, a FROM gf HAVING a > 1 ORDER BY id LIMIT 1;
SELECT DISTINCT a FROM gf HAVING a > 1 ORDER BY a;
SELECT DISTINCT a AS x FROM gf HAVING x < 3 ORDER BY x;
SELECT id, a, ROW_NUMBER() OVER (ORDER BY id) AS r FROM gf HAVING r > 2 ORDER BY id;
SELECT id, a, ROW_NUMBER() OVER (ORDER BY id) AS r FROM gf HAVING a > 1 ORDER BY id;
SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS r FROM gf HAVING id > 2 ORDER BY id;
SELECT id FROM gf HAVING (SELECT a) > 1 ORDER BY id;
SELECT id, a AS x FROM gf HAVING (SELECT x) > 1 ORDER BY id;
SELECT id, a FROM gf HAVING gf.a > 1 ORDER BY id;
SELECT id, a FROM gf AS p HAVING p.a > 1 ORDER BY id;
SELECT id, a FROM gf HAVING id IN (SELECT id FROM gf WHERE a = 1) ORDER BY id;
SELECT id, a + 1 AS x FROM gf HAVING x = a + 1 ORDER BY id;
SELECT id, RAND(1) AS r FROM gf HAVING r < 2 ORDER BY id;
(SELECT id, a FROM gf HAVING a > 1) ORDER BY id;
SELECT id, a FROM gf HAVING a > 1 UNION ALL SELECT 9, 9 ORDER BY id;
SELECT * FROM (SELECT id, a FROM gf HAVING a > 1) AS d ORDER BY id;
-- The names `HAVING` reads without grouping: the column under an alias,
-- swapped aliases, duplicate items, a qualified item, a join.
SELECT id, a AS x FROM gf HAVING a > 1 ORDER BY id;
SELECT id, a AS x FROM gf HAVING gf.a > 1 ORDER BY id;
SELECT b AS a, a AS b FROM gf HAVING a > 15 ORDER BY b;
SELECT id, a, a AS a2 FROM gf HAVING a > 1 ORDER BY id;
SELECT a, a FROM gf HAVING a > 1 ORDER BY a;
SELECT id, a AS id FROM gf HAVING id > 1;
SELECT a AS x, b AS x FROM gf HAVING x > 1;
SELECT id, gf.a FROM gf HAVING a > 1 ORDER BY id;
SELECT id, A FROM gf HAVING a > 1 ORDER BY id;
SELECT id, a + 0 AS a FROM gf HAVING a > 1 ORDER BY id;
SELECT id, a + 0 AS a FROM gf HAVING gf.a > 1 ORDER BY id;
SELECT * FROM gf AS p JOIN gf AS q USING (id) HAVING id > 2 ORDER BY id;
SELECT * FROM gf AS p JOIN gf AS q USING (id) HAVING a > 1 ORDER BY id;
SELECT * FROM gf AS p JOIN gf AS q USING (id) HAVING q.a > 1 ORDER BY id;
SELECT p.id, q.a FROM gf AS p JOIN gf AS q ON q.id = p.id HAVING p.a > 1 ORDER BY p.id;
SELECT p.id, q.a FROM gf AS p JOIN gf AS q ON q.id = p.id HAVING a > 1 ORDER BY p.id;
SELECT p.id, q.a FROM gf AS p JOIN gf AS q ON q.id = p.id HAVING id > 2 ORDER BY p.id;
SELECT d.x FROM (SELECT a AS x FROM gf) AS d HAVING x > 1 ORDER BY x;
-- Subqueries in `HAVING` without grouping, and an alias that is a
-- subquery.
SELECT id, a FROM gf HAVING EXISTS (SELECT 1 FROM gf AS q WHERE q.b = gf.b) ORDER BY id;
SELECT id, a FROM gf HAVING (SELECT COUNT(*) FROM gf AS q WHERE q.a = gf.a) > 1 ORDER BY id;
SELECT id FROM gf HAVING id > (SELECT AVG(id) FROM gf) ORDER BY id;
SELECT id, (SELECT MAX(q.b) FROM gf AS q WHERE q.a = gf.a) AS m FROM gf HAVING m > 15 ORDER BY id;
SELECT id, b FROM gf HAVING b = (SELECT MAX(b) FROM gf);
SELECT id, a, b FROM gf WHERE b IS NOT NULL HAVING a < 3 ORDER BY id;
SELECT id, a FROM gf HAVING a > 1 WINDOW w AS (ORDER BY id) ORDER BY id;
SELECT id, a FROM gf HAVING a > 1 ORDER BY ROW_NUMBER() OVER (ORDER BY id DESC);
SELECT id, a FROM gf HAVING a > 1 ORDER BY a > 1, id;
-- More names of `HAVING` without grouping: an item without alias that is
-- an expression, an alias that a column item has as name, two aliases of
-- the same column.
SELECT a + 1 FROM gf HAVING `a + 1` > 2 ORDER BY 1;
SELECT a, b AS a FROM gf HAVING a > 1;
SELECT a AS x, a AS y FROM gf HAVING a > 1 ORDER BY x;
SELECT a AS x, a AS y FROM gf HAVING gf.a > 1 ORDER BY x;
SELECT id, a AS b, b AS c FROM gf HAVING b > 1 ORDER BY id;
SELECT id, b AS a FROM gf HAVING a > 15 ORDER BY id;
SELECT id, a AS x FROM gf AS p HAVING p.a > 1 ORDER BY id;
SELECT id, a FROM gf HAVING x > 1;
-- A `HAVING` without grouping in a subquery: a column of the enclosing
-- query, an item of the subquery.
SELECT id, (SELECT 1 FROM DUAL HAVING gf.a > 1) AS s FROM gf ORDER BY id;
SELECT id, (SELECT q.a FROM gf AS q WHERE q.id = gf.id HAVING gf.b > 15) AS s FROM gf ORDER BY id;
SELECT id, (SELECT q.a FROM gf AS q WHERE q.id = gf.id HAVING a > 1) AS s FROM gf ORDER BY id;
SELECT id, (SELECT q.b FROM gf AS q WHERE q.id = 5 - gf.id HAVING a > 1) AS s FROM gf ORDER BY id;
SELECT id, (SELECT q.b FROM gf AS q WHERE q.id = 5 - gf.id HAVING q.a > 1) AS s FROM gf ORDER BY id;
