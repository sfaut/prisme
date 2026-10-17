//! Sort keys, used by `Sort`, by window functions and by the writes with an
//! `ORDER BY`.
//!
//! Each key carries its NULL position, always explicit
//! (`docs/FEDERATED.md`, section 7, point 6): the plan builder takes it from
//! the input dialect when the query does not write it. The reference rule of
//! Prisme, for PRQL and for the operations of Prisme, puts NULL as the
//! smallest value, as MySQL, MariaDB and SQLite do.

use crate::Expr;

/// Key of a sort.
#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    /// Value sorted on.
    pub expr: Expr,
    /// Direction.
    pub direction: Direction,
    /// Position of the NULLs.
    pub nulls: Nulls,
    /// What the query wrote.
    pub form: SortForm,
}

/// Direction of a sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Ascending (`ASC`).
    Ascending,
    /// Descending (`DESC`).
    Descending,
}

/// Position of the NULLs in a sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Nulls {
    /// Before the other values (`NULLS FIRST`).
    First,
    /// After the other values (`NULLS LAST`).
    Last,
}

/// Parts of a sort key the query wrote, for the emitter to write them again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SortForm {
    /// `ASC` or `DESC` written.
    pub direction: bool,
    /// `NULLS FIRST` or `NULLS LAST` written.
    pub nulls: bool,
}

impl Nulls {
    /// Position of the NULLs when NULL is the smallest value: first in
    /// ascending order, last in descending order. Rule of MySQL and MariaDB,
    /// and reference rule of Prisme.
    pub const fn as_smallest(direction: Direction) -> Self {
        match direction {
            Direction::Ascending => Self::First,
            Direction::Descending => Self::Last,
        }
    }
}

impl SortKey {
    /// Key whose direction and NULL position were left to the dialect, here
    /// NULL as the smallest value.
    pub fn implicit(expr: Expr, direction: Direction) -> Self {
        Self {
            expr,
            direction,
            nulls: Nulls::as_smallest(direction),
            form: SortForm::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_the_smallest_value_by_default() {
        // Measured on MySQL 8.4 and MariaDB 10.6 and 11.8.
        assert_eq!(Nulls::as_smallest(Direction::Ascending), Nulls::First);
        assert_eq!(Nulls::as_smallest(Direction::Descending), Nulls::Last);
    }
}
