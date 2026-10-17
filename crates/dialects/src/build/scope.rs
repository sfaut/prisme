//! Columns visible to the expressions of a query block, and the resolution
//! of the names written.
//!
//! Column names are compared without case, as in the MySQL family. The
//! qualifier is compared without case as well: the case of table names
//! depends on a setting of the server (`lower_case_table_names`), and the
//! name keeps the case written, which the server then judges.
//!
//! A join on columns of the same name (`USING`, `NATURAL`) merges them, as
//! MySQL and MariaDB do: the merged column answers to its name alone and
//! comes first in `*`; each side keeps its own column, reached by its
//! qualifier (`a.k`, `a.*`). The merged column is the one of the left side,
//! that of the right side for a right join, whose columns then come before
//! those of the left side in `*`.

use prisme_plan::{ColumnRef, Expr, Ident, JoinKind, Schema};
use prisme_types::Type;

use super::Output;
use crate::sql::ast::ObjectName;

/// Columns visible in a query block.
#[derive(Debug, Clone, Default)]
pub(super) struct Scope {
    /// Columns of the source, then the merged ones.
    entries: Vec<Entry>,
    /// Entries of `*`, in order.
    star: Vec<usize>,
}

/// Column visible in a scope.
#[derive(Debug, Clone)]
struct Entry {
    /// Reference to the column: qualified by its table, or its name alone
    /// for a merged column.
    reference: ColumnRef,
    /// Type.
    ty: Type,
    /// Position in the schema of the source.
    position: usize,
    /// Reachable by its name alone.
    bare: bool,
}

/// Columns a join on names matches, left then right.
pub(super) type Pairs = Vec<(Expr, Expr)>;

/// Name that designates no column, or several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolveError {
    /// No column has this name.
    Unknown,
    /// Several columns have this name.
    Ambiguous,
}

impl Scope {
    /// Columns of a source.
    pub(super) fn new(schema: Schema) -> Self {
        let entries: Vec<Entry> = schema
            .columns
            .into_iter()
            .enumerate()
            .map(|(position, column)| Entry {
                reference: ColumnRef {
                    qualifier: column.qualifier,
                    name: column.name,
                },
                ty: column.ty,
                position,
                bare: true,
            })
            .collect();
        let star = (0..entries.len()).collect();
        Self { entries, star }
    }

    /// No column: the values of a `VALUES` or of a `LIMIT`.
    pub(super) fn empty() -> Self {
        Self::default()
    }

    /// Same columns, each one nullable: the side of an outer join that may
    /// find no match.
    pub(super) fn nullable(mut self) -> Self {
        for entry in &mut self.entries {
            entry.ty.nullable = true;
        }
        self
    }

    /// Columns of a join of two sources, the left one having `left_len`
    /// columns in its schema.
    pub(super) fn concat(left: Self, right: Self, left_len: usize) -> Self {
        let offset = left.entries.len();
        let mut entries = left.entries;
        entries.extend(right.entries.into_iter().map(|entry| Entry {
            position: entry.position + left_len,
            ..entry
        }));
        let mut star = left.star;
        star.extend(right.star.into_iter().map(|index| index + offset));
        Self { entries, star }
    }

    /// Columns of a join of two sources on the columns `names` (`USING`),
    /// the left one having `left_len` columns in its schema; with the pairs
    /// of columns matched, left then right.
    ///
    /// # Errors
    ///
    /// The faulty name, when a side has no column of this name, or several.
    pub(super) fn using(
        left: Self,
        right: Self,
        left_len: usize,
        names: &[Ident],
        kind: JoinKind,
    ) -> Result<(Self, Pairs), (ResolveError, Ident)> {
        let offset = left.entries.len();
        let (left_star, right_star) = (left.star.clone(), right.star.clone());
        let mut joined = Self::concat(left, right, left_len);
        let mut left_star = left_star;
        let mut right_star: Vec<usize> =
            right_star.into_iter().map(|index| index + offset).collect();
        let mut merged = Vec::new();
        let mut pairs = Vec::new();
        for name in names {
            let find = |range: std::ops::Range<usize>| {
                let mut found = range.filter(|&index| {
                    let entry = &joined.entries[index];
                    entry.bare && same(&entry.reference.name, name)
                });
                match (found.next(), found.next()) {
                    (Some(index), None) => Ok(index),
                    (None, _) => Err((ResolveError::Unknown, name.clone())),
                    (Some(_), Some(_)) => Err((ResolveError::Ambiguous, name.clone())),
                }
            };
            let left_index = find(0..offset)?;
            let right_index = find(offset..joined.entries.len())?;
            pairs.push((
                joined.entries[left_index].as_expr(),
                joined.entries[right_index].as_expr(),
            ));
            joined.entries[left_index].bare = false;
            joined.entries[right_index].bare = false;
            left_star.retain(|&index| index != left_index);
            right_star.retain(|&index| index != right_index);
            let kept = if kind == JoinKind::Right {
                &joined.entries[right_index]
            } else {
                &joined.entries[left_index]
            };
            // An inner join keeps only the rows whose columns are equal,
            // hence not NULL.
            let nullable = kind != JoinKind::Inner && kept.ty.nullable;
            let entry = Entry {
                reference: ColumnRef {
                    qualifier: None,
                    name: kept.reference.name.clone(),
                },
                ty: kept.ty.clone().with_nullable(nullable),
                position: kept.position,
                bare: true,
            };
            merged.push(joined.entries.len());
            joined.entries.push(entry);
        }
        let (first, second) = if kind == JoinKind::Right {
            (right_star, left_star)
        } else {
            (left_star, right_star)
        };
        joined.star = merged.into_iter().chain(first).chain(second).collect();
        Ok((joined, pairs))
    }

    /// Names of the columns of `*` on both sides, for a `NATURAL` join, in
    /// the order of the left side.
    pub(super) fn common_names(left: &Self, right: &Self) -> Vec<Ident> {
        let mut names: Vec<Ident> = Vec::new();
        for &index in &left.star {
            let name = &left.entries[index].reference.name;
            let on_right = right
                .star
                .iter()
                .any(|&other| same(&right.entries[other].reference.name, name));
            if on_right && !names.iter().any(|known| same(known, name)) {
                names.push(name.clone());
            }
        }
        names
    }

    /// Column designated by `name`: `c`, `t.c` or `db.t.c`.
    pub(super) fn resolve(&self, name: &ObjectName) -> Result<(ColumnRef, Type), ResolveError> {
        let (qualifier, column) = match name.parts.as_slice() {
            [column] => (None, column),
            [.., table, column] => (Some(table), column),
            [] => return Err(ResolveError::Unknown),
        };
        let mut found = self.entries.iter().filter(|entry| {
            same(&entry.reference.name, column)
                && match qualifier {
                    None => entry.bare,
                    Some(qualifier) => entry.belongs_to(qualifier),
                }
        });
        let first = found.next().ok_or(ResolveError::Unknown)?;
        if found.next().is_some() {
            return Err(ResolveError::Ambiguous);
        }
        let reference = ColumnRef {
            qualifier: qualifier.cloned(),
            name: column.clone(),
        };
        Ok((reference, first.ty.clone()))
    }

    /// Positions in the schema of the source of the columns of `*` (all of
    /// them) or of `t.*`, with their value as a column of the select list;
    /// `None` when `t` designates no column.
    pub(super) fn wildcard(&self, qualifier: Option<&Ident>) -> Option<Vec<(usize, Output)>> {
        let indexes: Vec<usize> = match qualifier {
            None => self.star.clone(),
            Some(qualifier) => (0..self.entries.len())
                .filter(|&index| self.entries[index].belongs_to(qualifier))
                .collect(),
        };
        if qualifier.is_some() && indexes.is_empty() {
            return None;
        }
        Some(
            indexes
                .into_iter()
                .map(|index| (self.entries[index].position, self.entries[index].output()))
                .collect(),
        )
    }

    /// Columns of `*`, as outputs without alias: the keys of an `ORDER BY`
    /// after a `VALUES` or a set operation designate them.
    pub(super) fn outputs(&self) -> Vec<Output> {
        self.star
            .iter()
            .map(|&index| self.entries[index].output())
            .collect()
    }
}

impl Entry {
    /// Tells whether the column belongs to the table or alias `qualifier`.
    fn belongs_to(&self, qualifier: &Ident) -> bool {
        self.reference
            .qualifier
            .as_ref()
            .is_some_and(|own| same(own, qualifier))
    }

    /// Value of the column.
    fn as_expr(&self) -> Expr {
        Expr::column(self.reference.clone(), self.ty.clone())
    }

    /// Column as an output of the select list without alias.
    fn output(&self) -> Output {
        Output {
            alias: None,
            expr: self.as_expr(),
        }
    }
}

/// Tells whether two names are the same, without case.
fn same(left: &Ident, right: &Ident) -> bool {
    left.value.eq_ignore_ascii_case(&right.value)
}
