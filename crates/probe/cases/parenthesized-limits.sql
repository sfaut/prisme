-- Limit and order of a query between parentheses under an outer limit or
-- order, and the derived tables that could stand for it. The column of each
-- query is named after its case (`AS c_...`); the table holds 1 to 5.
DROP TABLE IF EXISTS lim;
CREATE TABLE lim (a INT);
INSERT INTO lim VALUES (1), (2), (3), (4), (5);
-- Outer limit over an inner limit: the standard keeps the smaller.
(SELECT a AS c_outer_larger FROM lim ORDER BY a LIMIT 2) LIMIT 5;
(SELECT a AS c_outer_smaller FROM lim ORDER BY a LIMIT 4) LIMIT 2;
((SELECT a AS c_twice FROM lim ORDER BY a LIMIT 3) LIMIT 2) LIMIT 5;
-- Offsets: inner, outer, both.
(SELECT a AS c_inner_offset FROM lim ORDER BY a LIMIT 2 OFFSET 1) LIMIT 5;
(SELECT a AS c_outer_offset FROM lim ORDER BY a LIMIT 3) LIMIT 5 OFFSET 1;
(SELECT a AS c_outer_offset_small FROM lim ORDER BY a LIMIT 3) LIMIT 1 OFFSET 1;
(SELECT a AS c_outer_offset_past FROM lim ORDER BY a LIMIT 3) LIMIT 5 OFFSET 3;
(SELECT a AS c_both_offsets FROM lim ORDER BY a LIMIT 3 OFFSET 1) LIMIT 1 OFFSET 1;
(SELECT a AS c_outer_offset_only FROM lim ORDER BY a LIMIT 3) LIMIT 18446744073709551615 OFFSET 1;
(SELECT a AS c_zero_offset FROM lim ORDER BY a LIMIT 2) LIMIT 5 OFFSET 0;
(SELECT a AS c_comma FROM lim ORDER BY a LIMIT 3) LIMIT 1, 5;
SELECT COUNT(*) AS c_past_count FROM ((SELECT a FROM lim ORDER BY a LIMIT 3) LIMIT 5 OFFSET 3) AS d;
-- Orders: inner order under an outer limit, outer order over an inner limit.
(SELECT a AS c_inner_order FROM lim ORDER BY a DESC) LIMIT 2;
(SELECT a AS c_outer_order FROM lim ORDER BY a LIMIT 2) ORDER BY 1 DESC;
(SELECT a AS c_orders FROM lim ORDER BY a LIMIT 3) ORDER BY 1 DESC LIMIT 2;
((SELECT a AS c_doubled FROM lim ORDER BY a LIMIT 2)) ORDER BY 1 DESC LIMIT 1;
(SELECT a AS c_order_offset FROM lim ORDER BY a LIMIT 3) ORDER BY 1 DESC LIMIT 5 OFFSET 1;
(SELECT a AS c_inner_order_only FROM lim ORDER BY a DESC) ORDER BY 1;
-- Set operation inside.
(SELECT a AS c_union_inside FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 2) LIMIT 5;
(SELECT a AS c_union_order FROM lim UNION SELECT 6 ORDER BY 1 DESC) LIMIT 2;
(SELECT a AS c_union_offset FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 3) LIMIT 5 OFFSET 1;
(SELECT a AS c_union_outer_order FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 2) ORDER BY 1 DESC;
(SELECT a AS c_union_outer_both FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 3) ORDER BY 1 DESC LIMIT 2;
(SELECT a AS c_union_limit_only FROM lim UNION SELECT 6 LIMIT 2) ORDER BY 1 DESC;
(SELECT a AS c_union_orders FROM lim UNION SELECT 6 ORDER BY 1) ORDER BY 1 DESC;
-- In a derived table, a common table expression, a right operand.
SELECT * FROM ((SELECT a AS c_derived FROM lim ORDER BY a LIMIT 2) LIMIT 5) AS d;
WITH c AS ((SELECT a AS c_cte FROM lim ORDER BY a LIMIT 2) LIMIT 5) SELECT * FROM c;
SELECT a AS c_right_operand FROM lim WHERE a = 1 UNION ALL ((SELECT a FROM lim ORDER BY a DESC LIMIT 2) LIMIT 5);
-- Derived tables standing for the parentheses.
SELECT * FROM (SELECT a AS c_dt_limits FROM lim ORDER BY a LIMIT 2) AS d LIMIT 5;
SELECT * FROM (SELECT a AS c_dt_offsets FROM lim ORDER BY a LIMIT 3) AS d LIMIT 5 OFFSET 1;
SELECT * FROM (SELECT a AS c_dt_order FROM lim ORDER BY a DESC) AS d LIMIT 2;
SELECT * FROM (SELECT a AS c_dt_orders FROM lim ORDER BY a LIMIT 3) AS d ORDER BY 1 DESC LIMIT 2;
SELECT * FROM (SELECT a AS c_dt_union FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 2) AS d LIMIT 5;
SELECT * FROM (SELECT a AS c_dt_union_order FROM lim UNION SELECT 6 ORDER BY 1 DESC) AS d LIMIT 2;
SELECT * FROM (SELECT a AS c_dt_offset_union FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 3) AS d LIMIT 5 OFFSET 1;
SELECT * FROM (SELECT a AS c_dt_union_outer_order FROM lim UNION SELECT 6 ORDER BY 1 LIMIT 2) AS d ORDER BY 1 DESC;
SELECT COUNT(*) AS c_dt_past_count FROM (SELECT * FROM (SELECT a FROM lim ORDER BY a LIMIT 3) AS d LIMIT 5 OFFSET 3) AS e;
SELECT * FROM (SELECT a AS c_dt_name FROM lim ORDER BY a LIMIT 3) AS d ORDER BY c_dt_name DESC LIMIT 2;
-- Operands with their own limit, then the limit of the whole.
(SELECT a AS c_operands FROM lim ORDER BY a LIMIT 2) UNION ALL (SELECT a FROM lim ORDER BY a DESC LIMIT 1) LIMIT 5;
(SELECT a AS c_operand_order FROM lim ORDER BY a LIMIT 2) UNION ALL SELECT 9 ORDER BY 1 DESC LIMIT 2;
