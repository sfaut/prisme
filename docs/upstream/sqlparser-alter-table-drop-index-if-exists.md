# sqlparser: `ALTER TABLE ... DROP INDEX IF EXISTS`

Draft of a ticket for [apache/datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs/issues),
to post from the maintainer's account. Prisme works around it in `crates/dialects/src/parse.rs`: after
the failure of `sqlparser`, the statement is parsed again by `crates/dialects/src/mysql_statements.rs`,
which reads the optional `IF EXISTS` itself.

---

**Title:** `ALTER TABLE ... DROP INDEX IF EXISTS name` (MariaDB) parses IF as the index name, then fails

### Description

MariaDB accepts `IF EXISTS` after `DROP {INDEX | KEY}` in `ALTER TABLE`, as it does after
`DROP COLUMN`, `DROP CONSTRAINT` and `DROP FOREIGN KEY`; MySQL does not (it is not in its
`ALTER TABLE` syntax). With no MariaDB dialect, MariaDB statements go through `MySqlDialect` or
`GenericDialect`. `sqlparser` 0.63.0 reads the `IF` keyword as the name of the index, then stops
at `EXISTS`:

```rust
use sqlparser::dialect::{GenericDialect, MySqlDialect};
use sqlparser::parser::Parser;

fn main() {
    for sql in [
        "ALTER TABLE t DROP INDEX IF EXISTS i",
        "ALTER TABLE t DROP INDEX i",
        "ALTER TABLE t DROP INDEX IF",
        "ALTER TABLE t DROP KEY i",
    ] {
        println!("{sql}");
        println!("  MySQL:   {:?}", Parser::parse_sql(&MySqlDialect {}, sql).map(|s| s[0].to_string()));
        println!("  Generic: {:?}", Parser::parse_sql(&GenericDialect {}, sql).map(|s| s[0].to_string()));
    }
}
```

With `sqlparser = "=0.63.0"`:

```
ALTER TABLE t DROP INDEX IF EXISTS i
  MySQL:   Err(ParserError("Expected: end of statement, found: EXISTS at Line: 1, Column: 29"))
  Generic: Err(ParserError("Expected: end of statement, found: EXISTS at Line: 1, Column: 29"))
ALTER TABLE t DROP INDEX i
  MySQL:   Ok("ALTER TABLE t DROP INDEX i")
  Generic: Ok("ALTER TABLE t DROP INDEX i")
ALTER TABLE t DROP INDEX IF
  MySQL:   Ok("ALTER TABLE t DROP INDEX IF")
  Generic: Ok("ALTER TABLE t DROP INDEX IF")
ALTER TABLE t DROP KEY i
  MySQL:   Err(ParserError("Expected: end of statement, found: i at Line: 1, Column: 24"))
  Generic: Err(ParserError("Expected: end of statement, found: i at Line: 1, Column: 24"))
```

`DROP KEY`, the synonym of `DROP INDEX` in MySQL and MariaDB, fails too: `KEY` is read as the name
of a column to drop.

The same statements on the servers:

```sql
CREATE TABLE t (a INT, b INT, KEY i (a), KEY j (b));
ALTER TABLE t DROP KEY j;
ALTER TABLE t DROP INDEX IF EXISTS i;
ALTER TABLE t DROP INDEX IF EXISTS i;
-- Note 1091: Can't DROP INDEX `i`; check that it exists
```

- MariaDB 10.5.29 and 11.4.2 accept them all: the first `DROP INDEX IF EXISTS` drops the index,
  the second one only gives the note.
- MySQL 8.4.11 and 9.3.0 accept `DROP KEY j`, and reject `DROP INDEX IF EXISTS i` (error 1064,
  near 'IF EXISTS i').

### Cause

`parse_alter_table_operation` (`src/parser/mod.rs`) parses `DROP INDEX` with
`parse_identifier` alone, and `AlterTableOperation::DropIndex` (`src/ast/ddl.rs`) only has a `name`.
Both are unchanged on `main` at the time of writing.

### Suggestion

Parse an optional `IF EXISTS` before the name, as `DROP CONSTRAINT` and `DROP PROJECTION` already do,
and carry it in `DropIndex { if_exists, name }`, displayed back as `DROP INDEX IF EXISTS name`.
`DROP KEY`, valid in MySQL and MariaDB, could take the same path as `DROP INDEX`.

### Environment

```
sqlparser: 0.63.0 (latest release)
Rust: 1.99.1
MySQL servers: 8.4.11, 9.3.0
MariaDB servers: 10.5.29, 11.4.2
```
