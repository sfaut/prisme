//! Prisme's SQL dialects: analysis and rewriting of queries per dialect.
//!
//! Relies on `sqlparser` (see `docs/V1.md`, section 3.2). For now: the
//! default row limit (`limit` module), the read / write classification
//! (`classify` module), the beautifier (`beautify` module), the keywords of
//! each dialect (`keywords` module, `keywords/*.yaml` files), the position
//! of syntax errors (`syntax` module) and the registry of the versions of each
//! dialect (`versions` module, `versions/*.yaml` files). The transpilation
//! is being written (`docs/DEV-DIALECTS.md`): lexer of the MySQL family
//! (`lexer` module), syntax tree and parser (`sql`), plan builder (`build`),
//! emitter (`emit`), report (`report`) and entry point (`transpile`); the
//! function catalog comes next.

mod alias;
pub mod analysis;
pub mod beautify;
pub mod build;
pub mod classify;
pub mod emit;
mod functions;
pub mod keywords;
mod layout;
pub mod lexer;
pub mod limit;
mod offsets;
mod parse;
pub mod report;
pub mod sql;
pub mod syntax;
pub mod transpile;
pub mod versions;
mod whitespace;

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
