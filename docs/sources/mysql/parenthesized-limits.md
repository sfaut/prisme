# MySQL: limits of a query between parentheses

Documents consulted for `(SELECT ... LIMIT m) LIMIT n` and the order of a
query between parentheses under an outer limit
(`crates/probe/cases/parenthesized-limits.sql`).

## Bug #82560, "nested limit produces counter intuitive result"

Source: https://bugs.mysql.com/bug.php?id=82560

Reported on 5.5.51, 5.6.32 and 5.7.14, status Verified.

```sql
create table t1(c1 int primary key);
insert into t1 values(1), (2),(3), (4),(5);
(select * from t1 limit 1) limit 5;   -- 5 rows
```

The MySQL Verification Team (2016-08-12) answers that it is the documented
behaviour: "If LIMIT occurs within a subquery and also is applied in the outer
query, the outermost LIMIT takes precedence." A developer (2016-08-15/16) notes
that the parentheses make a parenthesized query expression, not a subquery,
and gives the derived table that keeps the inner limit:

```sql
select * from (select * from t1 limit 1) t1 limit 5;   -- 1 row
```

The reporter adds that an outer `ORDER BY` keeps the inner limit:
`(select * from t1 limit 1) order by c1 desc limit 3` gives 1 row.

## Bug #103954, "multiple LIMIT clauses maybe get wrong result"

Source: https://bugs.mysql.com/bug.php?id=103954

Reported on 5.7.32, 8.0.22, 8.0.25 and 5.7.34 (2021-06-09):
`(select * from R limit 1) limit 10` returns the 7 rows of the table. Closed
2022-06-17: "Fixed in MySQL 8.0.31 by WL#11350." (`8.0.31.md`, Optimizer Notes).

## MySQL 8.0.19, Bug #30521098 and Bug #30521803

Source: https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-19.html
(`8.0.19.md`, Bugs Fixed)

"Adding a `LIMIT` clause outside of a parenthesized query is intended to
override a `LIMIT` clause within the parentheses. However, the outer `LIMIT`
suppressed `ORDER BY` within the parentheses as well. [...] Now inner locking
and `ORDER BY` clauses are not suppressed by an outer `LIMIT` clause."

## Manual, Parenthesized Query Expressions (8.4)

Source: https://dev.mysql.com/doc/refman/8.4/en/parenthesized-query-expressions.html

"An `ORDER BY` or `LIMIT` within a parenthesized query expression which is
also applied in the outer query is handled in accordance with the SQL
standard." "[...] when collapsing parenthesized expression bodies, MySQL
follows SQL standard semantics, so that a higher outer limit cannot override
an inner lower one. For example, `(SELECT ... LIMIT 5) LIMIT 10` can return no
more than five rows."

```sql
(SELECT 'a' UNION SELECT 'b' LIMIT 2) LIMIT 3;   -- 2 rows
```

The manual says nothing of an outer `OFFSET` over an inner limit, which MySQL
8.0.31 to 9.3.0 read otherwise than the standard (measured; no ticket found
on bugs.mysql.com): `(SELECT a FROM lim ORDER BY a LIMIT 3) LIMIT 5 OFFSET 1`
gives 2, 3, 4, where MariaDB gives 2, 3.
