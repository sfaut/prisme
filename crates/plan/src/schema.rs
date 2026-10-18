//! Output schema of a node: its columns, in order, with their type.

use prisme_types::Type;

use crate::{Ident, PlanError};

/// Column output by a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Table, alias or common table expression the column belongs to;
    /// `None` for a computed column.
    pub qualifier: Option<Ident>,
    /// Name of the column.
    pub name: Ident,
    /// Type of the column, with its nullability.
    pub ty: Type,
}

/// Columns output by a node, in order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Schema {
    /// Columns, in order.
    pub columns: Vec<Column>,
}

impl Schema {
    /// Schema of the given columns.
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    /// Number of columns.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// `true` without any column.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Columns of `self`, then those of `other`.
    pub fn concat(mut self, other: Schema) -> Schema {
        self.columns.extend(other.columns);
        self
    }

    /// Same columns, all nullable: the side of an outer join that may find
    /// no match.
    pub fn nullable(self) -> Schema {
        self.map(|column| Column {
            ty: column.ty.with_nullable(true),
            ..column
        })
    }

    /// Same columns, of the types of their values once stored
    /// (`DataType::stored`): the columns of a derived table or of a common
    /// table expression.
    pub fn stored(self) -> Schema {
        self.map(|column| Column {
            ty: Type {
                data_type: column.ty.data_type.stored(),
                ..column.ty
            },
            ..column
        })
    }

    /// Same columns, all belonging to `qualifier`.
    pub fn qualified(self, qualifier: &Ident) -> Schema {
        self.map(|column| Column {
            qualifier: Some(qualifier.clone()),
            ..column
        })
    }

    /// Same columns renamed, in order, by `names`; unchanged when `names` is
    /// empty.
    ///
    /// # Errors
    ///
    /// `PlanError::ColumnCount` when `names` is neither empty nor as long as
    /// the schema.
    pub fn renamed(self, names: &[Ident]) -> Result<Schema, PlanError> {
        if names.is_empty() {
            return Ok(self);
        }
        if names.len() != self.len() {
            return Err(PlanError::ColumnCount {
                expected: self.len(),
                found: names.len(),
            });
        }
        let columns = self
            .columns
            .into_iter()
            .zip(names)
            .map(|(column, name)| Column {
                name: name.clone(),
                ..column
            })
            .collect();
        Ok(Schema { columns })
    }

    /// Schema whose columns are those of `self`, transformed one by one.
    fn map(self, transform: impl Fn(Column) -> Column) -> Schema {
        Schema {
            columns: self.columns.into_iter().map(transform).collect(),
        }
    }
}
