-- Set operations between parentheses as operands of another: the right operand
-- refused by MySQL before 8.0.31 (error 1064 up to 8.0.18, 1235 from 8.0.19),
-- accepted by MariaDB; the left operand and a single query between
-- parentheses accepted everywhere.
SELECT 1 UNION (SELECT 2 UNION SELECT 3);
SELECT 1 UNION ALL (SELECT 2 UNION ALL SELECT 3);
SELECT 1 UNION (SELECT 2);
(SELECT 1 UNION SELECT 2) UNION SELECT 3;
SELECT 1 UNION (SELECT 2 ORDER BY 1 LIMIT 1);
SELECT 1 UNION ((SELECT 2 UNION SELECT 3));
