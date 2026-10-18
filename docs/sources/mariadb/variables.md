# MariaDB: user and system variables

Pages of the documentation consulted for the variables read in an expression
(`crates/probe/cases/variables.sql`).

## User-defined variables

Source: https://mariadb.com/docs/server/reference/sql-structure/sql-language-structure/user-defined-variables

- "the only allowed characters are ASCII letters, digits, dollar sign (`$`),
  underscore (`_`) and dot (`.`)." Measured otherwise: `@é` is read on each
  local image, 10.5.29 to 11.8.9.
- "If other characters are used, the name can be quoted [...] These
  characters can be escaped as usual."
- "User-variables names are case insensitive."
- No maximum length is given: a name of 65 characters is set on each local
  image, refused by MySQL (error 3061).
- Not in the documentation: `@` alone is the variable of an empty name, as
  `@''` (`SELECT @ a` is that variable, named `a` by its alias), up to
  10.6.28; a syntax error from 10.7.1, the first image of 10.7, measured.
- "They can be read even if no value has been set yet; in that case, they
  are `NULL`."
- "Since user-defined variables type cannot be declared, the only way to
  force their type is using `CAST()` or `CONVERT()`."
- "It is unsafe to read a user-defined variable and set its value in the
  same statement (unless the command is `SET`), because the order of these
  actions is undefined."
- "User-defined variables can be used in most MariaDB's statements and
  clauses which accept an SQL expression. However there are some exceptions,
  like the `LIMIT` clause."
- "User-defined variables can be viewed by either querying the
  `USER_VARIABLES`, or by running `SHOW USER_VARIABLES`."

## MDEV-21921

Source: https://jira.mariadb.org/browse/MDEV-21921

- Title: "Make transaction_isolation into a system variable". Fix version:
  11.1.1.
- "The transaction_isolation option can be set in a configuration file, but
  it cannot be set dynamically." "The tx_isolation system variable can be
  set dynamically, but it cannot be set in a configuration file."
- "Same has been done to transaction_read_only option".
- Measured: `transaction_isolation` and `transaction_read_only` absent up to
  11.0.5, present from 11.1.2 (no image of 11.1.1); `tx_isolation` and
  `tx_read_only` present on each local image, 10.5.29 to 11.8.9.
