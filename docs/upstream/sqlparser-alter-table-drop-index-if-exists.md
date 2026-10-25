# sqlparser: `ALTER TABLE ... DROP INDEX IF EXISTS`

Draft of a ticket for [apache/datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs/issues),
to post from the maintainer's account. Prisme works around it in `crates/dialects/src/parse.rs`: after
the failure of `sqlparser`, the statement is parsed again by `crates/dialects/src/mysql_statements.rs`,
which reads the optional `IF EXISTS` itself.

---

**Title:** `ALTER TABLE ... DROP INDEX IF EXISTS name` (MariaDB) parses IF as the index name, then fails

### Description

MariaDB accepts `IF EXISTS` after `DROP {INDEX | KEY}` in `ALTER TABLE`, as it does after
`DROP COLUMN`, `DROP CONSTRAINT` and `DROP FOREIGN KEY`. `sqlparser` 0.63.0 reads the
`IF` keyword as the name of the index, then stops at `EXISTS`:

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

The same statement on MariaDB 11.4.2: the first run drops the index, the second one only gives a
note.

```sql
CREATE TABLE t (a INT, KEY i (a));
ALTER TABLE t DROP INDEX IF EXISTS i;
ALTER TABLE t DROP INDEX IF EXISTS i;
-- Note 1091: Can't DROP INDEX `i`; check that it exists
```

### Cause

`parse_alter_table_operation` (`src/parser/mod.rs`) parses `DROP INDEX` with
`parse_identifier` alone, and `AlterTableOperation::DropIndex` (`src/ast/ddl.rs`) only has a `name`.
Both are unchanged on `main` at the time of writing.

### Suggestion

Parse an optional `IF EXISTS` before the name, as `DROP CONSTRAINT` and `DROP PROJECTION` already do,
and carry it in `DropIndex { if_exists, name }`, displayed back as `DROP INDEX IF EXISTS name`.
For MySQL and MariaDB, `DROP KEY` could take the same path as `DROP INDEX`.

### Environment

```
sqlparser: 0.63.0 (latest release)
Rust: 1.99.1
MariaDB server: 11.4.2
```
