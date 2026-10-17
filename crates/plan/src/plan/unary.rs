//! Nodes with a single input: filter, projections, window functions, sort,
//! limit and duplicate removal.

use crate::{
    AliasForm, Column, Expr, ExprKind, Frame, Ident, Plan, PlanError, Schema, SortKey, Span,
};

/// Rows for which a predicate is true; a false or unknown predicate drops the
/// row (`docs/FEDERATED.md`, section 7, point 2).
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// Input.
    pub input: Box<Plan>,
    /// Predicate, of type `Boolean`.
    pub predicate: Expr,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Columns computed from each row: the `SELECT` list.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    /// Input.
    pub input: Box<Plan>,
    /// Items of the list, in order.
    pub items: Vec<ProjectItem>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Item of a `SELECT` list.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectItem {
    /// Named expression.
    Expr(NamedExpr),
    /// `*` or `t.*`, kept as written.
    Wildcard(Wildcard),
}

/// `*` or `t.*`, with the columns of the input it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wildcard {
    /// Table written before the star (`t.*`).
    pub qualifier: Option<Ident>,
    /// Positions of the columns in the schema of the input, in order.
    pub columns: Vec<usize>,
}

/// Expression with the name of its output column.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedExpr {
    /// Expression.
    pub expr: Expr,
    /// Name of the column: the alias, or the name the dialect gives without
    /// alias.
    pub name: Ident,
    /// Form of the alias.
    pub alias: AliasForm,
}

/// Columns of the input, plus computed ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Extend {
    /// Input.
    pub input: Box<Plan>,
    /// Computed columns, added after those of the input.
    pub columns: Vec<NamedExpr>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Columns of the input, plus window functions.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// Input.
    pub input: Box<Plan>,
    /// Window functions (`ExprKind::Window`), added after the columns of the
    /// input.
    pub functions: Vec<NamedExpr>,
    /// Named windows (`WINDOW w AS (...)`), as written: the calls carry the
    /// windows they use whole.
    pub definitions: Vec<WindowDefinition>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Named window, as written: `w AS (base PARTITION BY a ORDER BY b)`.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowDefinition {
    /// Name.
    pub name: Ident,
    /// Named window it completes.
    pub base: Option<Ident>,
    /// `PARTITION BY` written in this definition.
    pub partition_by: Vec<Expr>,
    /// `ORDER BY` written in this definition.
    pub order_by: Vec<SortKey>,
    /// Frame written in this definition.
    pub frame: Option<Frame>,
}

/// Rows in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Sort {
    /// Input.
    pub input: Box<Plan>,
    /// Keys, in order of priority.
    pub keys: Vec<SortKey>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Part of the rows.
#[derive(Debug, Clone, PartialEq)]
pub struct Limit {
    /// Input.
    pub input: Box<Plan>,
    /// Number of rows kept; `None` for all of them.
    pub count: Option<Expr>,
    /// Number of rows skipped first; `None` for none.
    pub offset: Option<Expr>,
    /// Form written.
    pub form: LimitForm,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Form written for a limit with an offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitForm {
    /// `LIMIT count OFFSET offset`, or `LIMIT count` alone.
    Offset,
    /// `LIMIT offset, count` (MySQL family).
    Comma,
}

/// Rows without duplicates, NULLs being identical (`docs/FEDERATED.md`,
/// section 7, point 4).
#[derive(Debug, Clone, PartialEq)]
pub struct Distinct {
    /// Input.
    pub input: Box<Plan>,
    /// Position in the input.
    pub span: Option<Span>,
}

impl Project {
    /// `SELECT` list of `items` over `input`.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnPosition` when a wildcard stands for a column
    /// outside the input.
    pub fn new(input: Plan, items: Vec<ProjectItem>) -> Result<Self, PlanError> {
        let count = input.schema().len();
        let mut positions = items.iter().flat_map(|item| match item {
            ProjectItem::Wildcard(wildcard) => wildcard.columns.as_slice(),
            ProjectItem::Expr(_) => &[],
        });
        if let Some(&position) = positions.find(|&&position| position >= count) {
            return Err(PlanError::ColumnPosition { position, count });
        }
        Ok(Self {
            input: Box::new(input),
            items,
            span: None,
        })
    }

    /// Columns of the list, a wildcard standing for its columns.
    pub(crate) fn schema(&self) -> Schema {
        let input = self.input.schema();
        let columns = self
            .items
            .iter()
            .flat_map(|item| match item {
                ProjectItem::Expr(named) => vec![named.column()],
                ProjectItem::Wildcard(wildcard) => wildcard
                    .columns
                    .iter()
                    .map(|&position| input.columns[position].clone())
                    .collect(),
            })
            .collect();
        Schema::new(columns)
    }
}

impl NamedExpr {
    /// Output column. A column referenced without alias keeps its qualifier,
    /// so that `ORDER BY t.a` still finds it.
    pub fn column(&self) -> Column {
        let qualifier = match (&self.expr.kind, self.alias) {
            (ExprKind::Column(column), AliasForm::Implicit) => column.qualifier.clone(),
            _ => None,
        };
        Column {
            qualifier,
            name: self.name.clone(),
            ty: self.expr.ty.clone(),
        }
    }
}

impl Extend {
    /// Columns of the input, then the computed ones.
    pub(crate) fn schema(&self) -> Schema {
        with_columns(&self.input, &self.columns)
    }
}

impl Window {
    /// Columns of the input, then the window functions.
    pub(crate) fn schema(&self) -> Schema {
        with_columns(&self.input, &self.functions)
    }
}

/// Columns of `input`, then those of `added`.
fn with_columns(input: &Plan, added: &[NamedExpr]) -> Schema {
    let mut schema = input.schema();
    schema.columns.extend(added.iter().map(NamedExpr::column));
    schema
}

#[cfg(test)]
mod tests {
    use prisme_types::{DataType, IntegerSize, IntegerType, Type};

    use super::*;
    use crate::{ColumnRef, Scan, TableName};

    fn int() -> DataType {
        DataType::Integer(IntegerType::signed(IntegerSize::Int))
    }

    /// Table `t (a INT NOT NULL, b INT NULL)`.
    fn table() -> Plan {
        Plan::Scan(Scan::new(
            TableName {
                database: None,
                name: Ident::new("t"),
            },
            vec![
                (Ident::new("a"), Type::not_null(int())),
                (Ident::new("b"), Type::nullable(int())),
            ],
        ))
    }

    fn column_a() -> Expr {
        let column = ColumnRef {
            qualifier: Some(Ident::new("t")),
            name: Ident::new("a"),
        };
        Expr::column(column, Type::not_null(int()))
    }

    #[test]
    fn project_expands_its_wildcard_and_names_its_expressions() {
        let items = vec![
            ProjectItem::Wildcard(Wildcard {
                qualifier: None,
                columns: vec![1, 0],
            }),
            ProjectItem::Expr(NamedExpr {
                expr: column_a(),
                name: Ident::new("x"),
                alias: AliasForm::As,
            }),
        ];
        let schema = Project::new(table(), items).unwrap().schema();
        let names: Vec<&str> = schema
            .columns
            .iter()
            .map(|column| column.name.value.as_str())
            .collect();
        assert_eq!(names, ["b", "a", "x"]);
        assert!(schema.columns[0].ty.nullable);
        // An aliased column leaves its table.
        assert_eq!(schema.columns[2].qualifier, None);
    }

    #[test]
    fn project_refuses_a_wildcard_outside_its_input() {
        let items = vec![ProjectItem::Wildcard(Wildcard {
            qualifier: None,
            columns: vec![2],
        })];
        assert_eq!(
            Project::new(table(), items),
            Err(PlanError::ColumnPosition {
                position: 2,
                count: 2
            })
        );
    }

    #[test]
    fn column_without_alias_keeps_its_qualifier() {
        let named = NamedExpr {
            expr: column_a(),
            name: Ident::new("a"),
            alias: AliasForm::Implicit,
        };
        assert_eq!(named.column().qualifier, Some(Ident::new("t")));
    }

    #[test]
    fn extend_adds_after_the_input() {
        let extend = Extend {
            input: Box::new(table()),
            columns: vec![NamedExpr {
                expr: column_a(),
                name: Ident::new("c"),
                alias: AliasForm::As,
            }],
            span: None,
        };
        assert_eq!(extend.schema().len(), 3);
    }
}
