//! Nodes that bring rows: tables, `VALUES`, common table expressions and
//! subqueries of the `FROM`.
//!
//! The index hints of a table (`USE INDEX`, MySQL family) do not change the
//! rows read; its partitions (`PARTITION (p0)`) do, as a filter.

use prisme_types::{DataType, Type};

use crate::{AliasForm, Column, Expr, Ident, Plan, PlanError, Schema, Span};

/// Rows of a table.
#[derive(Debug, Clone, PartialEq)]
pub struct Scan {
    /// Table read.
    pub table: TableName,
    /// Alias of the table.
    pub alias: Option<TableAlias>,
    /// Columns of the table, in order, with their type read from the
    /// catalog of the connection.
    pub columns: Vec<(Ident, Type)>,
    /// Partitions read (`PARTITION (p0, p1)`); all of them when empty.
    pub partitions: Vec<Ident>,
    /// Index hints, in order (MySQL family).
    pub index_hints: Vec<IndexHint>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Index hint of a table (MySQL family): `USE INDEX (i)`, `FORCE KEY FOR
/// JOIN (i)`, `IGNORE INDEX FOR ORDER BY (i)`. Without effect on the rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexHint {
    /// What the optimizer is told.
    pub action: IndexHintAction,
    /// `KEY` written in place of `INDEX`.
    pub key: bool,
    /// Part of the query the hint is for; all of it when `None`.
    pub scope: Option<IndexHintScope>,
    /// Indexes; empty for `USE INDEX ()`, no index.
    pub indexes: Vec<Ident>,
}

/// What an index hint tells the optimizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexHintAction {
    /// `USE`: only these indexes.
    Use,
    /// `FORCE`: these indexes, a full scan being the last resort.
    Force,
    /// `IGNORE`: not these indexes.
    Ignore,
}

/// Part of the query an index hint is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexHintScope {
    /// `FOR JOIN`: finding rows and joins.
    Join,
    /// `FOR ORDER BY`.
    OrderBy,
    /// `FOR GROUP BY`.
    GroupBy,
}

/// Name of a table, with its database when written (`db.t`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableName {
    /// Database written before the table.
    pub database: Option<Ident>,
    /// Name of the table.
    pub name: Ident,
}

/// Alias of a table, of a subquery or of a common table expression, with its
/// column list when written (`AS t (a, b)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableAlias {
    /// Name given.
    pub name: Ident,
    /// New names of the columns, in order; empty when not written.
    pub columns: Vec<Ident>,
    /// Form written.
    pub form: AliasForm,
}

/// Rows written in the query.
#[derive(Debug, Clone, PartialEq)]
pub struct Values {
    /// Rows, all of the same length.
    pub rows: Vec<Vec<Expr>>,
    /// Columns, computed when built.
    pub schema: Schema,
    /// Form written.
    pub form: ValuesForm,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Form written for the rows of a `VALUES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValuesForm {
    /// `VALUES ROW (1), ROW (2)` (MySQL).
    Row,
    /// `VALUES (1), (2)` (MariaDB, standard SQL).
    Bare,
}

/// Rows of a common table expression, by its name.
#[derive(Debug, Clone, PartialEq)]
pub struct CteRef {
    /// Name of the common table expression.
    pub name: Ident,
    /// Alias given where it is read.
    pub alias: Option<TableAlias>,
    /// Columns, qualified by the alias or the name, computed when built.
    pub schema: Schema,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Common table expressions, then the query that uses them.
#[derive(Debug, Clone, PartialEq)]
pub struct With {
    /// `WITH RECURSIVE`.
    pub recursive: bool,
    /// Common table expressions, in order.
    pub ctes: Vec<Cte>,
    /// Query that uses them.
    pub body: Box<Plan>,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Common table expression (`name (columns) AS (query)`).
#[derive(Debug, Clone, PartialEq)]
pub struct Cte {
    /// Name.
    pub name: Ident,
    /// Column list written after the name; empty when not written.
    pub columns: Vec<Ident>,
    /// Query of the expression; a recursive one reads itself by `CteRef`.
    pub plan: Plan,
    /// Columns, renamed by the column list, computed when built.
    pub schema: Schema,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Subquery of the `FROM`, with its alias.
#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    /// Subquery.
    pub input: Box<Plan>,
    /// Alias, required in the `FROM`.
    pub alias: TableAlias,
    /// `LATERAL`: the subquery reads the columns of the tables before it in
    /// the `FROM`.
    pub lateral: bool,
    /// The subquery reads columns of an enclosing query (outer references),
    /// which some dialects and versions refuse in a derived table.
    pub outer_references: bool,
    /// Columns, qualified and renamed by the alias, computed when built.
    pub schema: Schema,
    /// Position in the input.
    pub span: Option<Span>,
}

impl Scan {
    /// Rows of `table`, of the given columns, without alias, partition or
    /// hint.
    pub fn new(table: TableName, columns: Vec<(Ident, Type)>) -> Self {
        Self {
            table,
            alias: None,
            columns,
            partitions: Vec::new(),
            index_hints: Vec::new(),
            span: None,
        }
    }

    /// Columns of the table, qualified by the alias or by the name of the
    /// table.
    pub(crate) fn schema(&self) -> Schema {
        let qualifier = self
            .alias
            .as_ref()
            .map_or(&self.table.name, |alias| &alias.name);
        let columns = self
            .columns
            .iter()
            .map(|(name, ty)| Column {
                qualifier: Some(qualifier.clone()),
                name: name.clone(),
                ty: ty.clone(),
            })
            .collect();
        Schema::new(columns)
    }
}

impl Values {
    /// `VALUES` of the given rows. The names and the data types of the
    /// columns come from the dialect; a column is nullable when one of its
    /// values is.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when a row or the data types differ in
    /// length from the names.
    pub fn new(
        rows: Vec<Vec<Expr>>,
        names: Vec<Ident>,
        data_types: Vec<DataType>,
        form: ValuesForm,
    ) -> Result<Self, PlanError> {
        let mut lengths = std::iter::once(data_types.len()).chain(rows.iter().map(Vec::len));
        if let Some(found) = lengths.find(|&length| length != names.len()) {
            return Err(PlanError::ColumnCount {
                expected: names.len(),
                found,
            });
        }
        let columns = names
            .into_iter()
            .zip(data_types)
            .enumerate()
            .map(|(position, (name, data_type))| Column {
                qualifier: None,
                name,
                ty: Type {
                    data_type,
                    nullable: rows.iter().any(|row| row[position].ty.nullable),
                },
            })
            .collect();
        Ok(Self {
            rows,
            schema: Schema::new(columns),
            form,
            span: None,
        })
    }
}

impl Cte {
    /// Common table expression named `name`, its columns renamed by
    /// `columns` when written.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the column list differs in length from
    /// the columns of the query.
    pub fn new(name: Ident, columns: Vec<Ident>, plan: Plan) -> Result<Self, PlanError> {
        let schema = unqualified(plan.schema()).renamed(&columns)?;
        Ok(Self {
            name,
            columns,
            plan,
            schema,
            span: None,
        })
    }
}

impl CteRef {
    /// Reading of the common table expression `cte`, under `alias` when
    /// given.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the column list of the alias differs in
    /// length from the columns of the expression.
    pub fn new(cte: &Cte, alias: Option<TableAlias>) -> Result<Self, PlanError> {
        let schema = aliased(cte.schema.clone(), &cte.name, alias.as_ref())?;
        Ok(Self {
            name: cte.name.clone(),
            alias,
            schema,
            span: None,
        })
    }
}

impl Derived {
    /// Subquery `input` of the `FROM`, under `alias`.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when the column list of the alias differs in
    /// length from the columns of the subquery.
    pub fn new(input: Plan, alias: TableAlias) -> Result<Self, PlanError> {
        let schema = aliased(input.schema(), &alias.name, Some(&alias))?;
        Ok(Self {
            input: Box::new(input),
            alias,
            lateral: false,
            outer_references: false,
            schema,
            span: None,
        })
    }
}

/// Columns of `schema` qualified by the alias, or by `name` without alias,
/// and renamed by the column list of the alias.
fn aliased(schema: Schema, name: &Ident, alias: Option<&TableAlias>) -> Result<Schema, PlanError> {
    let qualifier = alias.map_or(name, |alias| &alias.name);
    let names = alias.map_or(&[][..], |alias| &alias.columns);
    Ok(schema.renamed(names)?.qualified(qualifier))
}

/// Same columns, without qualifier: the columns of a query seen from
/// outside.
fn unqualified(schema: Schema) -> Schema {
    let columns = schema
        .columns
        .into_iter()
        .map(|column| Column {
            qualifier: None,
            ..column
        })
        .collect();
    Schema::new(columns)
}

#[cfg(test)]
mod tests {
    use prisme_types::{IntegerSize, IntegerType, Value};

    use super::*;

    fn int() -> DataType {
        DataType::Integer(IntegerType::signed(IntegerSize::Int))
    }

    fn values(rows: Vec<Vec<Expr>>, names: &[&str]) -> Result<Values, PlanError> {
        let names: Vec<Ident> = names.iter().map(|name| Ident::new(*name)).collect();
        let data_types = vec![int(); names.len()];
        Values::new(rows, names, data_types, ValuesForm::Bare)
    }

    fn integer(value: i128) -> Expr {
        Expr::literal(Value::Integer(value), int())
    }

    #[test]
    fn values_column_is_nullable_when_a_value_is() {
        let null = Expr::literal(Value::Null, DataType::Null);
        let values = values(
            vec![vec![integer(1), integer(2)], vec![null, integer(3)]],
            &["a", "b"],
        );
        let schema = values.unwrap().schema;
        assert!(schema.columns[0].ty.nullable);
        assert!(!schema.columns[1].ty.nullable);
        assert_eq!(schema.columns[0].ty.data_type, int());
    }

    #[test]
    fn values_rows_have_the_length_of_the_names() {
        let short = values(
            vec![vec![integer(1), integer(2)], vec![integer(3)]],
            &["a", "b"],
        );
        assert_eq!(
            short,
            Err(PlanError::ColumnCount {
                expected: 2,
                found: 1
            })
        );
    }

    #[test]
    fn scan_qualifies_by_alias_or_table() {
        let mut scan = Scan::new(
            TableName {
                database: None,
                name: Ident::new("t"),
            },
            vec![(Ident::new("a"), Type::not_null(int()))],
        );
        assert_eq!(scan.schema().columns[0].qualifier, Some(Ident::new("t")));
        scan.alias = Some(TableAlias {
            name: Ident::new("x"),
            columns: Vec::new(),
            form: AliasForm::As,
        });
        assert_eq!(scan.schema().columns[0].qualifier, Some(Ident::new("x")));
    }

    #[test]
    fn derived_renames_by_its_column_list() {
        let input = Plan::Values(values(vec![vec![integer(1)]], &["column_0"]).unwrap());
        let alias = |columns: &[&str]| TableAlias {
            name: Ident::new("d"),
            columns: columns.iter().map(|name| Ident::new(*name)).collect(),
            form: AliasForm::As,
        };
        let derived = Derived::new(input.clone(), alias(&["c"])).unwrap();
        let column = &derived.schema.columns[0];
        assert_eq!(
            (column.qualifier.clone(), column.name.clone()),
            (Some(Ident::new("d")), Ident::new("c"))
        );
        assert!(Derived::new(input, alias(&["c", "e"])).is_err());
    }
}
