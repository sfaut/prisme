//! Prisme's SQL dialects: analysis and rewriting of queries per dialect.
//!
//! Relies on `sqlparser` (see `docs/V1.md`, section 3.2), with a MySQL
//! dialect of its own for the compound time units (`mysql_parser` module).
//! For now: the default row limit (`limit` module), the read / write
//! classification (`classify` module), the beautifier (`beautify` module),
//! the keywords of each dialect (`keywords` module, `keywords/*.yaml` files)
//! and the position of syntax errors (`syntax` module). The function catalog and the emitters
//! per dialect will come with the translation.

mod alias;
pub mod analysis;
pub mod beautify;
pub mod classify;
mod functions;
pub mod keywords;
mod layout;
pub mod limit;
mod mysql_parser;
mod mysql_statements;
mod offsets;
mod parse;
pub mod syntax;
mod whitespace;

/// SQL dialect of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// MySQL.
    MySql,
    /// MariaDB. `sqlparser` has no MariaDB dialect: the MySQL one serves as a base.
    MariaDb,
}

impl Dialect {
    /// `sqlparser` dialect used to parse a query of this dialect.
    fn parser_dialect(self) -> Box<dyn sqlparser::dialect::Dialect> {
        match self {
            Self::MySql | Self::MariaDb => Box::new(mysql_parser::MySqlParserDialect::default()),
        }
    }
}
