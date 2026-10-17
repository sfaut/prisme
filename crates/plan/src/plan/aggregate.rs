//! Aggregation: one row per group, with aggregate functions.
//!
//! The NULL rules are those of standard SQL, measured on MySQL and MariaDB
//! (`docs/FEDERATED.md`, section 7, point 5): `GROUP BY` puts all NULLs in one
//! group, aggregate functions ignore NULLs except `COUNT(*)`. Without grouping
//! keys, the node gives one row even on an empty input, so its results can be
//! NULL even on a non-null column; with keys, an empty input gives no row.
//! `WITH ROLLUP` adds a row per level of super-aggregate, its keys NULL.

use prisme_types::{DataType, Type};

use crate::{
    AliasForm, Column, Expr, Ident, NamedExpr, Plan, Schema, SortKey, SortReference, Span,
};

/// One row per group.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    /// Input.
    pub input: Box<Plan>,
    /// Grouping keys (`GROUP BY`); empty for a single group over the whole
    /// input.
    pub keys: Vec<NamedExpr>,
    /// How each key designates its value, as written: an expression, an
    /// alias or a position of the select list.
    pub references: Vec<SortReference>,
    /// `WITH ROLLUP`: super-aggregate rows, from the last key to the first.
    pub rollup: bool,
    /// Aggregate functions, output after the keys.
    pub aggregates: Vec<NamedAggregate>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Aggregate function with the name of its output column.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedAggregate {
    /// Call.
    pub call: AggregateCall,
    /// Name of the column.
    pub name: Ident,
    /// Form of the alias.
    pub alias: AliasForm,
}

/// Call to an aggregate function.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateCall {
    /// Name of the function, as written.
    pub function: Ident,
    /// Arguments; empty for `COUNT(*)`.
    pub args: Vec<Expr>,
    /// `DISTINCT` written in the call.
    pub distinct: bool,
    /// Order of the values (`GROUP_CONCAT(a ORDER BY b)`).
    pub order_by: Vec<SortKey>,
    /// Separator of the values (`GROUP_CONCAT(a SEPARATOR ';')`), when
    /// written.
    pub separator: Option<String>,
    /// Value of a column neither grouped nor aggregated (MySQL family): any
    /// value of the group, written as the column alone (`ANY_VALUE`).
    pub implicit: bool,
    /// Data type of the result, given by the function catalog of the
    /// dialect (`SUM` of an `INT` is a `DECIMAL` in MySQL).
    pub data_type: DataType,
    /// Result over an empty or all-NULL set.
    pub on_empty: OnEmpty,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Result of an aggregate function over an empty or all-NULL set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OnEmpty {
    /// NULL: `SUM`, `MIN`, `MAX`, `AVG`, `GROUP_CONCAT`, `JSON_ARRAYAGG`...
    Null,
    /// A value, never NULL: 0 for `COUNT`.
    Value,
}

impl AggregateCall {
    /// Type of the result. A function that gives NULL over an empty set is
    /// nullable without grouping keys (the single group can be empty) or when
    /// an argument is nullable (a group can hold only NULLs).
    pub fn ty(&self, grouped: bool) -> Type {
        let nullable = match self.on_empty {
            OnEmpty::Value => false,
            OnEmpty::Null => !grouped || self.args.iter().any(|arg| arg.ty.nullable),
        };
        Type {
            data_type: self.data_type.clone(),
            nullable,
        }
    }
}

impl Aggregate {
    /// Keys, then aggregate functions. The keys of super-aggregate rows
    /// are NULL.
    pub(crate) fn schema(&self) -> Schema {
        let grouped = !self.keys.is_empty();
        let keys = self.keys.iter().map(|key| {
            let mut column = key.column();
            column.ty.nullable |= self.rollup;
            column
        });
        let aggregates = self.aggregates.iter().map(|aggregate| Column {
            qualifier: None,
            name: aggregate.name.clone(),
            ty: aggregate.call.ty(grouped),
        });
        Schema::new(keys.chain(aggregates).collect())
    }
}

#[cfg(test)]
mod tests {
    use prisme_types::{DecimalType, IntegerSize, IntegerType};

    use super::*;
    use crate::{ColumnRef, Scan, TableName};

    fn int() -> DataType {
        DataType::Integer(IntegerType::signed(IntegerSize::Int))
    }

    /// Column `t.<name>`, nullable or not.
    fn column(name: &str, nullable: bool) -> Expr {
        let column = ColumnRef {
            qualifier: Some(Ident::new("t")),
            name: Ident::new(name),
        };
        Expr::column(column, Type::nullable(int()).with_nullable(nullable))
    }

    /// `SUM(a)` over a non-null column: a `DECIMAL` in MySQL.
    fn sum() -> NamedAggregate {
        NamedAggregate {
            call: AggregateCall {
                function: Ident::new("SUM"),
                args: vec![column("a", false)],
                distinct: false,
                order_by: Vec::new(),
                separator: None,
                implicit: false,
                data_type: DataType::Decimal(DecimalType::new(32, 0).unwrap()),
                on_empty: OnEmpty::Null,
                span: None,
            },
            name: Ident::new("SUM(a)"),
            alias: AliasForm::Implicit,
        }
    }

    /// `COUNT(*)`.
    fn count() -> NamedAggregate {
        NamedAggregate {
            call: AggregateCall {
                function: Ident::new("COUNT"),
                args: Vec::new(),
                distinct: false,
                order_by: Vec::new(),
                separator: None,
                implicit: false,
                data_type: DataType::Integer(IntegerType::signed(IntegerSize::Big)),
                on_empty: OnEmpty::Value,
                span: None,
            },
            name: Ident::new("COUNT(*)"),
            alias: AliasForm::Implicit,
        }
    }

    /// Aggregate over `t (a INT NOT NULL, b INT NULL)`.
    fn aggregate(keys: Vec<NamedExpr>) -> Aggregate {
        let table = Plan::Scan(Scan::new(
            TableName {
                database: None,
                name: Ident::new("t"),
            },
            vec![
                (Ident::new("a"), Type::not_null(int())),
                (Ident::new("b"), Type::nullable(int())),
            ],
        ));
        let references = vec![SortReference::Expression; keys.len()];
        Aggregate {
            input: Box::new(table),
            keys,
            references,
            rollup: false,
            aggregates: vec![sum(), count()],
            span: None,
        }
    }

    #[test]
    fn without_keys_sum_can_be_null_and_count_cannot() {
        // Measured: on an empty input, `SUM(a)` gives NULL and `COUNT(*)` gives 0.
        let schema = aggregate(Vec::new()).schema();
        assert!(schema.columns[0].ty.nullable);
        assert!(!schema.columns[1].ty.nullable);
    }

    #[test]
    fn with_keys_sum_of_a_non_null_column_is_not_null() {
        let key = NamedExpr {
            expr: column("b", true),
            name: Ident::new("b"),
            alias: AliasForm::Implicit,
        };
        let schema = aggregate(vec![key]).schema();
        // The key keeps its nullability: the NULLs form one group.
        assert!(schema.columns[0].ty.nullable);
        assert!(!schema.columns[1].ty.nullable);
        assert!(!schema.columns[2].ty.nullable);
    }

    #[test]
    fn with_keys_sum_of_a_nullable_column_can_be_null() {
        let mut sum = sum().call;
        sum.args = vec![column("b", true)];
        assert!(sum.ty(true).nullable);
    }
}
