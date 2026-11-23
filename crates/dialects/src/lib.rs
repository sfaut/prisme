//! Prisme's SQL dialects: analysis and rewriting of queries per dialect.
//!
//! Relies on `sqlparser` (see `docs/V1.md`, section 3.2), with a MySQL
//! dialect of its own for the compound time units (`mysql_parser` module).
//! For now: the default row limit (`limit` module), the read / write
//! classification (`classify` module) and the verb it gives a statement
//! (`verb` module), the beautifier (`beautify` module),
//! the keywords of each dialect (`keywords` module, `keywords/*.yaml` files)
//! and the position of syntax errors (`syntax` module). The function catalog and the emitters
//! per dialect will come with the translation.
//!
//! Each dialect is described once (`DialectSpec`): the lexical rules of its
//! texts, read by the splitter and by the editor, the `sqlparser` dialect
//! that parses it, and its keyword file.

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
pub mod sqlite_files;
mod sqlite_pragmas;
mod sqlite_statements;
pub mod syntax;
pub mod temporary;
pub mod verb;
mod whitespace;
mod words;

use prisme_splitter::LexicalRules;

/// SQL dialect of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// MySQL.
    MySql,
    /// MariaDB. `sqlparser` has no MariaDB dialect: the MySQL one serves as a base.
    ///
    /// Workaround: the missing MariaDB dialect of `sqlparser` (`docs/DEVELOPER.md`, section "Workarounds").
    MariaDb,
    /// SQLite.
    Sqlite,
}

/// What defines a dialect: how its texts are read and parsed.
pub struct DialectSpec {
    /// Lexical rules of its texts, read by the splitter and by the editor.
    pub lexical: LexicalRules,
    /// `sqlparser` dialect that parses its queries.
    parser: fn() -> Box<dyn sqlparser::dialect::Dialect>,
    /// Its keyword file (`keywords/<dialect>.yaml`).
    keywords: &'static str,
    /// Statements Prisme parses itself, which `sqlparser` misses.
    statements: Statements,
    /// Does a `LIMIT` follow a `VALUES` (MySQL, MariaDB)? SQLite refuses it:
    /// its `LIMIT` belongs to a `SELECT`.
    values_take_limit: bool,
    /// Schema of the temporary tables, whose name says they are temporary
    /// (`temp.t` in SQLite); `None` where a schema is a database like any
    /// other.
    temporary_schema: Option<&'static str>,
}

/// Statements of a dialect that Prisme parses itself (`parse.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Statements {
    /// Routines, events, index operations, transactions of MySQL and MariaDB
    /// (`mysql_statements.rs`).
    MySql,
    /// `PRAGMA`, `DETACH`, `VACUUM`, `REINDEX` of SQLite (`sqlite_statements.rs`).
    Sqlite,
}

/// Lexical rules of MySQL and MariaDB: `#` comments, `--` followed by a
/// space, backslash escapes, double quotes for strings (without the
/// `ANSI_QUOTES` mode), backticks for names, `DELIMITER` command of the
/// `mysql` client.
const MYSQL_LEXICAL: LexicalRules = LexicalRules {
    hash_comments: true,
    space_after_dashes: true,
    backslash_escapes: true,
    double_quoted_strings: true,
    identifier_quotes: "`",
    bracket_identifiers: false,
    delimiter_command: true,
    trigger_bodies: false,
};

/// MySQL.
static MYSQL: DialectSpec = DialectSpec {
    lexical: MYSQL_LEXICAL,
    parser: || Box::new(mysql_parser::MySqlParserDialect::default()),
    keywords: include_str!("../keywords/mysql.yaml"),
    statements: Statements::MySql,
    values_take_limit: true,
    temporary_schema: None,
};

/// MariaDB, read as MySQL with keywords of its own.
static MARIADB: DialectSpec = DialectSpec {
    lexical: MYSQL_LEXICAL,
    parser: || Box::new(mysql_parser::MySqlParserDialect::default()),
    keywords: include_str!("../keywords/mariadb.yaml"),
    statements: Statements::MySql,
    values_take_limit: true,
    temporary_schema: None,
};

/// SQLite, as measured on the library Prisme ships (`docs/SQL-COMPARISON.md`,
/// section 6): `--` without a space, no backslash escape, names in double
/// quotes, backticks or square brackets, bodies of the triggers.
static SQLITE: DialectSpec = DialectSpec {
    lexical: LexicalRules {
        hash_comments: false,
        space_after_dashes: false,
        backslash_escapes: false,
        double_quoted_strings: false,
        identifier_quotes: "\"`",
        bracket_identifiers: true,
        delimiter_command: false,
        trigger_bodies: true,
    },
    parser: || Box::new(sqlparser::dialect::SQLiteDialect {}),
    keywords: include_str!("../keywords/sqlite.yaml"),
    statements: Statements::Sqlite,
    values_take_limit: false,
    temporary_schema: Some("temp"),
};

impl Dialect {
    /// Every dialect.
    pub const ALL: [Self; 3] = [Self::MySql, Self::MariaDb, Self::Sqlite];

    /// Description of the dialect.
    pub fn spec(self) -> &'static DialectSpec {
        match self {
            Self::MySql => &MYSQL,
            Self::MariaDb => &MARIADB,
            Self::Sqlite => &SQLITE,
        }
    }

    /// Lexical rules of the texts of the dialect.
    pub fn lexical_rules(self) -> &'static LexicalRules {
        &self.spec().lexical
    }

    /// `sqlparser` dialect used to parse a query of this dialect.
    fn parser_dialect(self) -> Box<dyn sqlparser::dialect::Dialect> {
        (self.spec().parser)()
    }

    /// Position of the dialect in `ALL`, for the tables kept per dialect.
    fn index(self) -> usize {
        self as usize
    }
}
