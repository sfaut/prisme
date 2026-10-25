# sqlparser: scope of `SET ... TRANSACTION`

Draft of a ticket for [apache/datafusion-sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs/issues),
to post from the maintainer's account. Prisme works around it in `crates/dialects/src/parse.rs`: before
`sqlparser`, `crates/dialects/src/mysql_statements.rs` parses `SET [GLOBAL | SESSION | LOCAL] TRANSACTION`
itself and keeps its scope.

---

**Title:** `SET GLOBAL TRANSACTION ...` parses to the same AST as `SET TRANSACTION ...` (scope dropped)

### Description

MySQL and MariaDB accept a scope before `TRANSACTION` in `SET`: `SET GLOBAL TRANSACTION` sets the default
of the sessions that open later, on the whole server; `SET SESSION TRANSACTION` the default of the current
session; `SET TRANSACTION`, without a scope, only the next transaction. `sqlparser` 0.63.0 parses the scope,
then drops it: the three statements give the same tree, displayed back as `SET TRANSACTION`.

```rust
use sqlparser::dialect::{GenericDialect, MySqlDialect};
use sqlparser::parser::Parser;

fn main() {
    for sql in [
        "SET GLOBAL TRANSACTION READ WRITE",
        "SET SESSION TRANSACTION READ WRITE",
        "SET TRANSACTION READ WRITE",
    ] {
        println!("{sql}");
        println!("  MySQL:   {:?}", Parser::parse_sql(&MySqlDialect {}, sql).map(|s| s[0].to_string()));
        println!("  Generic: {:?}", Parser::parse_sql(&GenericDialect {}, sql).map(|s| s[0].to_string()));
    }
    let global = Parser::parse_sql(&MySqlDialect {}, "SET GLOBAL TRANSACTION READ WRITE").unwrap();
    let plain = Parser::parse_sql(&MySqlDialect {}, "SET TRANSACTION READ WRITE").unwrap();
    println!("same AST: {}", global == plain);
}
```

With `sqlparser = "=0.63.0"`:

```
SET GLOBAL TRANSACTION READ WRITE
  MySQL:   Ok("SET TRANSACTION READ WRITE")
  Generic: Ok("SET TRANSACTION READ WRITE")
SET SESSION TRANSACTION READ WRITE
  MySQL:   Ok("SET TRANSACTION READ WRITE")
  Generic: Ok("SET TRANSACTION READ WRITE")
SET TRANSACTION READ WRITE
  MySQL:   Ok("SET TRANSACTION READ WRITE")
  Generic: Ok("SET TRANSACTION READ WRITE")
same AST: true
```

The round trip changes the meaning of the statement, and a tool that checks statements from their tree
(to tell a session setting from a server-wide one, for example) cannot see that `SET GLOBAL TRANSACTION`
changes the whole server.

The scope on the servers:

```sql
SET GLOBAL TRANSACTION READ ONLY;
SELECT @@global.transaction_read_only, @@session.transaction_read_only;
-- 1, 0
SET GLOBAL TRANSACTION READ WRITE;
```

The same on MySQL 8.4.11 and MariaDB 11.4.2: the global value changes, the session's stays.

### Cause

`parse_set` (`src/parser/mod.rs`) reads the scope with `parse_context_modifier`, then, after the keyword
`TRANSACTION`, builds `Set::SetTransaction { modes, snapshot, session: false }` without it; the `session`
field only marks `SET SESSION CHARACTERISTICS AS TRANSACTION` (PostgreSQL). Unchanged on `main` at the time
of writing (commit `72ce6a8`, 2026-10-25).

### Suggestion

Carry the scope in `Set::SetTransaction` (for example a `scope: Option<ContextModifier>`, as
`Set::SingleAssignment` has), and display it back: `SET GLOBAL TRANSACTION READ WRITE`.

### Environment

```
sqlparser: 0.63.0 (latest release)
Rust: 1.99.1
MySQL server: 8.4.11
MariaDB server: 11.4.2
```
