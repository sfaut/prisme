//! Joins of two inputs.
//!
//! A NULL key never matches with comparison equality (`docs/FEDERATED.md`,
//! section 7, point 3); a join on identity equality writes `NotDistinct` in
//! its condition. The side of an outer join that may find no match becomes
//! nullable. A semi-join or an anti-join outputs the columns of its left side
//! only, and never multiplies rows.

use crate::{Expr, Plan, Schema, Span};

/// Join of two inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    /// Left input.
    pub left: Box<Plan>,
    /// Right input.
    pub right: Box<Plan>,
    /// Kind of join.
    pub kind: JoinKind,
    /// Condition (`ON`); `None` for a cross join.
    pub condition: Option<Expr>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Kind of join. The full outer join comes with the engines that have it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinKind {
    /// Pairs of rows for which the condition is true.
    Inner,
    /// Inner join, plus the left rows without match, the right side NULL.
    Left,
    /// Inner join, plus the right rows without match, the left side NULL.
    Right,
    /// All pairs of rows.
    Cross,
    /// Left rows with at least one match (`EXISTS`, `MATCHING`).
    Semi,
    /// Left rows without match (`NOT EXISTS`, `NOT MATCHING`); a NULL key on
    /// the left is kept.
    Anti,
}

impl Join {
    /// Columns of the left side, then of the right side, made nullable on
    /// the side that may find no match.
    pub(crate) fn schema(&self) -> Schema {
        let (left, right) = (self.left.schema(), self.right.schema());
        match self.kind {
            JoinKind::Inner | JoinKind::Cross => left.concat(right),
            JoinKind::Left => left.concat(right.nullable()),
            JoinKind::Right => left.nullable().concat(right),
            JoinKind::Semi | JoinKind::Anti => left,
        }
    }
}

#[cfg(test)]
mod tests {
    use prisme_types::{DataType, IntegerSize, IntegerType, Type};

    use super::*;
    use crate::{Ident, Scan, TableName};

    /// Table `<name> (k INT NOT NULL)`.
    fn table(name: &str) -> Box<Plan> {
        let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
        Box::new(Plan::Scan(Scan {
            table: TableName {
                database: None,
                name: Ident::new(name),
            },
            alias: None,
            columns: vec![(Ident::new("k"), Type::not_null(int))],
            span: None,
        }))
    }

    /// Nullability of each output column of a join of `l` and `r`.
    fn nullability(kind: JoinKind) -> Vec<bool> {
        let join = Join {
            left: table("l"),
            right: table("r"),
            kind,
            condition: None,
            span: None,
        };
        join.schema()
            .columns
            .iter()
            .map(|column| column.ty.nullable)
            .collect()
    }

    #[test]
    fn outer_join_makes_the_other_side_nullable() {
        assert_eq!(nullability(JoinKind::Inner), [false, false]);
        assert_eq!(nullability(JoinKind::Cross), [false, false]);
        assert_eq!(nullability(JoinKind::Left), [false, true]);
        assert_eq!(nullability(JoinKind::Right), [true, false]);
    }

    #[test]
    fn semi_and_anti_joins_output_the_left_side() {
        assert_eq!(nullability(JoinKind::Semi), [false]);
        assert_eq!(nullability(JoinKind::Anti), [false]);
    }
}
