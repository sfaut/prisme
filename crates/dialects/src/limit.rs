//! Limite de lignes par défaut (voir `docs/V1.md`, section 2.3).
//!
//! Si l'utilisateur n'a écrit aucune limite, Prisme ajoute `LIMIT N + 1` : il
//! affiche N lignes, et la ligne en trop, si elle arrive, signale un résultat
//! tronqué. Une limite écrite par l'utilisateur passe telle quelle.
//!
//! L'AST décide s'il faut une limite. L'insertion se fait ensuite dans le texte
//! d'origine, à une position calculée sur les jetons : la requête de
//! l'utilisateur garde sa forme exacte (commentaires, casse, espaces), ce
//! qu'un réaffichage de l'AST ne garantirait pas. Le `LIMIT` se place avant un
//! éventuel `FOR UPDATE` / `FOR SHARE`, et jamais dans un commentaire final.

use sqlparser::ast::{Expr, LimitClause, SetExpr, Statement, Value};
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer};

use crate::Dialect;

/// Ce que Prisme a fait de la limite d'une requête.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitOutcome {
    /// Aucune limite écrite : Prisme a ajouté `LIMIT max_rows + 1`.
    Added,
    /// L'utilisateur a écrit une limite (`LIMIT`, `FETCH FIRST`) : elle s'applique telle quelle.
    UserLimit,
    /// La requête ne renvoie pas de lignes à limiter (`SHOW`, `EXPLAIN`, écriture, DDL...).
    NotApplicable,
    /// La requête n'a pas pu être analysée : elle passe telle quelle, et la
    /// lecture doit s'arrêter d'elle-même après `max_rows + 1` lignes.
    Unparsed,
}

/// Requête prête à être exécutée, avec ce qui a été fait de sa limite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitedQuery {
    /// Texte à envoyer au serveur.
    pub sql: String,
    /// Traitement appliqué à la limite.
    pub outcome: LimitOutcome,
}

/// Erreurs qui empêchent d'exécuter le texte saisi.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitError {
    /// Le texte ne contient aucune requête.
    #[error("aucune requête à exécuter")]
    Empty,

    /// Le texte contient plusieurs requêtes : une seule à la fois.
    #[error("{0} requêtes reçues : une seule requête à la fois")]
    MultipleStatements(usize),
}

/// Applique la limite par défaut de `max_rows` lignes à la requête `sql`.
pub fn apply_default_limit(
    sql: &str,
    dialect: Dialect,
    max_rows: u64,
) -> Result<LimitedQuery, LimitError> {
    let parser_dialect = dialect.parser_dialect();
    let unchanged = |outcome| LimitedQuery {
        sql: sql.to_string(),
        outcome,
    };

    let Ok(statements) = Parser::parse_sql(parser_dialect.as_ref(), sql) else {
        return Ok(unchanged(LimitOutcome::Unparsed));
    };
    let query = match statements.as_slice() {
        [] => return Err(LimitError::Empty),
        [Statement::Query(query)] => query,
        [_] => return Ok(unchanged(LimitOutcome::NotApplicable)),
        _ => return Err(LimitError::MultipleStatements(statements.len())),
    };

    if query.limit_clause.is_some() || query.fetch.is_some() {
        return Ok(unchanged(LimitOutcome::UserLimit));
    }
    if !returns_rows(&query.body) {
        return Ok(unchanged(LimitOutcome::NotApplicable));
    }

    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return Ok(unchanged(LimitOutcome::Unparsed));
    };
    let Some(position) = insertion_position(sql, &tokens, !query.locks.is_empty()) else {
        return Ok(unchanged(LimitOutcome::Unparsed));
    };

    Ok(LimitedQuery {
        sql: format!(
            "{} LIMIT {}{}",
            &sql[..position],
            max_rows + 1,
            &sql[position..]
        ),
        outcome: LimitOutcome::Added,
    })
}

/// Nombre de lignes que la limite écrite par l'utilisateur autorise
/// (`LIMIT n`, `LIMIT m, n`, `FETCH FIRST n ROWS`), quand c'est un nombre
/// littéral ; `None` sans limite écrite, pour une limite calculée ou un
/// pourcentage. L'interface s'en sert pour la barre d'avancement.
pub fn written_limit(sql: &str, dialect: Dialect) -> Option<u64> {
    let statements = Parser::parse_sql(dialect.parser_dialect().as_ref(), sql).ok()?;
    let [Statement::Query(query)] = statements.as_slice() else {
        return None;
    };
    let expression = match (&query.limit_clause, &query.fetch) {
        (Some(LimitClause::LimitOffset { limit, .. }), _) => limit.as_ref()?,
        (Some(LimitClause::OffsetCommaLimit { limit, .. }), _) => limit,
        (None, Some(fetch)) if !fetch.percent => fetch.quantity.as_ref()?,
        _ => return None,
    };
    match expression {
        Expr::Value(value) => match &value.value {
            Value::Number(number, _) => number.parse().ok(),
            _ => None,
        },
        _ => None,
    }
}

/// Indique si le corps d'une requête renvoie des lignes au client.
pub(crate) fn returns_rows(body: &SetExpr) -> bool {
    match body {
        // SELECT ... INTO écrit dans une table ou des variables : rien à limiter.
        SetExpr::Select(select) => select.into.is_none(),
        SetExpr::Query(query) => returns_rows(&query.body),
        SetExpr::SetOperation { .. } | SetExpr::Values(_) | SetExpr::Table(_) => true,
        // WITH ... INSERT / UPDATE / DELETE / MERGE.
        _ => false,
    }
}

/// Calcule la position, en octets, où insérer ` LIMIT n` dans `sql`.
///
/// C'est la fin du dernier jeton significatif (hors espaces, commentaires et
/// `;`), avant la clause de verrouillage si `has_locks` est vrai.
fn insertion_position(sql: &str, tokens: &[TokenWithSpan], has_locks: bool) -> Option<usize> {
    let boundary = if has_locks {
        lock_clause_start(tokens)?
    } else {
        tokens.len()
    };
    let last = tokens[..boundary]
        .iter()
        .rev()
        .find(|token| !matches!(token.token, Token::Whitespace(_) | Token::SemiColon))?;
    byte_offset(sql, last.span.end)
}

/// Renvoie l'indice du jeton `FOR` qui ouvre la clause de verrouillage
/// (`FOR UPDATE`, `FOR SHARE`), hors parenthèses.
fn lock_clause_start(tokens: &[TokenWithSpan]) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match &token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            Token::Word(word) if depth == 0 && word.keyword == Keyword::FOR => {
                let next = tokens[index + 1..]
                    .iter()
                    .find(|token| !matches!(token.token, Token::Whitespace(_)));
                if let Some(TokenWithSpan {
                    token: Token::Word(next),
                    ..
                }) = next
                    && matches!(next.keyword, Keyword::UPDATE | Keyword::SHARE)
                {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Convertit une position du tokenizer (ligne et colonne en caractères, à
/// partir de 1) en position en octets dans `sql`.
pub(crate) fn byte_offset(sql: &str, location: Location) -> Option<usize> {
    let line_index = usize::try_from(location.line).ok()?.checked_sub(1)?;
    let column_index = usize::try_from(location.column).ok()?.checked_sub(1)?;

    let line_start: usize = sql
        .split_inclusive('\n')
        .take(line_index)
        .map(str::len)
        .sum();
    let line = &sql[line_start..];
    // Une colonne juste après le dernier caractère de la ligne est valide (fin de jeton).
    let in_line = line
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(line.len()))
        .nth(column_index)?;
    Some(line_start + in_line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applique une limite de 1 000 lignes en dialecte MySQL.
    fn limit(sql: &str) -> LimitedQuery {
        apply_default_limit(sql, Dialect::MySql, 1000).unwrap()
    }

    fn added(sql: &str) -> String {
        let query = limit(sql);
        assert_eq!(query.outcome, LimitOutcome::Added, "requête : {sql}");
        query.sql
    }

    #[test]
    fn reads_the_limit_written_by_the_user() {
        let read = |sql| written_limit(sql, Dialect::MySql);
        assert_eq!(read("SELECT * FROM t LIMIT 50"), Some(50));
        assert_eq!(read("SELECT * FROM t LIMIT 10, 20"), Some(20));
        assert_eq!(read("SELECT * FROM t LIMIT 20 OFFSET 10"), Some(20));
        assert_eq!(read("SELECT * FROM t"), None);
        assert_eq!(read("SELECT * FROM t LIMIT @n"), None);
        assert_eq!(read("SHOW TABLES"), None);
    }

    #[test]
    fn adds_limit_to_plain_select() {
        assert_eq!(added("SELECT * FROM t"), "SELECT * FROM t LIMIT 1001");
    }

    #[test]
    fn keeps_the_user_text_intact() {
        assert_eq!(
            added("select  a,\n  b -- colonnes\nfrom t\nwhere x = 'é;' ;"),
            "select  a,\n  b -- colonnes\nfrom t\nwhere x = 'é;' LIMIT 1001 ;"
        );
    }

    #[test]
    fn inserts_before_trailing_comments() {
        assert_eq!(added("SELECT 1 -- fin"), "SELECT 1 LIMIT 1001 -- fin");
        assert_eq!(
            added("SELECT 1 /* fin */\n"),
            "SELECT 1 LIMIT 1001 /* fin */\n"
        );
    }

    #[test]
    fn inserts_after_order_by_and_on_whole_union() {
        assert_eq!(
            added("SELECT a FROM t ORDER BY a DESC"),
            "SELECT a FROM t ORDER BY a DESC LIMIT 1001"
        );
        assert_eq!(
            added("SELECT a FROM t UNION SELECT a FROM u"),
            "SELECT a FROM t UNION SELECT a FROM u LIMIT 1001"
        );
    }

    #[test]
    fn handles_cte_and_subqueries() {
        assert_eq!(
            added("WITH x AS (SELECT 1 AS a LIMIT 5) SELECT * FROM x"),
            "WITH x AS (SELECT 1 AS a LIMIT 5) SELECT * FROM x LIMIT 1001"
        );
        assert_eq!(
            added("SELECT * FROM (SELECT a FROM t LIMIT 3) AS s"),
            "SELECT * FROM (SELECT a FROM t LIMIT 3) AS s LIMIT 1001"
        );
    }

    #[test]
    fn inserts_before_lock_clause() {
        assert_eq!(
            added("SELECT * FROM t WHERE id > 1 FOR UPDATE"),
            "SELECT * FROM t WHERE id > 1 LIMIT 1001 FOR UPDATE"
        );
        assert_eq!(
            added("SELECT * FROM t\nFOR SHARE NOWAIT;"),
            "SELECT * FROM t LIMIT 1001\nFOR SHARE NOWAIT;"
        );
    }

    #[test]
    fn handles_multibyte_characters_before_the_insertion_point() {
        assert_eq!(
            added("SELECT 'é€😀' AS `clé`\nFROM t"),
            "SELECT 'é€😀' AS `clé`\nFROM t LIMIT 1001"
        );
    }

    #[test]
    fn keeps_user_limits() {
        for sql in [
            "SELECT * FROM t LIMIT 10",
            "SELECT * FROM t LIMIT 5, 10",
            "SELECT * FROM t LIMIT 10 OFFSET 5",
            "SELECT * FROM t LIMIT 50000000",
            "(SELECT a FROM t) UNION (SELECT a FROM u) LIMIT 3",
        ] {
            assert_eq!(
                limit(sql),
                LimitedQuery {
                    sql: sql.to_string(),
                    outcome: LimitOutcome::UserLimit
                },
                "requête : {sql}"
            );
        }
    }

    #[test]
    fn ignores_statements_without_rows() {
        for sql in [
            "SHOW TABLES",
            "EXPLAIN SELECT * FROM t",
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "INSERT INTO t SELECT * FROM u",
            "CREATE TABLE t (a INT)",
            "CALL p()",
        ] {
            assert_eq!(
                limit(sql).outcome,
                LimitOutcome::NotApplicable,
                "requête : {sql}"
            );
            assert_eq!(limit(sql).sql, sql);
        }
    }

    #[test]
    fn unparsable_query_passes_unchanged() {
        let sql = "SELECT * FROM t PROCEDURE ANALYSE()";
        let query = limit(sql);
        assert_eq!(query.outcome, LimitOutcome::Unparsed);
        assert_eq!(query.sql, sql);
    }

    #[test]
    fn rejects_empty_and_multiple_statements() {
        assert_eq!(
            apply_default_limit("  ; -- rien", Dialect::MySql, 1000),
            Err(LimitError::Empty)
        );
        assert_eq!(
            apply_default_limit("SELECT 1; SELECT 2", Dialect::MySql, 1000),
            Err(LimitError::MultipleStatements(2))
        );
    }
}
