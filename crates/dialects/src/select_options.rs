//! Options of `SELECT` in the MySQL family (`SELECT SQL_NO_CACHE a FROM t`):
//! which ones each dialect has, and where it takes them, alike in each
//! local image of MySQL 8.0.11 to 9.3.0 and MariaDB 10.5.29 to 11.8.9
//! (`crates/probe/cases/select-options.sql`). Read by the parser, which
//! refuses an option where the server of the input refuses it, and by the
//! emitter, which writes it where the output takes it.
//!
//! Both dialects take `STRAIGHT_JOIN`, `SQL_SMALL_RESULT` and
//! `SQL_BIG_RESULT` on any `SELECT`; `HIGH_PRIORITY`, `SQL_BUFFER_RESULT`
//! and `SQL_CALC_FOUND_ROWS` on the first `SELECT` of the statement only,
//! error 1234 ("incorrect usage/placement") elsewhere: a subquery, a derived
//! table, a common table expression, another operand of a set operation, a
//! `SELECT` after a `VALUES` or a `TABLE`. MySQL has no query cache:
//! `SQL_CACHE` is a name there (`SELECT SQL_CACHE a` reads the column
//! `SQL_CACHE` under the alias `a`), and it takes `SQL_NO_CACHE` on any
//! `SELECT`, deprecated (warning 1681). MariaDB takes `SQL_CACHE` and
//! `SQL_NO_CACHE` on the first `SELECT` only, and refuses the options of the
//! first `SELECT` between parentheses followed by an `ORDER BY` or a
//! `LIMIT`, unless the parentheses hold a set operation
//! (`(SELECT SQL_CALC_FOUND_ROWS a FROM t LIMIT 2) LIMIT 1`, which MySQL
//! takes).

use prisme_plan::SelectOption;

use crate::Dialect;

/// Options of `SELECT`.
pub(crate) const OPTIONS: [SelectOption; 8] = [
    SelectOption::HighPriority,
    SelectOption::StraightJoin,
    SelectOption::SmallResult,
    SelectOption::BigResult,
    SelectOption::BufferResult,
    SelectOption::NoCache,
    SelectOption::Cache,
    SelectOption::CalcFoundRows,
];

/// Tells whether `dialect` has `option`.
pub(crate) fn has(dialect: Dialect, option: SelectOption) -> bool {
    option != SelectOption::Cache || dialect == Dialect::MariaDb
}

/// Tells whether `dialect` takes `option` on the first `SELECT` of the
/// statement only.
pub(crate) const fn first_only(dialect: Dialect, option: SelectOption) -> bool {
    match option {
        SelectOption::HighPriority | SelectOption::BufferResult | SelectOption::CalcFoundRows => {
            true
        }
        SelectOption::NoCache | SelectOption::Cache => matches!(dialect, Dialect::MariaDb),
        SelectOption::StraightJoin | SelectOption::SmallResult | SelectOption::BigResult => false,
    }
}
