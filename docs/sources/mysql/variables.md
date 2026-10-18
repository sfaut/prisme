# MySQL: user and system variables

Pages of the manual consulted for the variables read in an expression
(`crates/probe/cases/variables.sql`).

## User-defined variables

Source: https://dev.mysql.com/doc/refman/8.4/en/user-variables.html

> User variables are written as `@var_name`, where the variable name
> `var_name` consists of alphanumeric characters, `.`, `_`, and `$`. A user
> variable name can contain other characters if you quote it as a string or
> identifier (for example, `@'my-var'`, `@"my-var"`, or `` @`my-var` ``).

> User variable names are not case-sensitive. Names have a maximum length of
> 64 characters.

> User variables can be assigned a value from a limited set of data types:
> integer, decimal, floating-point, binary or nonbinary string, or `NULL`
> value. Assignment of decimal and real values does not preserve the
> precision or scale of the value. A value of a type other than one of the
> permissible types is converted to a permissible type. For example, a value
> having a temporal or spatial data type is converted to a binary string. A
> value having the `JSON` data type is converted to a string with a character
> set of `utf8mb4` and a collation of `utf8mb4_bin`.

> Hexadecimal or bit values assigned to user variables are treated as binary
> strings.

> If you refer to a variable that has not been initialized, it has a value of
> `NULL` and a type of string.

> In addition, the default result type of a variable is based on its type at
> the beginning of the statement. This may have unintended effects if a
> variable holds a value of one type at the beginning of a statement in which
> it is also assigned a new value of a different type.

> Previous releases of MySQL made it possible to assign a value to a user
> variable in statements other than `SET`. This functionality is supported in
> MySQL 8.4 for backward compatibility but is subject to removal in a future
> release of MySQL.

> When making an assignment in this way, you must use `:=` as the assignment
> operator; `=` is treated as the comparison operator in statements other
> than `SET`.

> The order of evaluation for expressions involving user variables is
> undefined. For example, there is no guarantee that `SELECT @a, @a:=@a+1`
> evaluates `@a` first and then performs the assignment.

> User variables may be used in most contexts where expressions are
> permitted. This does not currently include contexts that explicitly require
> a literal value, such as in the `LIMIT` clause of a `SELECT` statement, or
> the `IGNORE N LINES` clause of a `LOAD DATA` statement.

## Release notes

- 5.7.20 (https://dev.mysql.com/doc/relnotes/mysql/5.7/en/news-5-7-20.html):
  "`transaction_isolation` and `transaction_read_only` have been created as
  aliases for `tx_isolation` and `tx_read_only`. The `tx_isolation` and
  `tx_read_only` variables are now deprecated and will be removed in MySQL
  8.0. (WL #9636)"
- 8.0.3 (https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-3.html):
  "The deprecated `tx_isolation` and `tx_read_only` system variables have
  been removed. Use `transaction_isolation` and `transaction_read_only`
  instead. (WL #9636)"
- 8.0.13 (https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-13.html):
  "Setting user variables in statements other than `SET` is now deprecated
  [...] Syntax such as `SELECT @var, @var:=@var+1` is still accepted in
  MySQL 8.0 for backward compatibility, but is subject to removal in a future
  MySQL version. (WL #12131)". Warning 1287 from 8.0.13, measured.
- 8.0.22 (https://dev.mysql.com/doc/relnotes/mysql/8.0/en/news-8-0-22.html):
  "A prepared statement is now prepared only once, when executing `PREPARE`".
  The metadata of a user variable change with it (`VAR_STRING` for an unset
  variable, a length of 21 for an integer), measured.

## System variables

Source: https://dev.mysql.com/doc/refman/8.4/en/using-system-variables.html

> You can also use system variable values in expressions.

> System variables implemented by a component are exposed when the component
> is installed and have names that begin with a component-specific prefix.
> For example, the `log_filter_dragnet` error log filter component implements
> a system variable named `log_error_filter_rules`, the full name of which is
> `dragnet.log_error_filter_rules`. To refer to this variable, use the full
> name.

Source: https://dev.mysql.com/doc/refman/8.4/en/structured-system-variables.html

> To refer to a component of a structured variable instance, you can use a
> compound name in `instance_name.component_name` format. Examples:
> `hot_cache.key_buffer_size`, `hot_cache.key_cache_block_size`,
> `cold_cache.key_cache_block_size`.

> For each structured system variable, an instance with the name of
> `default` is always predefined. If you refer to a component of a
> structured variable without any instance name, the `default` instance is
> used. Thus, `default.key_buffer_size` and `key_buffer_size` both refer to
> the same system variable.

> To retrieve the cache size, do this: `SELECT
> @@GLOBAL.hot_cache.key_buffer_size;`
