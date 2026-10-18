-- Grammar of the variables in each server: `@` alone and the empty name, the
-- extent of the value of an assignment (`1 + @x := 2 * 3`), the names after
-- `@@`, the assignments refused.
-- `@` alone before a word, and the empty names, with their column names.
SELECT @ a;
SELECT @ 'a';
SELECT @ `a`;
SELECT @'';
SELECT @``;
SELECT @/*c*/a;
SELECT @ x := 1;
SELECT @'' := 1;
SELECT @'q' := 1, @q;
-- Right side of an assignment.
SELECT 1 + @x := 2 * 3, @x;
SELECT @x := 1 OR 0, @x;
SELECT @x := 0 OR 1 AND 0, @x;
SELECT @x := @y := 2, @x, @y;
SELECT - @x := 2, @x;
SELECT NOT @x := 0, @x;
SELECT @x := 2 IS NULL, @x;
SELECT (@x := 2) IS NULL, @x;
SELECT @x := 5 BETWEEN 1 AND 3, @x;
SELECT @x := 2 AS a, @x;
SELECT @x := 'a' COLLATE utf8mb4_bin, @x;
SELECT @x := 3 IN (1, 3), @x;
SELECT @x := 2 > 1 + 1, @x;
SELECT @x := CASE WHEN 1 THEN 7 END, @x;
SELECT @x := (SELECT 8), @x;
SELECT @x:=9;
SELECT @x := NULL, @x;
SELECT 2 * @x := 3 + 1, @x;
SELECT @x := 1 = @x := 1;
SELECT @x := 4 | 1, @x;
SELECT 1 < @x := 2, @x;
SELECT ~ @x := 1, @x;
SELECT @x := - 4, @x;
SELECT @x := INTERVAL 1 DAY + DATE '2024-01-01', @x;
SELECT @x := DATE '2024-01-01' + INTERVAL 1 DAY, @x;
-- Names after @@.
SELECT @@read_only;
SELECT @@`select`;
SELECT @@global.select;
SELECT @@global.`select`;
SELECT @@session.SQL_MODE = @@SQL_MODE;
SELECT @@SESSION.sql_select_limit, @@Local.sql_select_limit, @@GLOBAL.sql_select_limit;
SELECT @@session . sql_select_limit;
SELECT @@global.hot_cache.key_buffer_size;
SELECT @@global.default.key_buffer_size;
SELECT @@x.y.z;
SELECT @@global;
SELECT @@session;
SELECT @@sql_select_limit + 1, - @@sql_select_limit;
SELECT @@max_execution_time;
SELECT @@max_statement_time;
-- Assignments to a system variable or a column.
SELECT @@sql_select_limit := 1;
SELECT a := 1 FROM (SELECT 1 AS a) AS t;
-- `@` alone is the variable of an empty name, and the word after it its
-- alias: values set first.
SET @a = 7, @`` = 9;
SELECT @ a, @a;
SELECT @ a AS b;
SELECT @;
SELECT @ AS b;
SELECT @ + 1 AS b;
SELECT 1 + @ AS b;
SELECT @'' AS b, @`` AS c;
SELECT @ := 3;
SELECT @/*c*/a;
