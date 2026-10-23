//! Temporary tables of a session's connection, followed query after query.
//!
//! A change of tables (`ALTER`, `RENAME`, `TRUNCATE`, `DROP`) needs the
//! switches of the temporary tables when all the tables it names are
//! temporary tables of the connection
//! (`prisme_dialects::classify::classify_with`). MySQL (checked on 8.4.11)
//! and MariaDB before 11.2 (checked on 11.1.5) list no temporary table, in
//! `information_schema.TABLES` or `SHOW TABLES`; MariaDB lists them from 11.2
//! (checked on 11.2.4 and 11.4.2, `TABLE_TYPE` `TEMPORARY`). The session
//! therefore follows, on every server, the tables its own queries create,
//! drop and rename
//! (`prisme_dialects::temporary`). A table is added only once its creation
//! succeeded; it leaves as soon as a query may have dropped or renamed it,
//! even a failed one. When in doubt, a table is taken for a permanent table,
//! the stricter reading. A new session starts on a new connection, without
//! temporary tables.

use std::collections::BTreeSet;

use prisme_dialects::temporary::{TableName, TemporaryChange};

/// Temporary tables of a connection, by database and name.
#[derive(Debug, Default)]
pub(super) struct TemporaryTables {
    /// Database and name of each table.
    tables: BTreeSet<(String, String)>,
}

impl TemporaryTables {
    /// Is `table`, named in a query run on the current database `database`,
    /// a temporary table?
    pub(super) fn contains(&self, table: &TableName, database: Option<&str>) -> bool {
        key(table, database).is_some_and(|key| self.tables.contains(&key))
    }

    /// Follows the changes a query brought to the temporary tables, run on
    /// the current database `database`; `succeeded` tells whether the query
    /// succeeded.
    pub(super) fn follow(
        &mut self,
        changes: &[TemporaryChange],
        database: Option<&str>,
        succeeded: bool,
    ) {
        for change in changes {
            match change {
                TemporaryChange::Created(table) => {
                    if let Some(key) = key(table, database).filter(|_| succeeded) {
                        self.tables.insert(key);
                    }
                }
                TemporaryChange::Dropped(table) => {
                    if let Some(key) = key(table, database) {
                        self.tables.remove(&key);
                    }
                }
                TemporaryChange::Renamed { from, to } => {
                    let renamed = key(from, database).is_some_and(|key| self.tables.remove(&key));
                    if let Some(key) = key(to, database).filter(|_| renamed && succeeded) {
                        self.tables.insert(key);
                    }
                }
            }
        }
    }
}

/// Database and name of a table, `None` for a name without a database when
/// the connection has no current database.
fn key(table: &TableName, database: Option<&str>) -> Option<(String, String)> {
    let schema = table.schema.as_deref().or(database)?;
    Some((schema.to_string(), table.name.clone()))
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
    fn follows_the_tables_created_dropped_and_renamed() {
        use TemporaryChange::*;
        let mut tables = TemporaryTables::default();
        let shop = Some("shop");

        // Created on the current database, seen with or without the database.
        tables.follow(&[Created(table(None, "tmp"))], shop, true);
        assert!(tables.contains(&table(None, "tmp"), shop));
        assert!(tables.contains(&table(Some("shop"), "tmp"), None));
        assert!(!tables.contains(&table(None, "tmp"), Some("other")));
        assert!(!tables.contains(&table(None, "tmp"), None));

        // A failed creation adds nothing.
        tables.follow(&[Created(table(None, "t2"))], shop, false);
        assert!(!tables.contains(&table(None, "t2"), shop));

        // Renamed once the query succeeded; a failed renaming forgets it.
        let renamed = Renamed {
            from: table(None, "tmp"),
            to: table(None, "tmp2"),
        };
        tables.follow(std::slice::from_ref(&renamed), shop, true);
        assert!(!tables.contains(&table(None, "tmp"), shop));
        assert!(tables.contains(&table(None, "tmp2"), shop));
        let failed = Renamed {
            from: table(None, "tmp2"),
            to: table(None, "tmp3"),
        };
        tables.follow(&[failed], shop, false);
        assert!(!tables.contains(&table(None, "tmp2"), shop));
        assert!(!tables.contains(&table(None, "tmp3"), shop));

        // A permanent table renamed does not become temporary.
        tables.follow(std::slice::from_ref(&renamed), shop, true);
        assert!(!tables.contains(&table(None, "tmp2"), shop));

        // Dropped, even by a failed query.
        tables.follow(&[Created(table(None, "tmp"))], shop, true);
        tables.follow(&[Dropped(table(Some("shop"), "tmp"))], None, false);
        assert!(!tables.contains(&table(None, "tmp"), shop));
    }
}
