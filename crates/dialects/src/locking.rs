//! Rows that a locking clause next to a query between parentheses locks,
//! per server, measured on each local image of MySQL 8.0.11 to 9.3.0 and
//! MariaDB 10.5.29 to 11.8.9 (`crates/probe/cases/locking-scope.sql`).
//! Read by the builder, which refuses a clause that locks no row in the
//! input, and by the emitter, which writes the clause where the output
//! locks the rows of the plan.
//!
//! A clause after the `ORDER BY` or the `LIMIT` that follow the parentheses
//! (`(SELECT a FROM t LIMIT 2) ORDER BY a FOR UPDATE`), or right after them,
//! locks the rows of the query between them (of its last operand for a set
//! operation) when the server joins the outer clauses to those of that
//! query; otherwise the outer clauses make a block of their own, without
//! table, and the clause locks no row:
//!
//! - MariaDB joins them when the query has no `LIMIT` of its own, and not
//!   an `ORDER BY` on both sides;
//! - MySQL up to 8.0.17 joins an outer `LIMIT` without `ORDER BY` alone;
//!   from 8.0.18, every outer clause, except an outer `LIMIT` over a set
//!   operation with its own `LIMIT` in 8.0.18, which 8.0.19 to 8.0.30
//!   refuse (error 1235, construct `parenthesized_set_limit`).
//!
//! A clause at the end of the query between parentheses
//! (`(SELECT a FROM t FOR UPDATE) LIMIT 1`) locks its rows, except in MySQL
//! up to 8.0.18 before an outer `LIMIT` without `ORDER BY`, which drops it
//! (Bug #30521098, fixed in 8.0.19). A derived table locks the rows of a
//! clause written inside it on every server.
//!
//! MySQL 8.0.19 to 8.0.25 give every row of a `SELECT` between parentheses
//! with its own `ORDER BY`, without `LIMIT`, and a locking clause, inside or
//! after the parentheses, the outer `LIMIT` left aside
//! (`(SELECT a FROM t ORDER BY a) LIMIT 2 FOR UPDATE`, construct
//! `parenthesized_locked_limit`): the plan joins that `LIMIT` to the block,
//! which the emitter writes without parentheses.

use crate::versions::Construct;

/// Clauses of a query between parentheses, and of the query around it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    /// The query around has an `ORDER BY`.
    pub outer_sort: bool,
    /// The query around has a `LIMIT`.
    pub outer_limit: bool,
    /// The query between parentheses has its own `ORDER BY`.
    pub inner_sort: bool,
    /// The query between parentheses has its own `LIMIT`.
    pub inner_limit: bool,
    /// The query between parentheses is a set operation.
    pub inner_set: bool,
}

impl Shape {
    /// Tells whether a locking clause after the outer clauses, or right
    /// after the parentheses, locks the rows of the query between them, on
    /// a server that has the constructs `has` tells.
    pub(crate) fn locked_after(self, has: impl Fn(Construct) -> bool) -> bool {
        if self.outer_sort && (self.inner_sort || self.inner_limit) {
            has(Construct::ParenthesizedOrderLock)
        } else if self.outer_sort && self.inner_set {
            has(Construct::ParenthesizedSetOrderLock)
        } else if !self.outer_sort && self.inner_limit && self.inner_set {
            has(Construct::ParenthesizedSetLimitLock)
        } else if !self.outer_sort && self.inner_limit {
            has(Construct::ParenthesizedLimitLock)
        } else {
            true
        }
    }

    /// Tells whether a locking clause at the end of the query between
    /// parentheses locks its rows, on a server that has the constructs
    /// `has` tells.
    pub(crate) fn locked_inside(self, has: impl Fn(Construct) -> bool) -> bool {
        !self.outer_limit || self.outer_sort || has(Construct::ParenthesizedInnerLock)
    }

    /// Tells whether the outer `LIMIT` bounds the rows of a locked `SELECT`
    /// between parentheses of this shape, on a server that has the
    /// constructs `has` tells.
    pub(crate) fn limited_when_locked(self, has: impl Fn(Construct) -> bool) -> bool {
        let sorted_alone = self.inner_sort && !self.inner_limit && !self.inner_set;
        !(self.outer_limit && !self.outer_sort && sorted_alone)
            || has(Construct::ParenthesizedLockedLimit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dialect;
    use crate::versions::{Version, registry};

    /// What `has` of `dialect` in `version` answers.
    fn has(dialect: Dialect, version: Version) -> impl Fn(Construct) -> bool {
        move |construct| registry(dialect).check(dialect, construct, version).is_ok()
    }

    /// Shape of `inner`, the clauses of the query between parentheses among
    /// `ORDER`, `LIMIT` and `SET`, followed by `outer`, among `ORDER` and
    /// `LIMIT`.
    fn shape(inner: &[&str], outer: &[&str]) -> Shape {
        Shape {
            outer_sort: outer.contains(&"ORDER"),
            outer_limit: outer.contains(&"LIMIT"),
            inner_sort: inner.contains(&"ORDER"),
            inner_limit: inner.contains(&"LIMIT"),
            inner_set: inner.contains(&"SET"),
        }
    }

    #[test]
    fn a_clause_after_the_outer_clauses_locks_as_measured() {
        let mariadb = has(Dialect::MariaDb, Version::new(10, 6, 3));
        let old = has(Dialect::MySql, Version::new(8, 0, 17));
        let mysql_8_0_18 = has(Dialect::MySql, Version::new(8, 0, 18));
        let mysql = has(Dialect::MySql, Version::new(8, 0, 31));
        for (inner, outer, expected) in [
            (&[][..], &["ORDER"][..], [true, true, true, true]),
            (&["ORDER"], &["LIMIT"], [true, true, true, true]),
            (&["ORDER"], &["ORDER"], [false, false, true, true]),
            (&["LIMIT"], &["ORDER"], [false, false, true, true]),
            (&["LIMIT"], &["LIMIT"], [false, true, true, true]),
            (&["LIMIT"], &[], [false, true, true, true]),
            (&["SET"], &["ORDER"], [true, false, true, true]),
            (&["SET"], &["ORDER", "LIMIT"], [true, false, true, true]),
            (&["SET"], &["LIMIT"], [true, true, true, true]),
            (&["SET", "ORDER"], &["LIMIT"], [true, true, true, true]),
            (&["SET", "ORDER"], &["ORDER"], [false, false, true, true]),
            (&["SET", "LIMIT"], &["ORDER"], [false, false, true, true]),
            (
                &["SET", "ORDER", "LIMIT"],
                &["LIMIT"],
                [false, true, false, true],
            ),
            (&["SET", "LIMIT"], &[], [false, true, false, true]),
        ] {
            let shape = shape(inner, outer);
            let found = [
                shape.locked_after(&mariadb),
                shape.locked_after(&old),
                shape.locked_after(&mysql_8_0_18),
                shape.locked_after(&mysql),
            ];
            assert_eq!(found, expected, "{inner:?} {outer:?}");
        }
    }

    #[test]
    fn an_outer_limit_bounds_a_locked_select_as_measured() {
        let sorted = shape(&["ORDER"], &["LIMIT"]);
        for (patch, expected) in [(18, true), (19, false), (25, false), (26, true)] {
            let mysql = has(Dialect::MySql, Version::new(8, 0, patch));
            assert_eq!(sorted.limited_when_locked(&mysql), expected, "8.0.{patch}");
        }
        let mysql = has(Dialect::MySql, Version::new(8, 0, 20));
        for (inner, outer) in [
            (&["ORDER", "LIMIT"][..], &["LIMIT"][..]),
            (&["ORDER"], &["ORDER", "LIMIT"]),
            (&[], &["LIMIT"]),
        ] {
            assert!(
                shape(inner, outer).limited_when_locked(&mysql),
                "{inner:?} {outer:?}"
            );
        }
        let mariadb = has(Dialect::MariaDb, Version::new(10, 6, 3));
        assert!(sorted.limited_when_locked(&mariadb));
    }

    #[test]
    fn a_clause_inside_the_parentheses_locks_as_measured() {
        let mariadb = has(Dialect::MariaDb, Version::new(10, 6, 3));
        let old = has(Dialect::MySql, Version::new(8, 0, 18));
        let mysql = has(Dialect::MySql, Version::new(8, 0, 19));
        for (inner, outer, expected) in [
            (&["LIMIT"][..], &["LIMIT"][..], [true, false, true]),
            (&[], &["LIMIT"], [true, false, true]),
            (&["SET"], &["LIMIT"], [true, false, true]),
            (&["LIMIT"], &["ORDER", "LIMIT"], [true, true, true]),
            (&["LIMIT"], &["ORDER"], [true, true, true]),
        ] {
            let shape = shape(inner, outer);
            let found = [
                shape.locked_inside(&mariadb),
                shape.locked_inside(&old),
                shape.locked_inside(&mysql),
            ];
            assert_eq!(found, expected, "{inner:?} {outer:?}");
        }
    }
}
