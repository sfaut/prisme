# MySQL: locking reads

Pages of the manual consulted for the locking clauses of `SELECT`
(`crates/probe/cases/locking.sql`).

## `SELECT` statement

Source: https://dev.mysql.com/doc/refman/8.4/en/select.html

```
[FOR {UPDATE | SHARE}
    [OF tbl_name [, tbl_name] ...]
    [NOWAIT | SKIP LOCKED]
  | LOCK IN SHARE MODE]
```

> `FOR SHARE` and `LOCK IN SHARE MODE` set shared locks that permit other
> transactions to read the examined rows but not to update or delete them.
> `FOR SHARE` and `LOCK IN SHARE MODE` are equivalent. However, `FOR SHARE`,
> like `FOR UPDATE`, supports `NOWAIT`, `SKIP LOCKED`, and `OF tbl_name`
> options. `FOR SHARE` is a replacement for `LOCK IN SHARE MODE`, but
> `LOCK IN SHARE MODE` remains available for backward compatibility.

> `OF tbl_name` applies `FOR UPDATE` and `FOR SHARE` queries to named tables.
> For example: `SELECT * FROM t1, t2 FOR SHARE OF t1 FOR UPDATE OF t2;`
>
> All tables referenced by the query block are locked when `OF tbl_name` is
> omitted. Consequently, using a locking clause without `OF tbl_name` in
> combination with another locking clause returns an error. Specifying the
> same table in multiple locking clauses returns an error. If an alias is
> specified as the table name in the `SELECT` statement, a locking clause may
> only use the alias. If the `SELECT` statement does not specify an alias
> explicitly, the locking clause may only specify the actual table name.

The `INTO` clause can be written after the select list, after `LIMIT`, or
after the locking clause, once.

## Locking reads

Source: https://dev.mysql.com/doc/refman/8.4/en/innodb-locking-reads.html

> A locking read clause in an outer statement does not lock the rows of a
> table in a nested subquery unless a locking read clause is also specified
> in the subquery. For example, the following statement does not lock rows
> in table `t2`.
>
> ```sql
> SELECT * FROM t1 WHERE c1 = (SELECT c1 FROM t2) FOR UPDATE;
> ```

> A locking read that uses `NOWAIT` never waits to acquire a row lock. The
> query executes immediately, failing with an error if a requested row is
> locked. A locking read that uses `SKIP LOCKED` never waits to acquire a row
> lock. The query executes immediately, removing locked rows from the result
> set.

## Release notes of MySQL 8.0.1

Source: https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-1.html

> **InnoDB:** `InnoDB` now supports `NOWAIT` and `SKIP LOCKED` options with
> `SELECT ... FOR SHARE` and `SELECT ... FOR UPDATE` locking read statements.
> [...]
> `SELECT ... FOR SHARE` replaces `SELECT ... LOCK IN SHARE MODE`, but
> `LOCK IN SHARE MODE` remains available for backward compatibility. The
> statements are equivalent. However, `FOR SHARE` supports `NOWAIT`,
> `SKIP LOCKED`, and `OF tbl_name` options.
>
> `OF tbl_name` applies locking queries to named tables.
>
> Note: `OF` now is a reserved word and cannot be used as an identifier
> without identifier quoting. (WL #8919, WL #3597)

The first version measured, 8.0.11, has all these clauses.
