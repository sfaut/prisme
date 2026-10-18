# MariaDB: locking reads

Tickets and pages of the documentation consulted for the locking clauses of `SELECT`
(`crates/probe/cases/locking.sql`).

## MDEV-13115

Source: https://jira.mariadb.org/browse/MDEV-13115

- Summary: Implement `SELECT [FOR UPDATE|LOCK IN SHARED MODE] SKIP LOCKED`.
- Fix version: 10.6.0. The description cites MySQL's WL #3597
  (https://dev.mysql.com/worklog/task/?id=3597).

## MDEV-25433

Source: https://jira.mariadb.org/browse/MDEV-25433

- Summary: `SKIP LOCKED` should imply `NOWAIT`.
- Fix version: 10.6.0. Description: "So remove the WAIT / NOWAIT
  compatibility and add to tests."

## `WAIT` and `NOWAIT`

Source: https://mariadb.com/kb/en/wait-and-nowait/

> `SELECT ... FOR UPDATE [WAIT n|NOWAIT]`
> `SELECT ... LOCK IN SHARE MODE [WAIT n|NOWAIT]`

> The lock wait timeout can be explicitly set in the statement by using
> either `WAIT n` (to set the wait in seconds) or `NOWAIT`, in which case the
> statement will immediately fail if the lock cannot be obtained. `WAIT 0` is
> equivalent to `NOWAIT`.

The page gives no version.

## MDEV-11379

Source: https://jira.mariadb.org/browse/MDEV-11379

- Summary: AliSQL: [Feature] Issue#8: `SELECT FOR UPDATE WAIT`.
- Fix version: 10.3.0, before the first version measured, 10.5.29. Usage
  given by the description:

  ```
  1.select ... for update [wait [n]|no_wait]
  2.select ... lock in share mode [wait [n]|no_wait]
  3.lock table ... [wait [n]|no_wait]
  ```

## `SELECT`

Source: https://mariadb.com/docs/server/reference/sql-statements/data-manipulation/selecting-data/select

```
[LIMIT {[offset,] row_count | row_count OFFSET offset}]
[PROCEDURE procedure_name(argument_list)]
[INTO OUTFILE 'file_name' [CHARACTER SET charset_name] [export_options] | ...]
[FOR UPDATE lock_option | LOCK IN SHARE MODE lock_option]

lock_option:
    [WAIT n | NOWAIT | SKIP LOCKED]
```

> `SKIP LOCKED`: This causes rows that couldn't be locked (`LOCK IN SHARE
> MODE` or `FOR UPDATE`) to be excluded from the result set. This is only
> implemented on InnoDB tables and ignored otherwise. An explicit `NOWAIT` is
> implied here.
