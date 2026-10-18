-- How each server reads `*` in a grouped query: the columns it gives,
-- the keys they depend on (`ONLY_FULL_GROUP_BY` of MySQL), the clauses that
-- name them.
DROP TABLE IF EXISTS gf;
CREATE TABLE gf (id INT NOT NULL PRIMARY KEY, a INT NOT NULL, b INT NULL);
INSERT INTO gf VALUES (1, 1, 10), (2, 1, 20), (3, 2, NULL), (4, 3, 40);
-- `*` grouped by the primary key, by another column, with an aggregate
-- function, qualified, after a join, over a derived table.
SELECT * FROM gf GROUP BY id ORDER BY id;
SELECT * FROM gf GROUP BY a ORDER BY a;
SELECT gf.* FROM gf GROUP BY id ORDER BY id;
SELECT *, COUNT(*) AS n FROM gf GROUP BY id ORDER BY id;
SELECT COUNT(*) AS n, gf.* FROM gf GROUP BY id ORDER BY id;
SELECT *, COUNT(*) AS n FROM gf GROUP BY a ORDER BY a;
SELECT * FROM gf GROUP BY id HAVING b > 10 ORDER BY id;
SELECT * FROM gf GROUP BY id, a ORDER BY id;
SELECT * FROM gf GROUP BY a, id ORDER BY id;
SELECT * FROM gf GROUP BY id + 0 ORDER BY id;
SELECT * FROM gf AS p JOIN gf AS q USING (id) GROUP BY id ORDER BY id;
SELECT p.*, q.a FROM gf AS p JOIN gf AS q ON q.id = p.id GROUP BY p.id ORDER BY p.id;
SELECT p.* FROM gf AS p JOIN gf AS q ON q.a = p.a GROUP BY p.id ORDER BY p.id;
SELECT * FROM (SELECT a, SUM(b) AS s FROM gf GROUP BY a) AS d GROUP BY a ORDER BY a;
SELECT * FROM (SELECT a, SUM(b) AS s FROM gf GROUP BY a) AS d GROUP BY s ORDER BY a;
SELECT * FROM gf GROUP BY id WITH ROLLUP;
SELECT DISTINCT * FROM gf GROUP BY id ORDER BY id;
SELECT * FROM gf GROUP BY id ORDER BY COUNT(*), id;
SELECT *, ROW_NUMBER() OVER (ORDER BY id DESC) AS r FROM gf GROUP BY id ORDER BY id;
-- More `*` in a grouped query: duplicate names, a natural join, `ORDER BY`
-- and `HAVING` on a column of the star, an alias as the key.
SELECT *, a FROM gf GROUP BY id ORDER BY id;
SELECT * FROM gf NATURAL JOIN gf AS q GROUP BY id ORDER BY id;
SELECT p.*, q.* FROM gf AS p JOIN gf AS q USING (id) GROUP BY p.id ORDER BY p.id;
SELECT * FROM gf GROUP BY id ORDER BY a, id;
SELECT * FROM gf GROUP BY id ORDER BY -a, id;
SELECT * FROM gf GROUP BY id HAVING COUNT(*) > 0 AND a > 1 ORDER BY id;
SELECT *, a AS x FROM gf GROUP BY x ORDER BY x;
SELECT *, id AS x FROM gf GROUP BY x ORDER BY x;
SELECT * FROM gf GROUP BY 1 ORDER BY id;
SELECT * FROM gf GROUP BY 2 ORDER BY id;
