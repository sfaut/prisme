//! Tables that a query writes, and the temporary tables it creates, drops or
//! renames.
//!
//! Serves the write switches of the temporary tables (see `docs/V1.md`,
//! section 2.4). Only `CREATE TEMPORARY TABLE` and `DROP TEMPORARY TABLE` say
//! by their syntax that they target a temporary table: for the other writes
//! (`INSERT`, `TRUNCATE`...), the session follows the temporary tables created
//! on its connection (`temporary_changes`), and a write counts as one on
//! temporary tables when all the tables it names are among them.
//!
//! Names are compared as written, without folding their case: a name written
//! in another case than at the creation of the table is taken for another
//! table, the stricter reading.

use sqlparser::ast::{
    AlterTableOperation, ObjectName, ObjectNamePart, ObjectType, RenameTableNameKind, Statement,
    TableFactor, TableWithJoins,
};

use crate::Dialect;
use crate::mysql_statements::MySqlStatement;
use crate::parse::{Parsed, parse_statements};

/// Name of a table in a query, unquoted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableName {
    /// Database written before the name (`shop.orders`), `None` if absent:
    /// the table is then in the current database of the connection.
    pub schema: Option<String>,
    /// Name of the table.
    pub name: String,
}

/// Change that a query brings to the temporary tables of its connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporaryChange {
    /// `CREATE TEMPORARY TABLE`: the table exists once the query succeeded.
    Created(TableName),
    /// `DROP [TEMPORARY] TABLE`, or a change Prisme cannot follow: the table
    /// may no longer be a temporary table, even if the query failed.
    Dropped(TableName),
    /// `RENAME TABLE`, `ALTER TABLE ... RENAME`: once the query succeeded, a
    /// temporary table `from` is called `to`.
    Renamed {
        /// Name before the query.
        from: TableName,
        /// Name after the query.
        to: TableName,
    },
}

/// Changes that the text `sql` brings to the temporary tables of its
/// connection, in order; none if it cannot be parsed.
pub fn temporary_changes(sql: &str, dialect: Dialect) -> Vec<TemporaryChange> {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return Vec::new();
    };
    statements.iter().flat_map(changes_of).collect()
}

/// Changes that a parsed statement brings to the temporary tables.
fn changes_of(statement: &Parsed) -> Vec<TemporaryChange> {
    match statement {
        Parsed::Sql(Statement::CreateTable(create)) if create.temporary => table_name(&create.name)
            .map(TemporaryChange::Created)
            .into_iter()
            .collect(),
        Parsed::Sql(Statement::Drop {
            object_type: ObjectType::Table,
            names,
            ..
        }) => names
            .iter()
            .filter_map(table_name)
            .map(TemporaryChange::Dropped)
            .collect(),
        Parsed::Sql(Statement::RenameTable(renames)) => renames
            .iter()
            .filter_map(|rename| renamed(&rename.old_name, &rename.new_name))
            .collect(),
        Parsed::Sql(Statement::AlterTable(alter)) => alter
            .operations
            .iter()
            .filter_map(|operation| match operation {
                AlterTableOperation::RenameTable {
                    table_name: RenameTableNameKind::As(to) | RenameTableNameKind::To(to),
                } => renamed(&alter.name, to),
                _ => None,
            })
            .collect(),
        // `ALTER TABLE` that `sqlparser` does not parse: it may rename the
        // table, so the table is no longer taken for a temporary table.
        Parsed::MySql(MySqlStatement::AlterTable { name }) => table_name(name)
            .map(TemporaryChange::Dropped)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// Renaming of a table, `None` if one of the names is not a table name.
fn renamed(from: &ObjectName, to: &ObjectName) -> Option<TemporaryChange> {
    Some(TemporaryChange::Renamed {
        from: table_name(from)?,
        to: table_name(to)?,
    })
}

/// Name of a table, `None` for another form (more than a database and a
/// name, name computed by a function).
pub(crate) fn table_name(name: &ObjectName) -> Option<TableName> {
    let parts: Vec<&str> = name
        .0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident.value.as_str()),
            ObjectNamePart::Function(_) => None,
        })
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [name] => Some(TableName {
            schema: None,
            name: (*name).to_string(),
        }),
        [schema, name] => Some(TableName {
            schema: Some((*schema).to_string()),
            name: (*name).to_string(),
        }),
        _ => None,
    }
}

/// Names of the tables of `FROM`-like clauses, joins included; derived
/// tables and the other forms, which cannot be written, are left out.
pub(crate) fn relation_names(tables: &[TableWithJoins]) -> Vec<&ObjectName> {
    let mut names = Vec::new();
    for table in tables {
        for factor in
            std::iter::once(&table.relation).chain(table.joins.iter().map(|join| &join.relation))
        {
            match factor {
                TableFactor::Table { name, .. } => names.push(name),
                TableFactor::NestedJoin {
                    table_with_joins, ..
                } => names.extend(relation_names(std::slice::from_ref(table_with_joins))),
                _ => {}
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(schema: Option<&str>, name: &str) -> TableName {
        TableName {
            schema: schema.map(str::to_string),
            name: name.to_string(),
        }
    }

    #[test]
    fn follows_the_temporary_tables_created_dropped_and_renamed() {
        use TemporaryChange::*;
        let changes = |sql| temporary_changes(sql, Dialect::MySql);
        assert_eq!(
            changes("CREATE TEMPORARY TABLE tmp (a INT)"),
            [Created(table(None, "tmp"))]
        );
        assert_eq!(
            changes("CREATE TEMPORARY TABLE IF NOT EXISTS `shop`.`tmp` LIKE shop.orders"),
            [Created(table(Some("shop"), "tmp"))]
        );
        assert!(changes("CREATE TABLE t (a INT)").is_empty());
        assert_eq!(
            changes("DROP TEMPORARY TABLE IF EXISTS tmp, shop.t2"),
            [
                Dropped(table(None, "tmp")),
                Dropped(table(Some("shop"), "t2"))
            ]
        );
        assert_eq!(changes("DROP TABLE tmp"), [Dropped(table(None, "tmp"))]);
        assert_eq!(
            changes("RENAME TABLE a TO b, b TO c"),
            [
                Renamed {
                    from: table(None, "a"),
                    to: table(None, "b")
                },
                Renamed {
                    from: table(None, "b"),
                    to: table(None, "c")
                }
            ]
        );
        assert_eq!(
            changes("ALTER TABLE tmp ADD b INT, RENAME TO tmp2"),
            [Renamed {
                from: table(None, "tmp"),
                to: table(None, "tmp2")
            }]
        );
        assert!(changes("ALTER TABLE tmp ADD b INT").is_empty());
        // ALTER TABLE that `sqlparser` does not parse: the table is forgotten.
        assert_eq!(
            changes("ALTER TABLE tmp ALTER INDEX i INVISIBLE"),
            [Dropped(table(None, "tmp"))]
        );
        assert!(changes("DROP VIEW v").is_empty());
        assert!(changes("SELECT * FROM").is_empty());
    }
}
