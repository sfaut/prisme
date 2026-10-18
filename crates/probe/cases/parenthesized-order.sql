-- An `ORDER BY` after a query between parentheses with its own `ORDER BY` or
-- `LIMIT` (MDEV-29681, docs/sources/mariadb/MDEV-29681.md), and the derived
-- tables that stand for the parentheses. Orders of the rows: the alias b
-- (= a) ascending 3, 1, 2, descending 2, 1, 3; the column b of the table
-- ascending 1, 3, 2, descending 2, 3, 1; id 1, 2, 3. The last statements stop
-- the versions with the defect: they come last.
DROP TABLE IF EXISTS ar;
CREATE TABLE ar (id INT NOT NULL PRIMARY KEY, a INT NOT NULL, b INT NOT NULL);
INSERT INTO ar VALUES (1, 2, 10), (2, 3, 30), (3, 1, 20);
-- An inner ORDER BY without LIMIT.
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY b;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY 2 DESC;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY -b;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY b + 0;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY CONCAT(b);
(SELECT id, a AS x FROM ar ORDER BY id) ORDER BY -x;
(SELECT id, a FROM ar ORDER BY id) ORDER BY -a;
(SELECT id, b FROM ar ORDER BY id) ORDER BY -b;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY -id;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY -b LIMIT 2;
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY b DESC LIMIT 2;
((SELECT id, a AS b FROM ar ORDER BY id)) ORDER BY -b;
(SELECT ar.id, ar.a AS b FROM ar JOIN ar AS s ON s.id = ar.id ORDER BY ar.id) ORDER BY -b;
(SELECT DISTINCT id, a AS b FROM ar ORDER BY id) ORDER BY -b;
(SELECT a AS b, COUNT(*) AS n FROM ar GROUP BY a ORDER BY 1) ORDER BY -b;
(SELECT a AS b, COUNT(*) AS n FROM ar GROUP BY a ORDER BY 1) ORDER BY b DESC;
(SELECT a + 0 AS x, COUNT(*) AS n FROM ar GROUP BY a + 0 ORDER BY 1) ORDER BY -x;
-- An inner LIMIT.
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY b;
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY b DESC;
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY 2 DESC;
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY -b;
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY CONCAT(b);
(SELECT id, a AS x FROM ar LIMIT 3) ORDER BY -x;
(SELECT id, a FROM ar LIMIT 3) ORDER BY -a;
(SELECT id, b FROM ar LIMIT 3) ORDER BY -b;
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY -id;
(SELECT id, a AS b FROM ar ORDER BY id DESC LIMIT 3) ORDER BY -b;
(SELECT id, a AS b FROM ar ORDER BY id DESC LIMIT 3) ORDER BY b;
(SELECT id, a AS b FROM ar ORDER BY id LIMIT 3) ORDER BY -b LIMIT 2;
(SELECT ar.id, ar.a AS b FROM ar JOIN ar AS s ON s.id = ar.id LIMIT 3) ORDER BY -b;
(SELECT DISTINCT id, a AS b FROM ar LIMIT 3) ORDER BY -b;
(SELECT a AS b, COUNT(*) AS n FROM ar GROUP BY a LIMIT 5) ORDER BY -b;
(SELECT a + 0 AS x, COUNT(*) AS n FROM ar GROUP BY a + 0 LIMIT 5) ORDER BY -x;
(SELECT a + 0 AS x, COUNT(*) AS n FROM ar GROUP BY a + 0 LIMIT 5) ORDER BY x DESC;
-- Derived tables that stand for the parentheses.
SELECT * FROM (SELECT id, a AS b FROM ar ORDER BY id) AS `d` ORDER BY -b;
SELECT * FROM (SELECT id, a AS b FROM ar LIMIT 3) AS `d` ORDER BY -b;
SELECT * FROM (SELECT a + 0 AS x, COUNT(*) AS n FROM ar GROUP BY a + 0 LIMIT 5) AS `d` ORDER BY -x;
SELECT * FROM (SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 LIMIT 3) AS `d` ORDER BY -b;
SELECT * FROM (SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 ORDER BY id) AS `d` ORDER BY -b;
-- A set operation between parentheses.
(SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 LIMIT 3) ORDER BY -b;
(SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 LIMIT 3) ORDER BY b;
(SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 LIMIT 3) ORDER BY 2;
(SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 ORDER BY id) ORDER BY -b;
(SELECT id, a AS b FROM ar UNION ALL SELECT 4, 0 ORDER BY id) ORDER BY b;
(SELECT id, a AS b FROM ar UNION SELECT 4, 0 ORDER BY 1 LIMIT 3) ORDER BY 2 DESC;
-- A subquery: no defined order in every MariaDB.
(SELECT id, a AS b FROM ar ORDER BY id) ORDER BY (SELECT -b);
(SELECT id, a AS b FROM ar LIMIT 3) ORDER BY (SELECT -b);
(SELECT id, a AS x FROM ar ORDER BY id) ORDER BY (SELECT -x);
(SELECT id, a AS x FROM ar LIMIT 3) ORDER BY (SELECT -x);
(SELECT id, a AS b FROM ar ORDER BY id DESC LIMIT 3) ORDER BY (SELECT b);
SELECT * FROM (SELECT id, a AS b FROM ar ORDER BY id) AS `d` ORDER BY (SELECT -b);
SELECT * FROM (SELECT id, a AS b FROM ar LIMIT 3) AS `d` ORDER BY (SELECT -b);
SELECT * FROM (SELECT id, a AS x FROM ar LIMIT 3) AS `d` ORDER BY (SELECT -x);
-- A window function of the select list.
(SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY a) AS r FROM ar ORDER BY id) ORDER BY r;
(SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY a) AS r FROM ar LIMIT 3) ORDER BY r DESC;
SELECT * FROM (SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY a) AS r FROM ar LIMIT 3) AS `d` ORDER BY -r;
-- Each of the two stops the versions with the defect (signal 11).
(SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY a) AS r FROM ar LIMIT 3) ORDER BY -r;
(SELECT id, a AS b, ROW_NUMBER() OVER (ORDER BY a) AS r FROM ar ORDER BY id) ORDER BY -r;
