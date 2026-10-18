//! Prisme's SQL dialects: analysis and rewriting of queries per dialect.
//!
//! Relies on `sqlparser` (see `docs/V1.md`, section 3.2). For now: the
//! default row limit (`limit` module), the read / write classification
//! (`classify` module), the beautifier (`beautify` module), the reference of
//! each dialect (`reference` module, `reference/*.yaml` files), read by the
//! keywords (`keywords` module), the registry of the versions (`versions`
//! module) and the built-in functions (`builtins` module), and the position
//! of syntax errors (`syntax` module). The transpilation is being written
//! (`docs/DEV-DIALECTS.md`): lexer of the MySQL family (`lexer` module),
//! syntax tree and parser (`sql`), plan builder (`build`), emitter (`emit`),
//! report (`report`) and entry point (`transpile`), with the dates and times
//! of the literals (`temporal`) and the patterns of `REGEXP` (`regexp`).

mod alias;
pub mod analysis;
pub mod beautify;
pub mod build;
mod builtins;
mod charsets;
pub mod classify;
pub mod emit;
mod functions;
pub mod keywords;
mod layout;
pub mod lexer;
pub mod limit;
mod offsets;
mod parse;
mod reference;
mod regexp;
pub mod report;
mod select_options;
pub mod sql;
pub mod syntax;
mod temporal;
pub mod transpile;
pub mod versions;
mod whitespace;

pub use builtins::{ListedFunction, listed_functions};

/// SQL dialect of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// MySQL.
    MySql,
    /// MariaDB. `sqlparser` has no MariaDB dialect: the MySQL one serves as a base.
    MariaDb,
}

impl Dialect {
    /// Name of the dialect in the messages.
    pub const fn label(self) -> &'static str {
        match self {
            Self::MySql => "MySQL",
            Self::MariaDb => "MariaDB",
        }
    }

    /// `sqlparser` dialect used to parse a query of this dialect.
    fn parser_dialect(self) -> Box<dyn sqlparser::dialect::Dialect> {
        match self {
            Self::MySql | Self::MariaDb => Box::new(sqlparser::dialect::MySqlDialect {}),
        }
    }
}
