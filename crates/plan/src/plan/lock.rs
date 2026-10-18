//! Locking reads: the rows of a query block read with locks on the rows of
//! its tables, held until the end of the transaction (`FOR UPDATE`, `FOR
//! SHARE`, `LOCK IN SHARE MODE` of the MySQL family).
//!
//! A locking clause reaches the tables of one query block, and neither those
//! of its subqueries nor those of the other operands of a set operation:
//! written after a set operation, it locks the tables of its last operand
//! (`docs/DEV-DIALECTS.md`, section 6.3). The node stands above the block it
//! locks.

use crate::{Ident, Plan, Span, TableName};

/// Rows of a query block read with locks.
#[derive(Debug, Clone, PartialEq)]
pub struct Lock {
    /// Block locked: a `SELECT`, a `TABLE`, a `VALUES`.
    pub input: Box<Plan>,
    /// Locking clauses, in the order written, at least one.
    pub clauses: Vec<LockClause>,
    /// Written after the parentheses around the block, and their `ORDER BY`
    /// and `LIMIT` (`(SELECT a FROM t LIMIT 2) LIMIT 1 FOR UPDATE`); at the
    /// end of the block otherwise.
    pub outside: bool,
}

/// Locking clause: the lock taken, on which tables, and what to do with a
/// row that another transaction locks.
#[derive(Debug, Clone, PartialEq)]
pub struct LockClause {
    /// Lock taken on the rows read.
    pub strength: LockStrength,
    /// Tables locked (`OF`), each one by its alias or, without alias, by its
    /// name; every table of the block when empty.
    pub tables: Vec<TableName>,
    /// What the read does with a row locked by another transaction.
    pub wait: LockWait,
    /// Form written.
    pub form: LockForm,
    /// Position in the input.
    pub span: Option<Span>,
}

/// Lock taken on the rows read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockStrength {
    /// Exclusive lock: `FOR UPDATE`.
    Update,
    /// Shared lock: `FOR SHARE`, `LOCK IN SHARE MODE`.
    Share,
}

/// Form written for a locking clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LockForm {
    /// `FOR UPDATE`, `FOR SHARE`.
    For,
    /// `LOCK IN SHARE MODE` (MySQL family).
    ShareMode,
}

/// What a locking read does with a row locked by another transaction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LockWait {
    /// Waits for the row, up to the timeout of the session.
    Wait,
    /// Fails at once (`NOWAIT`).
    NoWait,
    /// Leaves the row out of the result (`SKIP LOCKED`).
    SkipLocked,
    /// Waits for the row up to a number of seconds (`WAIT n`, MariaDB), the
    /// number as written.
    Seconds(String),
}

/// Table of the `FROM` of a query block, as a locking clause names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockTable<'p> {
    /// The alias, or the name of the table without alias.
    pub name: &'p Ident,
    /// Database written before the table, without alias.
    pub database: Option<&'p Ident>,
    /// What the name designates.
    pub kind: BlockTableKind,
}

/// What a table of the `FROM` of a query block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockTableKind {
    /// A table of the database, whose rows a locking clause locks.
    Table,
    /// A common table expression, whose tables no locking clause of the
    /// block reaches.
    Cte,
    /// A derived table, which no locking clause names (MySQL, error 3569).
    Derived,
}

impl BlockTable<'_> {
    /// Tells whether `name`, written after `OF`, designates this table: its
    /// alias, or its name without alias, the case as written (MySQL refuses
    /// `OF T` for `t`, error 3568, on a server with case-sensitive names). A
    /// database written for a table written without it is the current one,
    /// which the server checks.
    #[must_use]
    pub fn is_named(&self, name: &TableName) -> bool {
        self.name.value == name.name.value
            && (name.database.as_ref()).is_none_or(|database| {
                (self.database).is_none_or(|own| own.value == database.value)
            })
    }
}

impl Lock {
    /// Tables of the block locked, in the order of its `FROM`.
    #[must_use]
    pub fn tables(&self) -> Vec<BlockTable<'_>> {
        block_tables(&self.input)
    }
}

/// Tables of the `FROM` of the query block `plan`
/// (`Limit(Distinct(Project(...)))`, a `TABLE`), in order: a table, a
/// common table expression or a derived table each, those of the joins
/// included; none for a `VALUES`.
#[must_use]
pub fn block_tables(plan: &Plan) -> Vec<BlockTable<'_>> {
    let mut tables = Vec::new();
    source_tables(block_source(plan), &mut tables);
    tables
}

/// Source of the block `plan`: its `FROM`, or the table of a `TABLE`.
fn block_source(plan: &Plan) -> &Plan {
    match plan {
        Plan::Limit(limit) => block_source(&limit.input),
        Plan::Distinct(distinct) => block_source(&distinct.input),
        Plan::Project(project) => block_source(&project.input),
        Plan::Sort(sort) => block_source(&sort.input),
        Plan::Window(window) => block_source(&window.input),
        Plan::Filter(filter) => block_source(&filter.input),
        Plan::Aggregate(aggregate) => block_source(&aggregate.input),
        Plan::Extend(extend) => block_source(&extend.input),
        other => other,
    }
}

/// Tables of the source `plan`, added to `tables`.
fn source_tables<'p>(plan: &'p Plan, tables: &mut Vec<BlockTable<'p>>) {
    match plan {
        Plan::Join(join) => {
            source_tables(&join.left, tables);
            source_tables(&join.right, tables);
        }
        Plan::Scan(scan) => tables.push(match &scan.alias {
            Some(alias) => BlockTable {
                name: &alias.name,
                database: None,
                kind: BlockTableKind::Table,
            },
            None => BlockTable {
                name: &scan.table.name,
                database: scan.table.database.as_ref(),
                kind: BlockTableKind::Table,
            },
        }),
        Plan::CteRef(cte_ref) => tables.push(BlockTable {
            name: cte_ref
                .alias
                .as_ref()
                .map_or(&cte_ref.name, |alias| &alias.name),
            database: None,
            kind: BlockTableKind::Cte,
        }),
        Plan::Derived(derived) => tables.push(BlockTable {
            name: &derived.alias.name,
            database: None,
            kind: BlockTableKind::Derived,
        }),
        _ => {}
    }
}
