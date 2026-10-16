//! Analysis of a query: the role of each word, from `sqlparser`'s syntax
//! tree, the connection's catalog and the server version.
//!
//! It is the step before beautifying (and, later, the name resolution of the
//! federated engine): the query is understood before being rewritten,
//! instead of being processed word by word from lists.
//!
//! - **Names**: any identifier of the tree (database, table, column, alias,
//!   function...) is a name, never a keyword, even if it looks like a keyword
//!   (`cast`, `date`, `status`). It takes the catalog's spelling when the
//!   catalog knows it unambiguously; an alias takes the spelling of its
//!   declaration. An unknown or ambiguous name (`Clients` and `clients`), or
//!   one between backticks, keeps the typed spelling.
//! - **Keywords**: any other word that `sqlparser` recognises as a keyword
//!   (`SELECT`, `CAST`, `INTERVAL ... DAY`...). A keyword that the server
//!   version does not know (`keywords/*.yaml` files, `since` / `until`
//!   fields) is reported separately.
//!
//! Identifiers are collected in the serialised tree (`serde` option of
//! `sqlparser`), where each one carries its position: none escapes the
//! collection, wherever it is in the query. Their nature (table, column...)
//! comes from a walk of the tree; a name of unknown nature keeps its
//! spelling.

use std::collections::HashMap;
use std::ops::ControlFlow;

use serde_json::Value;
use sqlparser::ast::{
    Expr, ObjectName, ObjectNamePart, Query, SelectItem, SetExpr, TableFactor, Visit, Visitor,
};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};

use crate::Dialect;
use crate::keywords::is_known_in_version;
use crate::offsets::OffsetIndex;

/// Names of the connection's catalog, as defined (with their case).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogNames {
    /// Databases.
    pub schemas: Vec<String>,
    /// Tables and views, all databases together.
    pub tables: Vec<String>,
    /// Columns, all tables together.
    pub columns: Vec<String>,
    /// Stored functions and procedures.
    pub routines: Vec<String>,
}

/// Role of a word of the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// Keyword of the language, known to the server version.
    Keyword,
    /// Keyword for the parser, but unknown to the server version.
    UnknownKeyword,
    /// Name (database, table, column, alias, function...), with the spelling to give it.
    Name {
        /// Spelling of the name: the catalog's, the alias declaration's, or
        /// the typed one.
        spelling: String,
    },
}

/// A word of the query and its role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// Start of the word in the text, in bytes.
    pub start: usize,
    /// End of the word in the text, in bytes.
    pub end: usize,
    /// Role of the word.
    pub role: Role,
}

/// Nature of a name, from its place in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Database, before a table or column name.
    Schema,
    /// Table or view, or table alias used before a column.
    Table,
    /// Column, or column alias used (`ORDER BY total`).
    Column,
    /// Declaration of a table alias (`FROM t AS a`, name of a CTE).
    TableAlias,
    /// Declaration of a column alias (`SELECT x AS total`).
    ColumnAlias,
    /// Function or procedure (its spelling is handled by the `functions` module).
    Function,
}

/// Position of an identifier in the text: line and column of its start.
type Position = (u64, u64);

/// Analyses a text (one or more queries). `None` if `sqlparser` does not
/// understand it: the caller then falls back on word-by-word processing.
///
/// `version` is the one announced by the server (`10.11.11-MariaDB`);
/// without a version, all the dialect's keywords are known.
pub fn analyze(
    sql: &str,
    dialect: Dialect,
    version: Option<&str>,
    catalog: &CatalogNames,
) -> Option<Vec<Word>> {
    let parser_dialect = dialect.parser_dialect();
    let statements = Parser::parse_sql(parser_dialect.as_ref(), sql).ok()?;

    let identifiers = identifier_positions(&serde_json::to_value(&statements).ok()?);
    let mut kinds = KindCollector::default();
    let _ = statements.visit(&mut kinds);
    let names = Names::new(catalog, &kinds);

    let tokens = Tokenizer::new(parser_dialect.as_ref(), sql)
        .tokenize_with_location()
        .ok()?;
    let index = OffsetIndex::new(sql);
    let mut words = Vec::new();
    for token in tokens {
        let Token::Word(word) = &token.token else {
            continue;
        };
        if word.quote_style.is_some() {
            continue;
        }
        let position = (token.span.start.line, token.span.start.column);
        let role = if identifiers.contains_key(&position) {
            let kind = kinds.kinds.get(&position).copied();
            Role::Name {
                spelling: names.spelling(kind, &word.value),
            }
        } else if word.keyword != sqlparser::keywords::Keyword::NoKeyword {
            if is_known_in_version(&word.value, dialect, version) {
                Role::Keyword
            } else {
                Role::UnknownKeyword
            }
        } else {
            continue;
        };
        let (Some(start), Some(end)) =
            (index.offset(token.span.start), index.offset(token.span.end))
        else {
            return None;
        };
        words.push(Word { start, end, role });
    }
    Some(words)
}

/// Positions of all the identifiers of the serialised tree: an identifier is
/// an object `{ value, quote_style, span }` there.
fn identifier_positions(tree: &Value) -> HashMap<Position, bool> {
    let mut positions = HashMap::new();
    collect_identifiers(tree, &mut positions);
    positions
}

/// Walks the serialised tree and records the position of each identifier,
/// with a "quoted" flag.
fn collect_identifiers(node: &Value, positions: &mut HashMap<Position, bool>) {
    match node {
        Value::Object(fields) => {
            let is_identifier = fields.len() == 3
                && fields.get("value").is_some_and(Value::is_string)
                && fields.contains_key("quote_style")
                && fields.contains_key("span");
            if is_identifier {
                let start = &fields["span"]["start"];
                if let (Some(line), Some(column)) =
                    (start["line"].as_u64(), start["column"].as_u64())
                    && line > 0
                {
                    positions.insert((line, column), !fields["quote_style"].is_null());
                }
            }
            fields
                .values()
                .for_each(|value| collect_identifiers(value, positions));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_identifiers(item, positions)),
        _ => {}
    }
}

/// Walk of the tree that records the nature of each name, and the declared aliases.
#[derive(Default)]
struct KindCollector {
    /// Nature of each name, by position.
    kinds: HashMap<Position, Kind>,
    /// Declared table aliases, with their spelling.
    table_aliases: Vec<String>,
    /// Declared column aliases, with their spelling.
    column_aliases: Vec<String>,
}

impl KindCollector {
    /// Records the nature of an identifier.
    fn note(&mut self, ident: &sqlparser::ast::Ident, kind: Kind) {
        let start = ident.span.start;
        if start.line > 0 {
            self.kinds.insert((start.line, start.column), kind);
        }
    }

    /// Records the parts of a qualified name: the last one has the given
    /// nature, those before it are, from right to left, `qualifiers`.
    fn note_parts(&mut self, parts: &[&sqlparser::ast::Ident], last: Kind, qualifiers: &[Kind]) {
        let mut remaining = parts.iter().rev();
        if let Some(ident) = remaining.next() {
            self.note(ident, last);
        }
        for (ident, kind) in remaining.zip(qualifiers) {
            self.note(ident, *kind);
        }
    }

    /// Records the column aliases of a query body (`SELECT x AS total`), set
    /// operations included.
    fn note_projection(&mut self, body: &SetExpr) {
        match body {
            SetExpr::Select(select) => {
                for item in &select.projection {
                    if let SelectItem::ExprWithAlias { alias, .. } = item {
                        self.note(alias, Kind::ColumnAlias);
                        self.column_aliases.push(alias.value.clone());
                    }
                }
            }
            SetExpr::SetOperation { left, right, .. } => {
                self.note_projection(left);
                self.note_projection(right);
            }
            SetExpr::Query(query) => self.note_projection(&query.body),
            _ => {}
        }
    }
}

/// Identifiers of an object name (`database.table`), without computed parts.
fn object_idents(name: &ObjectName) -> Vec<&sqlparser::ast::Ident> {
    name.0.iter().filter_map(ObjectNamePart::as_ident).collect()
}

impl Visitor for KindCollector {
    type Break = ();

    fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.note(&cte.alias.name, Kind::TableAlias);
                self.table_aliases.push(cte.alias.name.value.clone());
            }
        }
        self.note_projection(&query.body);
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        self.note_parts(&object_idents(relation), Kind::Table, &[Kind::Schema]);
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        if let TableFactor::Table {
            alias: Some(alias), ..
        }
        | TableFactor::Derived {
            alias: Some(alias), ..
        } = factor
        {
            self.note(&alias.name, Kind::TableAlias);
            self.table_aliases.push(alias.name.value.clone());
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::Identifier(ident) => self.note(ident, Kind::Column),
            Expr::CompoundIdentifier(parts) => {
                let parts: Vec<_> = parts.iter().collect();
                self.note_parts(&parts, Kind::Column, &[Kind::Table, Kind::Schema]);
            }
            Expr::Function(function) => {
                self.note_parts(
                    &object_idents(&function.name),
                    Kind::Function,
                    &[Kind::Schema],
                );
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

/// Spelling of names: catalog and declared aliases, compared
/// case-insensitively.
struct Names<'a> {
    /// Names of the catalog.
    catalog: &'a CatalogNames,
    /// Table aliases declared in the query.
    table_aliases: Vec<String>,
    /// Column aliases declared in the query.
    column_aliases: Vec<String>,
}

impl<'a> Names<'a> {
    /// Prepares the spelling of the names of a query.
    fn new(catalog: &'a CatalogNames, kinds: &KindCollector) -> Self {
        Self {
            catalog,
            table_aliases: kinds.table_aliases.clone(),
            column_aliases: kinds.column_aliases.clone(),
        }
    }

    /// Spelling of a typed name `typed`, of nature `kind` (unknown: `None`).
    fn spelling(&self, kind: Option<Kind>, typed: &str) -> String {
        let found =
            match kind {
                Some(Kind::Schema) => unique(&self.catalog.schemas, typed),
                Some(Kind::Table) => unique(&self.table_aliases, typed)
                    .or_else(|| unique(&self.catalog.tables, typed)),
                Some(Kind::Column) => unique(&self.column_aliases, typed)
                    .or_else(|| unique(&self.catalog.columns, typed)),
                Some(Kind::TableAlias | Kind::ColumnAlias | Kind::Function) | None => None,
            };
        found.unwrap_or_else(|| typed.to_string())
    }
}

/// The spelling of a name in `names`, if it appears there under a single
/// spelling (case-insensitively); `None` if it does not appear, or under
/// several spellings (`Clients` and `clients`).
fn unique(names: &[String], typed: &str) -> Option<String> {
    let mut matches = names.iter().filter(|name| name.eq_ignore_ascii_case(typed));
    let first = matches.next()?;
    matches.all(|other| other == first).then(|| first.clone())
}

#[cfg(test)]
mod tests {
    use sqlparser::tokenizer::Location;

    use super::*;

    /// Line and column of a location.
    fn position_of(location: Location) -> Position {
        (location.line, location.column)
    }

    /// Test catalog: one database, two tables, their columns, one function.
    fn catalog() -> CatalogNames {
        CatalogNames {
            schemas: vec!["Accounts".to_string()],
            tables: vec![
                "operations".to_string(),
                "Clients".to_string(),
                "clients".to_string(),
            ],
            columns: vec![
                "operation_id".to_string(),
                "cast".to_string(),
                "Amount".to_string(),
            ],
            routines: vec!["TotalTax".to_string()],
        }
    }

    /// Rewrites the text from the analysis: keywords in capitals, names in
    /// their spelling, keywords unknown to the version between brackets.
    fn rewrite(sql: &str, dialect: Dialect, version: Option<&str>) -> String {
        let words = analyze(sql, dialect, version, &catalog()).unwrap();
        let mut result = String::new();
        let mut last = 0;
        for word in words {
            result.push_str(&sql[last..word.start]);
            let text = &sql[word.start..word.end];
            match word.role {
                Role::Keyword => result.push_str(&text.to_ascii_uppercase()),
                Role::UnknownKeyword => result.push_str(&format!("[{text}]")),
                Role::Name { spelling } => result.push_str(&spelling),
            }
            last = word.end;
        }
        result.push_str(&sql[last..]);
        result
    }

    #[test]
    fn tells_keywords_from_names_that_look_like_keywords() {
        assert_eq!(
            rewrite(
                "select cast(cast as char), date '2026-10-14' + interval 1 day from t",
                Dialect::MySql,
                None
            ),
            "SELECT CAST(cast AS CHAR), DATE '2026-10-14' + INTERVAL 1 DAY FROM t"
        );
    }

    #[test]
    fn spells_names_as_the_catalog_does() {
        assert_eq!(
            rewrite(
                "select OPERATION_ID, amount from ACCOUNTS.Operations where operation_id > 0",
                Dialect::MySql,
                None
            ),
            "SELECT operation_id, Amount FROM Accounts.operations WHERE operation_id > 0"
        );
    }

    #[test]
    fn keeps_unknown_ambiguous_and_quoted_names_as_typed() {
        assert_eq!(
            rewrite(
                "select x from CLIENTS join `OPERATIONS` join Unknown",
                Dialect::MySql,
                None
            ),
            "SELECT x FROM CLIENTS JOIN `OPERATIONS` JOIN Unknown"
        );
    }

    #[test]
    fn spells_aliases_as_declared() {
        assert_eq!(
            rewrite(
                "select O.operation_id total from operations Op_1 order by TOTAL",
                Dialect::MySql,
                None
            ),
            "SELECT O.operation_id total FROM operations Op_1 ORDER BY total"
        );
        assert_eq!(
            rewrite(
                "select OP_1.operation_id from operations Op_1",
                Dialect::MySql,
                None
            ),
            "SELECT Op_1.operation_id FROM operations Op_1"
        );
    }

    #[test]
    fn flags_keywords_unknown_to_the_server_version() {
        let sql = "with recursive r as (select 1) select * from r";
        assert_eq!(
            rewrite(sql, Dialect::MySql, Some("5.7.44")),
            "WITH [recursive] r AS (SELECT 1) SELECT * FROM r"
        );
        assert_eq!(
            rewrite(sql, Dialect::MySql, Some("8.0.36")),
            "WITH RECURSIVE r AS (SELECT 1) SELECT * FROM r"
        );
    }

    #[test]
    fn gives_up_on_unparsable_text() {
        assert_eq!(
            analyze("select from where", Dialect::MySql, None, &catalog()),
            None
        );
    }

    #[test]
    fn records_identifier_positions() {
        let sql = "select a from t";
        let statements = Parser::parse_sql(Dialect::MySql.parser_dialect().as_ref(), sql).unwrap();
        let positions = identifier_positions(&serde_json::to_value(&statements).unwrap());
        assert!(positions.contains_key(&position_of(Location::new(1, 8))));
        assert!(positions.contains_key(&position_of(Location::new(1, 15))));
    }
}
