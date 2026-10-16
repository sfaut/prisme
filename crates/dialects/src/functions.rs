//! Beautifier rule: function names in capitals, except the user's functions
//! and procedures, written in the case of their definition.
//!
//! `select count(*), my_Function(x) from t` becomes `select COUNT(*),
//! MyFunction(x) from t` if the database defines `MyFunction`. The meaning
//! does not change: in MySQL and MariaDB, a function or procedure name is
//! case-insensitive.
//!
//! Function calls are found by `sqlparser`'s parser (`Function` nodes of the
//! AST and `CALL`), never by the text: in `INSERT INTO t (a)`, `t` is not a
//! function. A few functions have their own syntax, which the AST represents
//! without the position of their name (`EXTRACT`, `SUBSTRING`, `TRIM`...):
//! they are recognised by their name followed by a parenthesis, except after
//! a word that announces a table or index name (`INTO`, `TABLE`, `ON`...).
//!
//! A function name unknown to the catalog is taken for a MySQL function and
//! goes to capitals: without a loaded catalog, a user function therefore also
//! goes to capitals, which does not change its meaning.

use std::collections::HashMap;
use std::ops::ControlFlow;

use sqlparser::ast::{Expr, Function, ObjectNamePart, Statement, Visit, Visitor};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer};

use crate::Dialect;
use crate::limit::byte_offset;
use crate::parse::parse_each;

/// Functions with their own syntax, which the AST represents without the position of their name.
const SPECIAL_FUNCTIONS: &[&str] = &[
    "CEIL",
    "CEILING",
    "EXTRACT",
    "FLOOR",
    "OVERLAY",
    "POSITION",
    "SUBSTR",
    "SUBSTRING",
    "TRIM",
];

/// Words after which a name followed by a parenthesis designates a table, a
/// view or an index, never a function (`INSERT INTO trim (a)`).
const OBJECT_KEYWORDS: &[Keyword] = &[
    Keyword::AS,
    Keyword::CONSTRAINT,
    Keyword::FUNCTION,
    Keyword::INDEX,
    Keyword::INTO,
    Keyword::KEY,
    Keyword::ON,
    Keyword::PROCEDURE,
    Keyword::REFERENCES,
    Keyword::TABLE,
    Keyword::TRIGGER,
    Keyword::VIEW,
];

/// Writes function names in capitals, or in the case of their definition for
/// the functions and procedures of `routines`. See the module header.
pub(crate) fn case_function_names(sql: &str, dialect: Dialect, routines: &[String]) -> String {
    let parser_dialect = dialect.parser_dialect();
    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return sql.to_string();
    };
    let defined: HashMap<String, &str> = routines
        .iter()
        .map(|name| (name.to_lowercase(), name.as_str()))
        .collect();

    let mut finder = FunctionFinder::default();
    for statement in parse_each(sql, dialect) {
        let _ = statement.visit(&mut finder);
    }
    let mut starts = finder.starts;
    starts.extend(special_function_starts(&tokens));

    // Replacements from the end to the start: positions stay correct.
    let mut replacements: Vec<(usize, usize, String)> = starts
        .into_iter()
        .filter_map(|start| {
            let token = tokens.iter().find(|token| token.span.start == start)?;
            let Token::Word(word) = &token.token else {
                return None;
            };
            if word.quote_style.is_some() {
                return None;
            }
            let name = match defined.get(&word.value.to_lowercase()) {
                Some(name) => (*name).to_string(),
                None => word.value.to_uppercase(),
            };
            Some((
                byte_offset(sql, token.span.start)?,
                byte_offset(sql, token.span.end)?,
                name,
            ))
        })
        .collect();
    replacements.sort_unstable_by_key(|(start, _, _)| *start);
    replacements.dedup_by_key(|(start, _, _)| *start);
    let mut result = sql.to_string();
    for (start, end, name) in replacements.into_iter().rev() {
        result.replace_range(start..end, &name);
    }
    result
}

/// Start of the names of functions with their own syntax (`EXTRACT(`...): a
/// name of `SPECIAL_FUNCTIONS` followed by a parenthesis, which follows
/// neither a period nor a word of `OBJECT_KEYWORDS`.
fn special_function_starts(tokens: &[TokenWithSpan]) -> Vec<Location> {
    let significant: Vec<&TokenWithSpan> = tokens
        .iter()
        .filter(|token| !matches!(token.token, Token::Whitespace(_)))
        .collect();
    significant
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            let Token::Word(word) = &token.token else {
                return None;
            };
            let special = word.quote_style.is_none()
                && SPECIAL_FUNCTIONS.contains(&word.value.to_uppercase().as_str());
            let called = significant
                .get(index + 1)
                .is_some_and(|next| next.token == Token::LParen);
            let object = index
                .checked_sub(1)
                .and_then(|previous| significant.get(previous))
                .is_some_and(|previous| match &previous.token {
                    Token::Period => true,
                    Token::Word(word) => OBJECT_KEYWORDS.contains(&word.keyword),
                    _ => false,
                });
            (special && called && !object).then_some(token.span.start)
        })
        .collect()
}

/// Collects the position of the name of each called function and of each
/// procedure run by `CALL`.
#[derive(Default)]
struct FunctionFinder {
    /// Start of each name in the original text.
    starts: Vec<Location>,
}

impl FunctionFinder {
    /// Collects the name of a function: the last part of its name
    /// (`my_db.my_function`: `my_function`).
    fn function(&mut self, function: &Function) {
        if let Some(ObjectNamePart::Identifier(name)) = function.name.0.last() {
            self.starts.push(name.span.start);
        }
    }
}

impl Visitor for FunctionFinder {
    type Break = ();

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
        if let Expr::Function(function) = expr {
            self.function(function);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<()> {
        if let Statement::Call(function) = statement {
            self.function(function);
        }
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cased(sql: &str) -> String {
        case_function_names(
            sql,
            Dialect::MySql,
            &["MaFonction".to_string(), "Purge_Logs".to_string()],
        )
    }

    #[test]
    fn uppercases_builtin_functions() {
        assert_eq!(
            cased("select count(*), concat(a, now()), coalesce(b, 0) from t"),
            "select COUNT(*), CONCAT(a, NOW()), COALESCE(b, 0) from t"
        );
    }

    #[test]
    fn writes_user_routines_in_their_defined_case() {
        assert_eq!(
            cased("select mafonction(x), app.MAFONCTION(y) from t; call purge_logs()"),
            "select MaFonction(x), app.MaFonction(y) from t; call Purge_Logs()"
        );
    }

    #[test]
    fn leaves_tables_columns_and_quoted_names_alone() {
        let sql = "insert into trim (a, `count`) select `count`(x) from floor";
        assert_eq!(cased(sql), sql);
        assert_eq!(
            cased("select extract(day from d), substring(s, 2), trim(s), floor(x) from t"),
            "select EXTRACT(day from d), SUBSTRING(s, 2), TRIM(s), FLOOR(x) from t"
        );
    }
}
