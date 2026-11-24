//! Classification of a query: read or write, from its AST.
//!
//! Serves the write switches (see `docs/V1.md`, section 2.4). Classification
//! works on the AST, never on the text: `WITH ... DELETE` or
//! `SELECT ... INTO` are writes, even though they start like a read. An allow
//! list decides what a read is: everything else is a write.
//!
//! A write has the kinds of the switches it needs (`WriteKind`): `REPLACE`
//! inserts and deletes, `INSERT ... ON DUPLICATE KEY UPDATE` inserts and
//! updates, `CREATE INDEX` / `DROP INDEX` change the table, `ALTER VIEW`
//! creates the view again. A write that no switch covers (`CALL`, database,
//! trigger, account, file, server setting...) is forbidden.
//!
//! The structure of the temporary tables has its own switches, their data
//! stays under the switches of the data. `CREATE TEMPORARY TABLE` and
//! `DROP TEMPORARY TABLE` need them by their syntax; `ALTER`, `RENAME` and
//! `DROP TABLE` need them when all the tables they name are temporary tables
//! known to the session (`classify_with`, `temporary.rs`). `TRUNCATE TABLE`
//! empties the data: it has its switch beside `DELETE`, for every table.
//!
//! A setting of the session (`SET @v = 1`, `SET SESSION sql_mode = ...`,
//! `SET NAMES`) writes nothing in the database: it is always allowed.
//!
//! SQLite follows the same switches (`docs/TODO-SQLITE.md`, decisions 2,
//! 3, 10 and 11), with its own forms: `INSERT OR REPLACE` and `REPLACE`
//! need `INSERT` and `DELETE`, `UPDATE OR REPLACE` needs `UPDATE` and
//! `DELETE`, `ON CONFLICT DO UPDATE` needs `INSERT` and `UPDATE`; a table of
//! the schema `temp` is a temporary table. A pragma has the role its list
//! gives it (`sqlite_pragmas.rs`): it reads, sets the connection (a setting
//! of the session), writes the file (the "Maintenance" switch, as `VACUUM`,
//! `REINDEX`, `ANALYZE` and the function `pragma_optimize`), or is refused.
//! `ATTACH` and `DETACH` have their switch; an `ATTACH` must name its file
//! by a literal absolute path, or `':memory:'`. `VACUUM INTO`, which writes a
//! new file, and triggers are refused.
//!
//! The control of transactions is always allowed too (`docs/V1.md`,
//! section 2.5): `START TRANSACTION`, `BEGIN`, `COMMIT`, `ROLLBACK`,
//! savepoints, `SET TRANSACTION`, `SET autocommit`. Its forms that can lift
//! the read-only mode of the session (`READ WRITE`, the variables
//! `transaction_read_only` and `tx_read_only`) stand apart: a read-only
//! session refuses them. `SET GLOBAL TRANSACTION` changes the whole server:
//! forbidden. XA transactions (`XA START`...) are refused apart, with their
//! own message: a prepared one outlives its session and keeps its locks.
//!
//! Also tells whether a query can change the current database of the
//! session (`changes_database`): the session reads it again only then.
//!
//! The queries run when a connection opens (`SET NAMES`,
//! `SET SESSION sql_mode`...) are checked separately
//! (`session_setting_refusal`): only settings of the session itself are
//! allowed, the pragmas of `SESSION_PRAGMAS` for SQLite.

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use serde::{Deserialize, Serialize};
use sqlparser::ast::{
    ContextModifier, Expr, ObjectName, ObjectType, OnConflict, OnConflictAction, OnInsert, Set,
    SetExpr, SqliteOnConflict, Statement, TransactionAccessMode, TransactionMode, Value,
    visit_relations,
};
use ts_rs::TS;

use crate::Dialect;
use crate::limit::returns_rows;
use crate::mysql_statements::MySqlStatement;
use crate::parse::{Parsed, parse_statements};
use crate::sqlite_files::attached_file;
use crate::sqlite_pragmas::{self, PragmaRole};
use crate::sqlite_statements::{PragmaForm, SqliteStatement};
use crate::temporary::{TableName, table_name};

/// Nature of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    /// Read: `SELECT` that returns rows, `SHOW`, `DESCRIBE`, `EXPLAIN`, `USE`.
    Read,
    /// Setting of the session (`SET @v = 1`...), with reads at most: allowed,
    /// it only makes sense on the connection of the session.
    Setting,
    /// Control of a transaction (`START TRANSACTION`, `COMMIT`, `ROLLBACK`,
    /// savepoints, `SET TRANSACTION`, `SET autocommit`), with reads and
    /// settings at most: always allowed, on the connection of the session.
    Transaction,
    /// Control of a transaction that can lift the read-only mode of the
    /// session (`START TRANSACTION READ WRITE`, `SET TRANSACTION READ WRITE`,
    /// `transaction_read_only`, `tx_read_only`): allowed, except in a
    /// read-only session.
    ReadWriteTransaction,
    /// Write of data or schema, allowed when each of its kinds is.
    Write(BTreeSet<WriteKind>),
    /// Write that no switch allows: procedure call, database, trigger,
    /// account, file, server setting, unknown pragma...
    Forbidden,
    /// Statement of an XA (distributed) transaction, which Prisme does not
    /// support: a prepared one outlives its session, with its locks.
    XaTransaction,
    /// Query that cannot be parsed: its nature is unknown.
    Unparsed,
}

/// Kind of write, allowed or not by its switch in the settings of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WriteKind {
    /// `INSERT`, and the insertion of `REPLACE`, temporary tables included.
    Insert,
    /// `UPDATE`, single or multiple-table, and `ON DUPLICATE KEY UPDATE`,
    /// temporary tables included.
    Update,
    /// `DELETE`, single or multiple-table, and the deletion of `REPLACE`,
    /// temporary tables included.
    Delete,
    /// `TRUNCATE TABLE`, temporary tables included.
    TruncateTable,
    /// `CREATE TABLE`.
    CreateTable,
    /// `ALTER TABLE`, `CREATE INDEX`, `DROP INDEX`.
    AlterTable,
    /// `RENAME TABLE`.
    RenameTable,
    /// `DROP TABLE`.
    DropTable,
    /// `CREATE TEMPORARY TABLE`.
    CreateTemporaryTable,
    /// `ALTER TABLE`, `CREATE INDEX`, `DROP INDEX` on a temporary table.
    AlterTemporaryTable,
    /// `RENAME TABLE` of temporary tables.
    RenameTemporaryTable,
    /// `DROP TEMPORARY TABLE`, and `DROP TABLE` of temporary tables only.
    DropTemporaryTable,
    /// `CREATE [OR REPLACE] VIEW`, `ALTER VIEW`.
    CreateView,
    /// `DROP VIEW`.
    DropView,
    /// `CREATE FUNCTION` (stored function).
    CreateFunction,
    /// `DROP FUNCTION`.
    DropFunction,
    /// `CREATE PROCEDURE`.
    CreateProcedure,
    /// `DROP PROCEDURE`.
    DropProcedure,
    /// `CREATE EVENT`.
    CreateEvent,
    /// `ALTER EVENT`.
    AlterEvent,
    /// `DROP EVENT`.
    DropEvent,
    /// `ATTACH` and `DETACH` of SQLite, for the session.
    Attach,
    /// `VACUUM`, `REINDEX`, `ANALYZE` of SQLite, and its pragmas that write
    /// in the file (`optimize`, `wal_checkpoint`, `incremental_vacuum`).
    Maintenance,
}

impl WriteKind {
    /// Every kind, in the order of the settings.
    pub const ALL: [Self; 23] = [
        Self::Insert,
        Self::Update,
        Self::Delete,
        Self::TruncateTable,
        Self::CreateTable,
        Self::AlterTable,
        Self::RenameTable,
        Self::DropTable,
        Self::CreateTemporaryTable,
        Self::AlterTemporaryTable,
        Self::RenameTemporaryTable,
        Self::DropTemporaryTable,
        Self::CreateView,
        Self::DropView,
        Self::CreateFunction,
        Self::DropFunction,
        Self::CreateProcedure,
        Self::DropProcedure,
        Self::CreateEvent,
        Self::AlterEvent,
        Self::DropEvent,
        Self::Attach,
        Self::Maintenance,
    ];

    /// Name of the kind, as serialized (`create_table`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::TruncateTable => "truncate_table",
            Self::CreateTable => "create_table",
            Self::AlterTable => "alter_table",
            Self::RenameTable => "rename_table",
            Self::DropTable => "drop_table",
            Self::CreateTemporaryTable => "create_temporary_table",
            Self::AlterTemporaryTable => "alter_temporary_table",
            Self::RenameTemporaryTable => "rename_temporary_table",
            Self::DropTemporaryTable => "drop_temporary_table",
            Self::CreateView => "create_view",
            Self::DropView => "drop_view",
            Self::CreateFunction => "create_function",
            Self::DropFunction => "drop_function",
            Self::CreateProcedure => "create_procedure",
            Self::DropProcedure => "drop_procedure",
            Self::CreateEvent => "create_event",
            Self::AlterEvent => "alter_event",
            Self::DropEvent => "drop_event",
            Self::Attach => "attach",
            Self::Maintenance => "maintenance",
        }
    }

    /// Does a read-only session still allow the kind, under its switch? Only
    /// `ATTACH` / `DETACH`: a file attached by a read-only connection is
    /// read-only too, and a missing one is not created
    /// (`docs/SQL-COMPARISON.md`, section 6).
    pub fn allowed_read_only(self) -> bool {
        matches!(self, Self::Attach)
    }

    /// Kind of the name `name` (`create_table`), `None` if unknown.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }
}

/// Classifies the text `sql`, without a known temporary table. If it
/// contains several queries, it needs the kinds of all its writes; a single
/// forbidden write forbids it.
pub fn classify(sql: &str, dialect: Dialect) -> StatementKind {
    classify_with(sql, dialect, &|_| false)
}

/// Classifies the text `sql` as `classify` does; `temporary` tells whether a
/// table named in it is a temporary table of the connection.
pub fn classify_with(
    sql: &str,
    dialect: Dialect,
    temporary: &dyn Fn(&TableName) -> bool,
) -> StatementKind {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return StatementKind::Unparsed;
    };
    // A table of the temporary schema (`temp.t`) is a temporary table.
    let temporary = |table: &TableName| in_temporary_schema(table, dialect) || temporary(table);
    let mut kinds = BTreeSet::new();
    let mut setting = false;
    let mut transaction = None;
    for statement in &statements {
        if writes_by_pragma_function(statement, dialect) {
            kinds.insert(WriteKind::Maintenance);
        }
        match need(statement, dialect, &temporary) {
            Need::Read => {}
            Need::Setting => setting = true,
            Need::Transaction { read_write } => {
                transaction = Some(read_write || transaction == Some(true));
            }
            Need::Write(needed) => kinds.extend(needed),
            Need::Forbidden => return StatementKind::Forbidden,
            Need::Xa => return StatementKind::XaTransaction,
        }
    }
    match transaction {
        _ if !kinds.is_empty() => StatementKind::Write(kinds),
        Some(true) => StatementKind::ReadWriteTransaction,
        Some(false) => StatementKind::Transaction,
        None if setting => StatementKind::Setting,
        None => StatementKind::Read,
    }
}

/// What one statement needs to run.
enum Need {
    /// Nothing: a read.
    Read,
    /// The connection of the session: a setting of the session.
    Setting,
    /// The connection of the session: a control of the transaction; with
    /// `read_write`, a read-write session too.
    Transaction {
        /// Can it lift the read-only mode of the session?
        read_write: bool,
    },
    /// The switches of these kinds of write.
    Write(Vec<WriteKind>),
    /// What no switch allows.
    Forbidden,
    /// Nothing: an XA transaction, which Prisme refuses.
    Xa,
}

/// What a statement needs in the dialect; `temporary` tells the temporary
/// tables of the connection.
fn need(statement: &Parsed, dialect: Dialect, temporary: &dyn Fn(&TableName) -> bool) -> Need {
    match statement {
        Parsed::Sql(statement) if is_read(statement) => Need::Read,
        Parsed::Sql(statement) => {
            if let Some(read_write) = transaction_control(statement) {
                Need::Transaction { read_write }
            } else if matches!(statement, Statement::Set(set) if is_session_setting(set)) {
                Need::Setting
            } else {
                write_kinds(statement, dialect, temporary).map_or(Need::Forbidden, Need::Write)
            }
        }
        Parsed::MySql(MySqlStatement::SetTransaction { global: true, .. }) => Need::Forbidden,
        Parsed::MySql(
            MySqlStatement::SetTransaction { read_write, .. }
            | MySqlStatement::StartTransaction { read_write },
        ) => Need::Transaction {
            read_write: *read_write,
        },
        Parsed::MySql(MySqlStatement::EndTransaction { .. }) => {
            Need::Transaction { read_write: false }
        }
        Parsed::MySql(MySqlStatement::Xa) => Need::Xa,
        Parsed::MySql(statement) => mysql_write_kind(statement, temporary)
            .map_or(Need::Forbidden, |kind| Need::Write(vec![kind])),
        Parsed::Sqlite(statement) => sqlite_need(statement),
    }
}

/// What a statement of SQLite that `sqlparser` does not parse needs.
fn sqlite_need(statement: &SqliteStatement) -> Need {
    match statement {
        SqliteStatement::Pragma { name, form } => {
            let bare = *form == PragmaForm::Bare;
            match sqlite_pragmas::role(name) {
                Some(PragmaRole::Read) if bare => Need::Read,
                Some(PragmaRole::ReadArgument) => Need::Read,
                Some(PragmaRole::Setting) if bare => Need::Read,
                Some(PragmaRole::Setting) => Need::Setting,
                Some(PragmaRole::Maintenance) => Need::Write(vec![WriteKind::Maintenance]),
                Some(PragmaRole::Read) | None => Need::Forbidden,
            }
        }
        SqliteStatement::Detach => Need::Write(vec![WriteKind::Attach]),
        // VACUUM INTO writes a new file.
        SqliteStatement::Vacuum { into: true } => Need::Forbidden,
        SqliteStatement::Vacuum { into: false } | SqliteStatement::Reindex => {
            Need::Write(vec![WriteKind::Maintenance])
        }
    }
}

/// Is the table a table of the temporary schema of the dialect (`temp.t` in
/// SQLite)?
fn in_temporary_schema(table: &TableName, dialect: Dialect) -> bool {
    dialect
        .spec()
        .temporary_schema
        .zip(table.schema.as_deref())
        .is_some_and(|(temporary, schema)| schema.eq_ignore_ascii_case(temporary))
}

/// Is the name that of a table of the temporary schema of the dialect?
fn named_in_temporary_schema(name: &ObjectName, dialect: Dialect) -> bool {
    table_name(name).is_some_and(|table| in_temporary_schema(&table, dialect))
}

/// Does the statement read the table function of a pragma that writes in
/// the file (`SELECT * FROM pragma_optimize`, SQLite)?
fn writes_by_pragma_function(statement: &Parsed, dialect: Dialect) -> bool {
    let Parsed::Sql(statement) = statement else {
        return false;
    };
    dialect.spec().statements == crate::Statements::Sqlite
        && visit_relations(statement, |name| {
            let writes = name
                .0
                .last()
                .and_then(|part| part.as_ident())
                .is_some_and(|ident| {
                    sqlite_pragmas::function_writes(&ident.value.to_ascii_lowercase())
                });
            if writes {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .is_break()
}

/// Can an `ATTACH` of SQLite name this file? A literal that
/// `sqlite_files::attached_file` accepts: an absolute path, `':memory:'`,
/// or the URI of a file in the default file system; never a computed name.
fn attachable(file: &Expr) -> bool {
    let Expr::Value(value) = file else {
        return false;
    };
    let Value::SingleQuotedString(name) = &value.value else {
        return false;
    };
    attached_file(name).is_some()
}

/// Variables that set the read-only mode of the session.
const READ_ONLY_VARIABLES: [&str; 2] = ["transaction_read_only", "tx_read_only"];

/// Is a statement of `sqlparser` a control of the transaction? `Some`, with
/// whether it can lift the read-only mode of the session; `None` for any
/// other statement. A `BEGIN ... END` block, which holds statements, is none.
fn transaction_control(statement: &Statement) -> Option<bool> {
    match statement {
        Statement::StartTransaction {
            modes,
            statements,
            exception,
            ..
        } if statements.is_empty() && exception.is_none() => Some(gives_read_write(modes)),
        Statement::Commit { .. }
        | Statement::Rollback { .. }
        | Statement::Savepoint { .. }
        | Statement::ReleaseSavepoint { .. } => Some(false),
        Statement::Set(Set::SetTransaction { modes, .. }) => Some(gives_read_write(modes)),
        Statement::Set(set) if !is_global(set) => {
            let names = assigned_variables(set);
            if names
                .iter()
                .any(|name| READ_ONLY_VARIABLES.contains(&name.as_str()))
            {
                Some(true)
            } else {
                names
                    .iter()
                    .any(|name| name == "autocommit")
                    .then_some(false)
            }
        }
        _ => None,
    }
}

/// Do transaction modes give the `READ WRITE` access mode?
fn gives_read_write(modes: &[TransactionMode]) -> bool {
    modes.contains(&TransactionMode::AccessMode(
        TransactionAccessMode::ReadWrite,
    ))
}

/// Names of the variables a `SET` assigns, in lowercase, without their
/// session scope (`@@session.autocommit` gives `autocommit`); none for the
/// other forms of `SET`.
fn assigned_variables(set: &Set) -> Vec<String> {
    let bare = |name: String| {
        let name = name.to_ascii_lowercase();
        let name = name.strip_prefix("@@").unwrap_or(&name);
        ["session.", "local."]
            .iter()
            .find_map(|scope| name.strip_prefix(scope))
            .unwrap_or(name)
            .to_string()
    };
    match set {
        Set::SingleAssignment { variable, .. } => vec![bare(variable.to_string())],
        Set::MultipleAssignments { assignments } => assignments
            .iter()
            .map(|assignment| bare(assignment.name.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Is a `SET` a setting of the session that is always allowed? Not global;
/// the controls of the transaction are told apart before
/// (`transaction_control`).
fn is_session_setting(set: &Set) -> bool {
    match set {
        Set::SingleAssignment { .. } | Set::MultipleAssignments { .. } => !is_global(set),
        Set::SetNames { .. } | Set::SetNamesDefault {} | Set::SetTimeZone { .. } => true,
        _ => false,
    }
}

/// Do the names designate temporary tables only? Never for an empty list.
fn all_temporary<'a>(
    names: impl IntoIterator<Item = &'a ObjectName>,
    temporary: &dyn Fn(&TableName) -> bool,
) -> bool {
    let mut names = names.into_iter().peekable();
    names.peek().is_some()
        && names.all(|name| table_name(name).is_some_and(|table| temporary(&table)))
}

/// Kind of a write, according to whether it targets temporary tables only.
fn on(temporary: bool, kind: WriteKind, temporary_kind: WriteKind) -> WriteKind {
    if temporary { temporary_kind } else { kind }
}

/// Kinds of a write of `sqlparser` in the dialect, `None` if no switch
/// allows it; `temporary` tells the temporary tables of the connection.
fn write_kinds(
    statement: &Statement,
    dialect: Dialect,
    temporary: &dyn Fn(&TableName) -> bool,
) -> Option<Vec<WriteKind>> {
    use WriteKind::*;
    let sqlite = dialect.spec().statements == crate::Statements::Sqlite;
    let kinds = match statement {
        Statement::Insert(insert) => {
            let mut kinds = vec![Insert];
            // REPLACE deletes the rows whose keys the new ones take.
            if insert.replace_into || insert.or == Some(SqliteOnConflict::Replace) {
                kinds.push(Delete);
            }
            if matches!(
                insert.on,
                Some(
                    OnInsert::DuplicateKeyUpdate(_)
                        | OnInsert::OnConflict(OnConflict {
                            action: OnConflictAction::DoUpdate(_),
                            ..
                        })
                )
            ) {
                kinds.push(Update);
            }
            kinds
        }
        Statement::Update(update) if update.or == Some(SqliteOnConflict::Replace) => {
            vec![Update, Delete]
        }
        Statement::Update(_) => vec![Update],
        Statement::Delete(_) => vec![Delete],
        // WITH ... INSERT / UPDATE / DELETE; SELECT ... INTO stays forbidden.
        Statement::Query(query) => match query.body.as_ref() {
            SetExpr::Insert(statement)
            | SetExpr::Update(statement)
            | SetExpr::Delete(statement) => {
                return write_kinds(statement, dialect, temporary);
            }
            _ => return None,
        },
        // EXPLAIN ANALYZE runs the query.
        Statement::Explain { statement, .. } => {
            return write_kinds(statement, dialect, temporary);
        }
        Statement::CreateTable(create) => vec![on(
            create.temporary || named_in_temporary_schema(&create.name, dialect),
            CreateTable,
            CreateTemporaryTable,
        )],
        // A virtual table of SQLite (fts5, rtree) is a table.
        Statement::CreateVirtualTable { name, .. } => vec![on(
            named_in_temporary_schema(name, dialect),
            CreateTable,
            CreateTemporaryTable,
        )],
        Statement::AttachDatabase {
            database_file_name, ..
        } if sqlite && attachable(database_file_name) => vec![Attach],
        Statement::Analyze(_) if sqlite => vec![Maintenance],
        Statement::AlterTable(alter) => vec![on(
            all_temporary([&alter.name], temporary),
            AlterTable,
            AlterTemporaryTable,
        )],
        // SQLite names the schema on the index (`CREATE INDEX temp.i ON x`),
        // its table staying unqualified.
        Statement::CreateIndex(index) => vec![on(
            all_temporary([&index.table_name], temporary)
                || index
                    .name
                    .as_ref()
                    .is_some_and(|name| named_in_temporary_schema(name, dialect)),
            AlterTable,
            AlterTemporaryTable,
        )],
        Statement::RenameTable(renames) => vec![on(
            all_temporary(renames.iter().map(|rename| &rename.old_name), temporary),
            RenameTable,
            RenameTemporaryTable,
        )],
        Statement::Truncate(_) => vec![TruncateTable],
        Statement::CreateView(_) | Statement::AlterView { .. } => vec![CreateView],
        Statement::Drop {
            object_type,
            names,
            temporary: drop_temporary,
            table,
            ..
        } => match object_type {
            // DROP TEMPORARY TABLE only drops temporary tables.
            ObjectType::Table => vec![on(
                *drop_temporary || all_temporary(names, temporary),
                DropTable,
                DropTemporaryTable,
            )],
            ObjectType::View => vec![DropView],
            // SQLite drops an index without its table: `DROP INDEX temp.i`.
            ObjectType::Index => vec![on(
                all_temporary(table, temporary)
                    || (!names.is_empty()
                        && names
                            .iter()
                            .all(|name| named_in_temporary_schema(name, dialect))),
                AlterTable,
                AlterTemporaryTable,
            )],
            _ => return None,
        },
        Statement::DropFunction(_) => vec![DropFunction],
        Statement::DropProcedure { .. } => vec![DropProcedure],
        _ => return None,
    };
    Some(kinds)
}

/// Kind of a write that `sqlparser` does not parse, `None` for a statement
/// that writes nothing (a control of the transaction); `temporary` tells the
/// temporary tables of the connection.
fn mysql_write_kind(
    statement: &MySqlStatement,
    temporary: &dyn Fn(&TableName) -> bool,
) -> Option<WriteKind> {
    Some(match statement {
        MySqlStatement::CreateProcedure { .. } => WriteKind::CreateProcedure,
        MySqlStatement::CreateFunction { .. } => WriteKind::CreateFunction,
        MySqlStatement::CreateEvent { .. } => WriteKind::CreateEvent,
        MySqlStatement::AlterEvent { .. } => WriteKind::AlterEvent,
        MySqlStatement::DropEvent { .. } => WriteKind::DropEvent,
        MySqlStatement::AlterTable { name } => on(
            all_temporary([name], temporary),
            WriteKind::AlterTable,
            WriteKind::AlterTemporaryTable,
        ),
        MySqlStatement::SetTransaction { .. }
        | MySqlStatement::StartTransaction { .. }
        | MySqlStatement::EndTransaction { .. }
        | MySqlStatement::Xa => return None,
    })
}

/// Does the text `sql` end a transaction with `COMMIT` or `ROLLBACK` (not
/// to a savepoint)? The server may close the connection then (`RELEASE`,
/// `completion_type = 2`).
pub fn ends_transaction(sql: &str, dialect: Dialect) -> bool {
    parse_statements(sql, dialect).is_ok_and(|statements| {
        statements.iter().any(|statement| {
            matches!(
                statement,
                Parsed::Sql(
                    Statement::Commit { .. }
                        | Statement::Rollback {
                            savepoint: None,
                            ..
                        }
                ) | Parsed::MySql(MySqlStatement::EndTransaction { .. })
            )
        })
    })
}

/// Can the text `sql` change the current database of the session? `USE`,
/// `DROP DATABASE` / `DROP SCHEMA` (the current database may disappear), and
/// a text Prisme cannot parse, whose effect is unknown.
pub fn changes_database(sql: &str, dialect: Dialect) -> bool {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return true;
    };
    statements.iter().any(|statement| {
        matches!(
            statement,
            Parsed::Sql(
                Statement::Use(_)
                    | Statement::Drop {
                        object_type: ObjectType::Database | ObjectType::Schema,
                        ..
                    }
            )
        )
    })
}

/// Refusal of a query to run when a connection opens, `None` if it is
/// allowed: only session settings (`SET NAMES`, `SET SESSION x = ...`,
/// `SET @v = ...`, `SET TIME ZONE`...) are. A global setting (`SET GLOBAL`,
/// `SET @@global.x`), which would change the whole server, and any other
/// query are refused, like a query Prisme does not understand.
pub fn session_setting_refusal(sql: &str, dialect: Dialect) -> Option<SettingRefusal> {
    let Ok(statements) = parse_statements(sql, dialect) else {
        return Some(SettingRefusal::NotUnderstood);
    };
    for statement in &statements {
        match statement {
            Parsed::Sql(Statement::Set(set)) if is_global(set) => {
                return Some(SettingRefusal::Global);
            }
            Parsed::MySql(MySqlStatement::SetTransaction { global: true, .. }) => {
                return Some(SettingRefusal::Global);
            }
            Parsed::Sql(Statement::Set(_))
            | Parsed::MySql(MySqlStatement::SetTransaction { .. }) => {}
            Parsed::Sqlite(statement @ SqliteStatement::Pragma { .. })
                if matches!(sqlite_need(statement), Need::Setting) => {}
            _ => return Some(SettingRefusal::NotASetting),
        }
    }
    None
}

/// Why a query to run when a connection opens is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingRefusal {
    /// Prisme does not understand the query.
    NotUnderstood,
    /// A global setting, which would change the whole server.
    Global,
    /// Not a setting (`SET ...`, a pragma of `SESSION_PRAGMAS` with its value
    /// for SQLite).
    NotASetting,
}

/// Does a `SET` target the whole server (`GLOBAL`, `@@global.x`, `@@persist.x`)?
fn is_global(set: &Set) -> bool {
    let global_name = |name: &str| {
        let name = name.to_ascii_lowercase();
        name.starts_with("@@global.") || name.starts_with("@@persist")
    };
    match set {
        Set::SingleAssignment {
            scope, variable, ..
        } => scope == &Some(ContextModifier::Global) || global_name(&variable.to_string()),
        Set::MultipleAssignments { assignments } => assignments.iter().any(|assignment| {
            assignment.scope == Some(ContextModifier::Global)
                || global_name(&assignment.name.to_string())
        }),
        _ => false,
    }
}

/// Tells whether a query is a read.
fn is_read(statement: &Statement) -> bool {
    match statement {
        Statement::Query(query) => returns_rows(&query.body),
        // EXPLAIN alone shows the plan without running; EXPLAIN ANALYZE runs the query.
        Statement::Explain {
            analyze, statement, ..
        } => !analyze || is_read(statement),
        Statement::ExplainTable { .. }
        | Statement::ShowFunctions { .. }
        | Statement::ShowVariable { .. }
        | Statement::ShowStatus { .. }
        | Statement::ShowVariables { .. }
        | Statement::ShowCreate { .. }
        | Statement::ShowColumns { .. }
        | Statement::ShowCatalogs { .. }
        | Statement::ShowDatabases { .. }
        | Statement::ShowProcessList { .. }
        | Statement::ShowSchemas { .. }
        | Statement::ShowCharset(_)
        | Statement::ShowObjects(_)
        | Statement::ShowTables { .. }
        | Statement::ShowViews { .. }
        | Statement::ShowCollation { .. }
        | Statement::Use(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_session_settings_at_connection() {
        let refused = |sql| session_setting_refusal(sql, Dialect::MariaDb).is_some();
        assert!(!refused("SET NAMES utf8mb4"));
        assert!(!refused("SET SESSION sql_mode = 'ANSI_QUOTES'"));
        assert!(!refused("SET @start = NOW()"));
        assert!(!refused("SET time_zone = '+00:00'"));
        assert!(!refused(
            "SET SESSION TRANSACTION ISOLATION LEVEL READ COMMITTED"
        ));
        assert!(refused("SET GLOBAL max_connections = 10"));
        assert!(refused("SET @@global.sql_mode = ''"));
        assert_eq!(
            session_setting_refusal("SET GLOBAL TRANSACTION READ WRITE", Dialect::MariaDb),
            Some(SettingRefusal::Global)
        );
        assert!(refused("START TRANSACTION"));
        assert!(refused("DELETE FROM t"));
        assert!(refused("SELECT 1"));
        assert!(refused("SET whatever you like"));
    }

    #[test]
    fn tells_the_queries_that_end_a_transaction() {
        let ends = |sql| ends_transaction(sql, Dialect::MySql);
        assert!(ends("COMMIT"));
        assert!(ends("SELECT 1; ROLLBACK"));
        assert!(ends("COMMIT RELEASE"));
        assert!(!ends("ROLLBACK TO SAVEPOINT s"));
        assert!(!ends("START TRANSACTION"));
        assert!(!ends("SELECT 1 FROM"));
    }

    #[test]
    fn tells_the_queries_that_can_change_the_current_database() {
        let changes = |sql| changes_database(sql, Dialect::MySql);
        assert!(changes("USE sakila"));
        assert!(changes("DROP DATABASE sakila"));
        assert!(changes("DROP SCHEMA IF EXISTS sakila"));
        assert!(changes("SELECT 1 FROM"));
        assert!(!changes("SELECT DATABASE()"));
        assert!(!changes("SHOW TABLES"));
        assert!(!changes("DROP TABLE t"));
    }

    fn kind(sql: &str) -> StatementKind {
        classify(sql, Dialect::MySql)
    }

    #[test]
    fn allows_the_settings_of_the_session() {
        for sql in [
            "SET @a = 1",
            "SET @a = 1, @b = 'x'",
            "SET SESSION sql_mode = 'ANSI_QUOTES'",
            "SET @@session.sql_mode = ''",
            "SET LOCAL time_zone = '+00:00'",
            "SET sql_safe_updates = 1",
            "SET NAMES utf8mb4",
            "SET @autocommit = 0",
            "SET @a = 1; SELECT @a",
        ] {
            assert_eq!(kind(sql), StatementKind::Setting, "query: {sql}");
        }
        assert_eq!(
            kind("SET @a = 1; DELETE FROM t"),
            StatementKind::Write([WriteKind::Delete].into())
        );
    }

    #[test]
    fn reads() {
        for sql in [
            "SELECT * FROM t",
            "SELECT * FROM t FOR UPDATE",
            "WITH x AS (SELECT 1) SELECT * FROM x",
            "SELECT a FROM t UNION SELECT a FROM u",
            "SHOW TABLES",
            "SHOW CREATE TABLE t",
            "SHOW VARIABLES LIKE 'sql_mode'",
            "DESCRIBE t",
            "EXPLAIN SELECT * FROM t",
            "EXPLAIN UPDATE t SET a = 1",
            "USE test",
            // Compound time units, unknown to `sqlparser` (`mysql_parser` module).
            "select extract(year_month from operation_date) from accounts.operations",
            "SELECT DATE_ADD(d, INTERVAL '1 2' DAY_HOUR) FROM t",
        ] {
            assert_eq!(kind(sql), StatementKind::Read, "query: {sql}");
        }
    }

    #[test]
    fn writes_need_the_switches_of_their_kinds() {
        use WriteKind::*;
        for (sql, kinds) in [
            ("INSERT INTO t VALUES (1)", vec![Insert]),
            ("INSERT INTO t SELECT * FROM u", vec![Insert]),
            (
                "INSERT INTO t VALUES (1) ON DUPLICATE KEY UPDATE a = 2",
                vec![Insert, Update],
            ),
            ("REPLACE INTO t VALUES (1)", vec![Insert, Delete]),
            ("UPDATE t SET a = 1", vec![Update]),
            ("UPDATE t JOIN u ON t.a = u.a SET t.b = u.b", vec![Update]),
            ("DELETE FROM t", vec![Delete]),
            ("DELETE t FROM t JOIN u ON t.a = u.a", vec![Delete]),
            (
                "WITH x AS (SELECT 1 AS a) DELETE FROM t WHERE a IN (SELECT a FROM x)",
                vec![Delete],
            ),
            ("EXPLAIN ANALYZE DELETE FROM t", vec![Delete]),
            ("CREATE TABLE t (a INT)", vec![CreateTable]),
            (
                "CREATE TEMPORARY TABLE t (a INT)",
                vec![CreateTemporaryTable],
            ),
            ("CREATE TABLE t AS SELECT 1", vec![CreateTable]),
            ("ALTER TABLE t ADD b INT", vec![AlterTable]),
            ("ALTER TABLE t ALTER INDEX i INVISIBLE", vec![AlterTable]),
            ("ALTER TABLE t RENAME KEY i TO j", vec![AlterTable]),
            ("CREATE INDEX i ON t (a)", vec![AlterTable]),
            ("DROP INDEX i ON t", vec![AlterTable]),
            ("RENAME TABLE t TO u", vec![RenameTable]),
            ("TRUNCATE TABLE t", vec![TruncateTable]),
            ("DROP TABLE t", vec![DropTable]),
            ("DROP TEMPORARY TABLE IF EXISTS t", vec![DropTemporaryTable]),
            ("CREATE VIEW v AS SELECT 1", vec![CreateView]),
            ("CREATE OR REPLACE VIEW v AS SELECT 1", vec![CreateView]),
            ("ALTER VIEW v AS SELECT 2", vec![CreateView]),
            ("DROP VIEW v", vec![DropView]),
            (
                "CREATE FUNCTION f() RETURNS INT RETURN 1",
                vec![CreateFunction],
            ),
            ("DROP FUNCTION f", vec![DropFunction]),
            (
                "CREATE PROCEDURE p() BEGIN SELECT 1; END",
                vec![CreateProcedure],
            ),
            ("DROP PROCEDURE IF EXISTS p", vec![DropProcedure]),
            (
                "CREATE EVENT e ON SCHEDULE EVERY 1 DAY DO DELETE FROM t",
                vec![CreateEvent],
            ),
            ("ALTER EVENT e DISABLE", vec![AlterEvent]),
            ("DROP EVENT e", vec![DropEvent]),
            (
                "SELECT 1; DELETE FROM t; INSERT INTO t VALUES (1)",
                vec![Insert, Delete],
            ),
        ] {
            assert_eq!(
                kind(sql),
                StatementKind::Write(kinds.into_iter().collect()),
                "query: {sql}"
            );
        }
    }

    #[test]
    fn changes_of_temporary_tables_only_need_their_switches() {
        use WriteKind::*;
        // `tmp` and `shop.tmp2` are temporary tables of the connection.
        let temporary = |table: &TableName| {
            matches!(
                (table.schema.as_deref(), table.name.as_str()),
                (None, "tmp") | (Some("shop"), "tmp2")
            )
        };
        let kind = |sql| classify_with(sql, Dialect::MySql, &temporary);
        for (sql, kinds) in [
            // The data of the temporary tables: the switches of the data.
            ("INSERT INTO tmp SELECT * FROM t", vec![Insert]),
            ("REPLACE INTO shop.tmp2 VALUES (1)", vec![Insert, Delete]),
            ("UPDATE tmp SET a = 1", vec![Update]),
            ("DELETE FROM tmp", vec![Delete]),
            ("TRUNCATE TABLE tmp", vec![TruncateTable]),
            ("ALTER TABLE tmp ADD b INT", vec![AlterTemporaryTable]),
            (
                "ALTER TABLE tmp ALTER INDEX i INVISIBLE",
                vec![AlterTemporaryTable],
            ),
            ("CREATE INDEX i ON tmp (a)", vec![AlterTemporaryTable]),
            ("DROP INDEX i ON tmp", vec![AlterTemporaryTable]),
            ("RENAME TABLE tmp TO tmp3", vec![RenameTemporaryTable]),
            ("RENAME TABLE tmp TO a, t TO b", vec![RenameTable]),
            ("DROP TABLE tmp, shop.tmp2", vec![DropTemporaryTable]),
            // A permanent table among the tables: the switch of the tables.
            ("DROP TABLE tmp, t", vec![DropTable]),
            ("DROP TEMPORARY TABLE t", vec![DropTemporaryTable]),
            // Another database, or another case: another table.
            ("DROP TABLE shop.tmp", vec![DropTable]),
            ("DROP TABLE TMP", vec![DropTable]),
        ] {
            assert_eq!(
                kind(sql),
                StatementKind::Write(kinds.into_iter().collect()),
                "query: {sql}"
            );
        }
    }

    #[test]
    fn forbids_the_writes_without_a_switch() {
        for sql in [
            "CALL p()",
            "CREATE DATABASE d",
            "DROP DATABASE d",
            "DROP TRIGGER g",
            "GRANT SELECT ON t TO u",
            "SET GLOBAL max_connections = 10",
            "SET @@global.sql_mode = ''",
            "SET GLOBAL TRANSACTION READ WRITE",
            "SET GLOBAL TRANSACTION ISOLATION LEVEL READ COMMITTED",
            "SET GLOBAL autocommit = 0",
            "SELECT * INTO t2 FROM t",
            "DELETE FROM t; CALL p()",
            "COMMIT; CALL p()",
        ] {
            assert_eq!(kind(sql), StatementKind::Forbidden, "query: {sql}");
        }
        // Refused too, as `sqlparser` does not parse their MySQL syntax.
        for sql in [
            "LOAD DATA INFILE 'f.csv' INTO TABLE t",
            "SET PERSIST max_connections = 10",
            "SELECT * INTO OUTFILE '/tmp/t.csv' FROM t",
            "CREATE TRIGGER g BEFORE INSERT ON t FOR EACH ROW SET NEW.a = 1",
        ] {
            assert_eq!(kind(sql), StatementKind::Unparsed, "query: {sql}");
        }
    }

    #[test]
    fn allows_the_control_of_transactions() {
        for sql in [
            "START TRANSACTION",
            "START TRANSACTION READ ONLY",
            "START TRANSACTION WITH CONSISTENT SNAPSHOT",
            "BEGIN",
            "BEGIN WORK",
            "COMMIT",
            "COMMIT AND CHAIN",
            "COMMIT RELEASE",
            "ROLLBACK NO RELEASE",
            "ROLLBACK",
            "ROLLBACK TO SAVEPOINT s",
            "SAVEPOINT s",
            "RELEASE SAVEPOINT s",
            "SET TRANSACTION ISOLATION LEVEL READ COMMITTED",
            "SET SESSION TRANSACTION READ ONLY",
            "SET autocommit = 0",
            "SET @a = 1, @@session.autocommit = 0",
            "START TRANSACTION; SELECT 1; COMMIT",
        ] {
            assert_eq!(kind(sql), StatementKind::Transaction, "query: {sql}");
        }
        // The forms that can lift the read-only mode of the session.
        for sql in [
            "START TRANSACTION READ WRITE",
            "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ WRITE",
            "SET TRANSACTION READ WRITE",
            "SET SESSION TRANSACTION ISOLATION LEVEL SERIALIZABLE, READ WRITE",
            "SET SESSION transaction_read_only = OFF",
            "SET @@tx_read_only = 0",
            "BEGIN; SET TRANSACTION READ WRITE",
        ] {
            assert_eq!(
                kind(sql),
                StatementKind::ReadWriteTransaction,
                "query: {sql}"
            );
        }
        // XA transactions are refused, whatever else the text holds.
        for sql in [
            "XA START 'x1'",
            "SELECT 1; XA RECOVER",
            "DELETE FROM t; XA END 'x1'",
        ] {
            assert_eq!(kind(sql), StatementKind::XaTransaction, "query: {sql}");
        }
        // A write in the text needs its switch.
        assert_eq!(
            kind("START TRANSACTION; DELETE FROM t"),
            StatementKind::Write([WriteKind::Delete].into_iter().collect())
        );
        // `transaction_isolation` is a setting of the session.
        assert_eq!(
            kind("SET SESSION transaction_isolation = 'READ-COMMITTED'"),
            StatementKind::Setting
        );
    }

    #[test]
    fn names_the_kinds_as_serialized() {
        for kind in WriteKind::ALL {
            let name = yaml_serde::to_string(&kind).unwrap();
            assert_eq!(name.trim(), kind.as_str());
            assert_eq!(WriteKind::from_name(kind.as_str()), Some(kind));
        }
        assert_eq!(WriteKind::from_name("grant"), None);
    }

    #[test]
    fn unparsable() {
        assert_eq!(
            kind("SELECT * FROM t PROCEDURE ANALYSE()"),
            StatementKind::Unparsed
        );
    }

    /// Nature of a query of SQLite, without a known temporary table.
    fn sqlite(sql: &str) -> StatementKind {
        classify(sql, Dialect::Sqlite)
    }

    /// A write of these kinds.
    fn writes(kinds: &[WriteKind]) -> StatementKind {
        StatementKind::Write(kinds.iter().copied().collect())
    }

    #[test]
    fn classifies_the_writes_of_sqlite() {
        use WriteKind::*;
        for (sql, kinds) in [
            ("INSERT INTO t VALUES (1) RETURNING *", &[Insert][..]),
            ("INSERT OR IGNORE INTO t VALUES (1)", &[Insert]),
            ("INSERT OR REPLACE INTO t VALUES (1)", &[Insert, Delete]),
            ("REPLACE INTO t VALUES (1)", &[Insert, Delete]),
            ("INSERT INTO t VALUES (1) ON CONFLICT DO NOTHING", &[Insert]),
            (
                "INSERT INTO t VALUES (1) ON CONFLICT (a) DO UPDATE SET b = excluded.b",
                &[Insert, Update],
            ),
            ("UPDATE OR IGNORE t SET a = 1", &[Update]),
            ("UPDATE OR REPLACE t SET a = 1", &[Update, Delete]),
            ("UPDATE t SET a = u.a FROM u WHERE t.id = u.id", &[Update]),
            ("DELETE FROM t WHERE a = 1 RETURNING a", &[Delete]),
            (
                "CREATE TABLE t (a INTEGER PRIMARY KEY) STRICT",
                &[CreateTable],
            ),
            ("CREATE VIRTUAL TABLE f USING fts5(a)", &[CreateTable]),
            ("ALTER TABLE t RENAME TO u", &[AlterTable]),
            ("CREATE INDEX i ON t (a)", &[AlterTable]),
            ("DROP TABLE t", &[DropTable]),
            ("CREATE TEMP VIEW v AS SELECT 1", &[CreateView]),
        ] {
            assert_eq!(sqlite(sql), writes(kinds), "query: {sql}");
        }
    }

    #[test]
    fn a_table_of_the_schema_temp_is_a_temporary_table() {
        use WriteKind::*;
        for (sql, kind) in [
            ("CREATE TEMP TABLE x (a)", CreateTemporaryTable),
            ("CREATE TEMPORARY TABLE x (a)", CreateTemporaryTable),
            ("CREATE TABLE temp.x (a)", CreateTemporaryTable),
            ("CREATE TABLE TEMP.x (a)", CreateTemporaryTable),
            (
                "CREATE VIRTUAL TABLE temp.f USING fts5(a)",
                CreateTemporaryTable,
            ),
            ("ALTER TABLE temp.x ADD COLUMN b", AlterTemporaryTable),
            ("CREATE INDEX i ON temp.x (a)", AlterTemporaryTable),
            ("DROP TABLE temp.x", DropTemporaryTable),
            // A table `temp` of the main schema is no temporary table.
            ("CREATE TABLE main.temp (a)", CreateTable),
        ] {
            assert_eq!(sqlite(sql), writes(&[kind]), "query: {sql}");
        }
        // `temp` is a database like any other in MySQL.
        assert_eq!(kind("CREATE TABLE temp.x (a INT)"), writes(&[CreateTable]));
    }

    #[test]
    fn classifies_the_operations_on_an_index_written_by_the_navigator() {
        use WriteKind::*;
        for (sql, kind) in [
            ("CREATE INDEX \"other\".i ON \"t\" (a)", AlterTable),
            ("CREATE UNIQUE INDEX other.i ON t (a, b)", AlterTable),
            ("DROP INDEX \"other\".\"i\"", AlterTable),
            ("CREATE INDEX temp.i ON x (a)", AlterTemporaryTable),
            ("DROP INDEX temp.i", AlterTemporaryTable),
        ] {
            assert_eq!(sqlite(sql), writes(&[kind]), "query: {sql}");
        }
        assert_eq!(sqlite("REINDEX \"other\".i"), writes(&[Maintenance]));
    }

    #[test]
    fn classifies_the_pragmas_by_their_role() {
        for sql in [
            "PRAGMA journal_mode",
            "PRAGMA main.user_version",
            "PRAGMA table_info(t)",
            "PRAGMA table_info = t",
            "PRAGMA aux.index_list('t')",
            "PRAGMA integrity_check",
            "PRAGMA integrity_check(10)",
            "PRAGMA foreign_keys",
            "SELECT * FROM pragma_table_info('t')",
            "SELECT name FROM pragma_table_list WHERE schema = 'main'",
        ] {
            assert_eq!(sqlite(sql), StatementKind::Read, "query: {sql}");
        }
        for sql in [
            "PRAGMA foreign_keys = ON",
            "PRAGMA foreign_keys(1)",
            "PRAGMA cache_size = -2000",
            "PRAGMA temp_store = MEMORY",
            "PRAGMA busy_timeout = 5000; SELECT 1",
        ] {
            assert_eq!(sqlite(sql), StatementKind::Setting, "query: {sql}");
        }
        for sql in [
            "PRAGMA optimize",
            "PRAGMA optimize(0x10002)",
            "PRAGMA wal_checkpoint(TRUNCATE)",
            "PRAGMA aux.incremental_vacuum(10)",
            "SELECT * FROM pragma_optimize",
            "SELECT * FROM main.PRAGMA_OPTIMIZE",
            "VACUUM",
            "VACUUM aux",
            "REINDEX",
            "REINDEX main.i",
            "ANALYZE",
            "ANALYZE main.t",
        ] {
            assert_eq!(
                sqlite(sql),
                writes(&[WriteKind::Maintenance]),
                "query: {sql}"
            );
        }
        // A write that reads the function of a pragma that writes needs both.
        assert_eq!(
            sqlite("INSERT INTO log SELECT * FROM pragma_optimize"),
            writes(&[WriteKind::Insert, WriteKind::Maintenance])
        );
        for sql in [
            // A setting of the file or of the library.
            "PRAGMA journal_mode = WAL",
            "PRAGMA journal_mode(WAL)",
            "PRAGMA user_version = 3",
            "PRAGMA writable_schema = ON",
            "PRAGMA query_only = OFF",
            // A pragma Prisme does not know, which SQLite would run silently.
            "PRAGMA no_such_pragma",
            "PRAGMA no_such_pragma = 1",
        ] {
            assert_eq!(sqlite(sql), StatementKind::Forbidden, "query: {sql}");
        }
        // A table of MySQL may bear the name of a pragma function.
        assert_eq!(kind("SELECT * FROM pragma_optimize"), StatementKind::Read);
    }

    #[test]
    fn attaches_only_files_named_by_a_literal_absolute_path_or_uri() {
        #[cfg(windows)]
        let paths = [
            "D:\\data\\sales.db",
            "D:/data/sales.db",
            "\\\\server\\share\\s.db",
        ];
        #[cfg(not(windows))]
        let paths = ["/srv/data/sales.db", "/tmp/s.db", "/s.db"];
        let uri = crate::sqlite_files::file_uri(paths[0], false);
        for path in paths.iter().chain(&[":memory:", uri.as_str()]) {
            for sql in [
                format!("ATTACH '{path}' AS sales"),
                format!("ATTACH DATABASE '{path}' AS sales"),
            ] {
                assert_eq!(sqlite(&sql), writes(&[WriteKind::Attach]), "query: {sql}");
            }
        }
        for sql in ["DETACH sales", "DETACH DATABASE sales"] {
            assert_eq!(sqlite(sql), writes(&[WriteKind::Attach]), "query: {sql}");
        }
        for sql in [
            "ATTACH 'sales.db' AS sales",
            "ATTACH '' AS sales",
            "ATTACH 'file:sales.db?mode=rwc' AS sales",
            "ATTACH 'file:///D:/data/sales.db' AS sales",
            "ATTACH 'file:/prisme/session-1/main?vfs=memdb' AS sales",
            "ATTACH '/srv/' || 'sales.db' AS sales",
            "ATTACH sales AS sales",
            "ATTACH (SELECT path FROM files) AS sales",
        ] {
            assert_eq!(sqlite(sql), StatementKind::Forbidden, "query: {sql}");
        }
    }

    #[test]
    fn forbids_the_writes_of_sqlite_without_a_switch() {
        for sql in [
            "VACUUM INTO 'D:/copy.db'",
            "VACUUM main INTO '/tmp/copy.db'",
            "CREATE TRIGGER tr AFTER INSERT ON t BEGIN DELETE FROM u; END",
            "CREATE TEMP TRIGGER tr AFTER INSERT ON t BEGIN SELECT 1; END",
            "DROP TRIGGER tr",
        ] {
            assert_eq!(sqlite(sql), StatementKind::Forbidden, "query: {sql}");
        }
    }

    #[test]
    fn allows_the_control_of_the_transactions_of_sqlite() {
        for sql in [
            "BEGIN",
            "BEGIN DEFERRED TRANSACTION",
            "BEGIN IMMEDIATE",
            "BEGIN EXCLUSIVE",
            "COMMIT",
            "END",
            "END TRANSACTION",
            "ROLLBACK",
            "ROLLBACK TO s",
            "ROLLBACK TRANSACTION TO SAVEPOINT s",
            "SAVEPOINT s",
            "RELEASE s",
            "RELEASE SAVEPOINT s",
        ] {
            assert_eq!(sqlite(sql), StatementKind::Transaction, "query: {sql}");
        }
        let ends = |sql| ends_transaction(sql, Dialect::Sqlite);
        assert!(ends("COMMIT"));
        assert!(ends("END TRANSACTION"));
        assert!(ends("ROLLBACK"));
        assert!(!ends("ROLLBACK TO s"));
        assert!(!ends("BEGIN IMMEDIATE"));
    }

    #[test]
    fn allows_only_the_settings_of_the_connection_at_its_opening() {
        let refusal = |sql: &str| session_setting_refusal(sql, Dialect::Sqlite);
        for pragma in sqlite_pragmas::SESSION_PRAGMAS {
            assert_eq!(refusal(&format!("PRAGMA {pragma} = 1")), None, "{pragma}");
        }
        assert_eq!(
            refusal("PRAGMA foreign_keys = ON; PRAGMA temp_store(MEMORY)"),
            None
        );
        for sql in [
            "PRAGMA foreign_keys",
            "PRAGMA journal_mode = WAL",
            "PRAGMA optimize",
            "PRAGMA no_such_pragma = 1",
            "SELECT 1",
            "ATTACH ':memory:' AS m",
        ] {
            assert_eq!(
                refusal(sql),
                Some(SettingRefusal::NotASetting),
                "query: {sql}"
            );
        }
        assert_eq!(refusal("PRAGMA a ="), Some(SettingRefusal::NotUnderstood));
    }
}
