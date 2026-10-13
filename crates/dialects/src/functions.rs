//! Règle de l'enjoliveur : noms de fonctions en capitales, sauf les
//! fonctions et procédures de l'utilisateur, écrites dans la casse de leur
//! définition.
//!
//! `select count(*), ma_Fonction(x) from t` devient `select COUNT(*),
//! MaFonction(x) from t` si la base définit `MaFonction`. Le sens ne change
//! pas : en MySQL et MariaDB, un nom de fonction ou de procédure ne tient
//! pas compte de la casse.
//!
//! Les appels de fonction sont trouvés par l'analyseur de `sqlparser`
//! (nœuds `Function` de l'AST et `CALL`), jamais par le texte : dans
//! `INSERT INTO t (a)`, `t` n'est pas une fonction. Quelques fonctions ont
//! une syntaxe propre que l'AST représente sans la position de leur nom
//! (`EXTRACT`, `SUBSTRING`, `TRIM`...) : elles sont reconnues à leur nom
//! suivi d'une parenthèse, sauf derrière un mot qui annonce un nom de table
//! ou d'index (`INTO`, `TABLE`, `ON`...).
//!
//! Un nom de fonction inconnu du catalogue est tenu pour une fonction de
//! MySQL et passe en capitales : sans catalogue chargé, une fonction de
//! l'utilisateur passe donc aussi en capitales, ce qui ne change pas son sens.

use std::collections::HashMap;
use std::ops::ControlFlow;

use sqlparser::ast::{Expr, Function, ObjectNamePart, Statement, Visit, Visitor};
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer};

use crate::Dialect;
use crate::limit::byte_offset;
use crate::parse::parse_each;

/// Fonctions à syntaxe propre, que l'AST représente sans la position de leur nom.
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

/// Mots après lesquels un nom suivi d'une parenthèse désigne une table, une
/// vue ou un index, jamais une fonction (`INSERT INTO trim (a)`).
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

/// Écrit les noms de fonctions en capitales, ou dans la casse de leur
/// définition pour les fonctions et procédures de `routines`. Voir l'en-tête
/// du module.
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

    // Remplacements de la fin vers le début : les positions restent justes.
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

/// Début des noms de fonctions à syntaxe propre (`EXTRACT(`...) : un nom de
/// `SPECIAL_FUNCTIONS` suivi d'une parenthèse, qui ne suit ni un point ni un
/// mot de `OBJECT_KEYWORDS`.
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

/// Relève la position du nom de chaque fonction appelée et de chaque
/// procédure lancée par `CALL`.
#[derive(Default)]
struct FunctionFinder {
    /// Début de chaque nom dans le texte d'origine.
    starts: Vec<Location>,
}

impl FunctionFinder {
    /// Relève le nom d'une fonction : la dernière partie de son nom
    /// (`ma_base.ma_fonction` : `ma_fonction`).
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
