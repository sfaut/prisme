//! Set operations: union, difference and intersection of two inputs.
//!
//! Rows are compared whole, NULLs being identical (`NotDistinct`), as MySQL
//! and MariaDB do (`docs/FEDERATED.md`, section 7, point 4). The duplicate
//! mode is in the name of the operator, without a flag to interpret
//! (section 5.1).

use prisme_types::{DataType, Type};

use crate::{Column, Plan, PlanError, Schema, Span};

/// Union, difference or intersection of two inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct SetOperation {
    /// Operator.
    pub operator: SetOperator,
    /// Left input, which names the columns.
    pub left: Box<Plan>,
    /// Right input.
    pub right: Box<Plan>,
    /// Columns, computed when built.
    pub schema: Schema,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Set operator, with its duplicate mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SetOperator {
    /// `UNION ALL`: duplicates kept, occurrences added.
    UnionAll,
    /// `UNION`: duplicates removed.
    UnionDistinct,
    /// `EXCEPT ALL`: max(0, n − m) occurrences.
    MinusAll,
    /// `EXCEPT`: duplicates removed.
    MinusDistinct,
    /// `INTERSECT ALL`: min(n, m) occurrences.
    IntersectAll,
    /// `INTERSECT`: duplicates removed.
    IntersectDistinct,
}

impl SetOperation {
    /// Set operation of `left` and `right`. The names of the columns come from
    /// the left side, their data types from the dialect; a column is
    /// nullable when it is on one side.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the right side or the data types differ
    /// in length from the left side.
    pub fn new(
        operator: SetOperator,
        left: Plan,
        right: Plan,
        data_types: Vec<DataType>,
    ) -> Result<Self, PlanError> {
        let (left_schema, right_schema) = (left.schema(), right.schema());
        let expected = left_schema.len();
        for found in [right_schema.len(), data_types.len()] {
            if found != expected {
                return Err(PlanError::ColumnCount { expected, found });
            }
        }
        let columns = left_schema
            .columns
            .into_iter()
            .zip(right_schema.columns)
            .zip(data_types)
            .map(|((left, right), data_type)| Column {
                qualifier: None,
                name: left.name,
                ty: Type {
                    data_type,
                    nullable: left.ty.nullable || right.ty.nullable,
                },
            })
            .collect();
        Ok(Self {
            operator,
            left: Box::new(left),
            right: Box::new(right),
            schema: Schema::new(columns),
            span: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use prisme_types::{IntegerSize, IntegerType, Value};

    use super::*;
    use crate::{Expr, Ident, Values, ValuesForm};

    fn int() -> DataType {
        DataType::Integer(IntegerType::signed(IntegerSize::Int))
    }

    /// `VALUES (value)` with one column named `name`.
    fn values(name: &str, value: Value) -> Plan {
        let data_type = if value == Value::Null {
            DataType::Null
        } else {
            int()
        };
        let row = vec![Expr::literal(value, data_type)];
        Plan::Values(
            Values::new(
                vec![row],
                vec![Ident::new(name)],
                vec![int()],
                ValuesForm::Bare,
            )
            .unwrap(),
        )
    }

    #[test]
    fn left_names_and_either_side_nullable() {
        let union = SetOperation::new(
            SetOperator::UnionDistinct,
            values("a", Value::Integer(1)),
            values("b", Value::Null),
            vec![int()],
        )
        .unwrap();
        let column = &union.schema.columns[0];
        assert_eq!(column.name, Ident::new("a"));
        assert_eq!(column.ty, Type::nullable(int()));
    }

    #[test]
    fn sides_have_the_same_number_of_columns() {
        let two = Plan::Values(
            Values::new(
                vec![vec![
                    Expr::literal(Value::Integer(1), int()),
                    Expr::literal(Value::Integer(2), int()),
                ]],
                vec![Ident::new("a"), Ident::new("b")],
                vec![int(), int()],
                ValuesForm::Bare,
            )
            .unwrap(),
        );
        let result = SetOperation::new(
            SetOperator::MinusAll,
            values("a", Value::Integer(1)),
            two,
            vec![int()],
        );
        assert_eq!(
            result,
            Err(PlanError::ColumnCount {
                expected: 1,
                found: 2
            })
        );
    }
}
