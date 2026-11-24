//! Default row limit (see `docs/V1.md`, section 2.3).
//!
//! If the user wrote no limit, Prisme adds `LIMIT N + 1`: it shows N rows,
//! and the extra row, if it arrives, reports a truncated result. A limit
//! written by the user goes through as is.
//!
//! The AST decides whether a limit is needed. The insertion then happens in
//! the original text, at a position computed on the tokens: the user's query
//! keeps its exact form (comments, case, spaces), which printing the AST back
//! would not guarantee. The `LIMIT` goes before a possible `FOR UPDATE` /
//! `FOR SHARE`, and never inside a trailing comment.
//!
//! SQLite refuses a `LIMIT` after a `VALUES` (`docs/SQL-COMPARISON.md`,
//! section 6): such a query goes as is, and reading stops by itself after
//! the extra row.

use sqlparser::ast::{Expr, LimitClause, SetExpr, Statement, Value};
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, TokenWithSpan, Tokenizer};

use crate::SessionDialect;
use crate::offsets::OffsetIndex;
use crate::parse::{Parsed, parse_statements};

/// What Prisme did with the limit of a query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitOutcome {
    /// No limit written: Prisme added `LIMIT max_rows + 1`.
    Added,
    /// The user wrote a limit (`LIMIT`, `FETCH FIRST`): it applies as is.
    UserLimit,
    /// The query returns no rows to limit (`SHOW`, `EXPLAIN`, write, DDL...).
    NotApplicable,
    /// No limit written, and the dialect takes none after the query
    /// (`VALUES` of SQLite): it goes through as is, and reading must stop by
    /// itself after `max_rows + 1` rows.
    Capped,
    /// The query could not be parsed: it goes through as is, and reading must
    /// stop by itself after `max_rows + 1` rows.
    Unparsed,
}

/// Query ready to run, with what was done with its limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitedQuery {
    /// Text to send to the server.
    pub sql: String,
    /// Handling applied to the limit.
    pub outcome: LimitOutcome,
}

/// Errors that prevent running the typed text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitError {
    /// The text contains no query.
    #[error("no query to run")]
    Empty,

    /// The text contains several queries: one at a time.
    #[error("{0} queries received: one query at a time")]
    MultipleStatements(usize),
}

/// Applies the default limit of `max_rows` rows to the query `sql`, read in
/// the lexical settings of the session.
pub fn apply_default_limit(
    sql: &str,
    dialect: impl Into<SessionDialect>,
    max_rows: u64,
) -> Result<LimitedQuery, LimitError> {
    let session: SessionDialect = dialect.into();
    let dialect = session.dialect;
    let parser_dialect = session.parser_dialect();
    let unchanged = |outcome| LimitedQuery {
        sql: sql.to_string(),
        outcome,
    };

    let Ok(statements) = parse_statements(sql, session) else {
        return Ok(unchanged(LimitOutcome::Unparsed));
    };
    let query = match statements.as_slice() {
        [] => return Err(LimitError::Empty),
        [Parsed::Sql(Statement::Query(query))] => query,
        [_] => return Ok(unchanged(LimitOutcome::NotApplicable)),
        _ => return Err(LimitError::MultipleStatements(statements.len())),
    };

    if query.limit_clause.is_some() || query.fetch.is_some() {
        return Ok(unchanged(LimitOutcome::UserLimit));
    }
    if !returns_rows(&query.body) {
        return Ok(unchanged(LimitOutcome::NotApplicable));
    }
    if !dialect.spec().values_take_limit && ends_with_values(&query.body) {
        return Ok(unchanged(LimitOutcome::Capped));
    }

    let Ok(tokens) = Tokenizer::new(parser_dialect.as_ref(), sql).tokenize_with_location() else {
        return Ok(unchanged(LimitOutcome::Unparsed));
    };
    let index = OffsetIndex::new(sql);
    let Some(position) = insertion_position(&index, &tokens, !query.locks.is_empty()) else {
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

/// Number of rows allowed by the limit written by the user (`LIMIT n`,
/// `LIMIT m, n`, `FETCH FIRST n ROWS`), when it is a literal number; `None`
/// without a written limit, for a computed limit or a percentage. The
/// interface uses it for the progress bar.
pub fn written_limit(sql: &str, dialect: impl Into<SessionDialect>) -> Option<u64> {
    let statements = Parser::parse_sql(dialect.into().parser_dialect().as_ref(), sql).ok()?;
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

/// Tells whether the body of a query returns rows to the client.
pub(crate) fn returns_rows(body: &SetExpr) -> bool {
    match body {
        // SELECT ... INTO writes into a table or variables: nothing to limit.
        SetExpr::Select(select) => select.into.is_none(),
        SetExpr::Query(query) => returns_rows(&query.body),
        SetExpr::SetOperation { .. } | SetExpr::Values(_) | SetExpr::Table(_) => true,
        // WITH ... INSERT / UPDATE / DELETE / MERGE.
        _ => false,
    }
}

/// Does the body of a query end with a `VALUES`, alone or last of a set
/// operation, where a `LIMIT` would follow it?
fn ends_with_values(body: &SetExpr) -> bool {
    match body {
        SetExpr::Values(_) => true,
        SetExpr::SetOperation { right, .. } => ends_with_values(right),
        _ => false,
    }
}

/// Computes the position, in bytes, where to insert ` LIMIT n` in `sql`.
///
/// It is the end of the last significant token (outside spaces, comments and
/// `;`), before the locking clause if `has_locks` is true.
fn insertion_position(
    index: &OffsetIndex,
    tokens: &[TokenWithSpan],
    has_locks: bool,
) -> Option<usize> {
    let boundary = if has_locks {
        lock_clause_start(tokens)?
    } else {
        tokens.len()
    };
    let last = tokens[..boundary]
        .iter()
        .rev()
        .find(|token| !matches!(token.token, Token::Whitespace(_) | Token::SemiColon))?;
    index.offset(last.span.end)
}

/// Returns the index of the `FOR` token that opens the locking clause
/// (`FOR UPDATE`, `FOR SHARE`), outside parentheses.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dialect;

    /// Applies a limit of 1,000 rows in the MySQL dialect.
    fn limit(sql: &str) -> LimitedQuery {
        apply_default_limit(sql, Dialect::MySql, 1000).unwrap()
    }

    fn added(sql: &str) -> String {
        let query = limit(sql);
        assert_eq!(query.outcome, LimitOutcome::Added, "query: {sql}");
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
        // Query generated by the navigator, quantifier written in full.
        assert_eq!(
            added("SELECT ALL * FROM `b`.`t`"),
            "SELECT ALL * FROM `b`.`t` LIMIT 1001"
        );
    }

    #[test]
    fn adds_limit_to_a_query_with_a_compound_time_unit() {
        assert_eq!(
            added("SELECT EXTRACT(YEAR_MONTH FROM d) FROM t"),
            "SELECT EXTRACT(YEAR_MONTH FROM d) FROM t LIMIT 1001"
        );
    }

    #[test]
    fn keeps_the_user_text_intact() {
        assert_eq!(
            added("select  a,\n  b -- columns\nfrom t\nwhere x = 'é;' ;"),
            "select  a,\n  b -- columns\nfrom t\nwhere x = 'é;' LIMIT 1001 ;"
        );
    }

    #[test]
    fn inserts_before_trailing_comments() {
        assert_eq!(added("SELECT 1 -- end"), "SELECT 1 LIMIT 1001 -- end");
        assert_eq!(
            added("SELECT 1 /* end */\n"),
            "SELECT 1 LIMIT 1001 /* end */\n"
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
                "query: {sql}"
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
                "query: {sql}"
            );
            assert_eq!(limit(sql).sql, sql);
        }
    }

    #[test]
    fn limits_the_queries_of_sqlite() {
        // Each query limited runs on SQLite, and stops at the extra row.
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE t (a);
             WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 10)
             INSERT INTO t SELECT n FROM r;",
        )
        .unwrap();
        let count = |sql: &str| {
            let mut statement = db.prepare(sql).unwrap();
            statement.query_map([], |_| Ok(())).unwrap().count()
        };
        for sql in [
            "SELECT a FROM t",
            "select a from t order by a desc;",
            "SELECT a FROM t UNION SELECT a + 10 FROM t",
            "VALUES (1), (2) UNION SELECT a FROM t",
            "WITH r AS (SELECT a FROM t) SELECT * FROM r -- end",
            "SELECT a FROM t WHERE a IN (SELECT a FROM t LIMIT 5)",
            "SELECT [a] FROM t /* ; */",
        ] {
            let limited = apply_default_limit(sql, Dialect::Sqlite, 2).unwrap();
            assert_eq!(limited.outcome, LimitOutcome::Added, "query: {sql}");
            assert_eq!(count(&limited.sql), 3, "query: {}", limited.sql);
        }
        // `LIMIT -1` is a limit written: none.
        for (sql, rows) in [
            ("SELECT a FROM t LIMIT -1", 10),
            ("SELECT a FROM t LIMIT 2, 3", 3),
            ("SELECT a FROM t LIMIT 3 OFFSET 8", 2),
        ] {
            let limited = apply_default_limit(sql, Dialect::Sqlite, 2).unwrap();
            assert_eq!(limited.outcome, LimitOutcome::UserLimit, "query: {sql}");
            assert_eq!(count(&limited.sql), rows, "query: {sql}");
        }
        // SQLite takes no LIMIT after a VALUES: reading stops by itself.
        for sql in [
            "VALUES (1), (2), (3), (4)",
            "SELECT a FROM t UNION VALUES (2)",
        ] {
            let limited = apply_default_limit(sql, Dialect::Sqlite, 2).unwrap();
            assert_eq!(limited.outcome, LimitOutcome::Capped, "query: {sql}");
            assert_eq!(limited.sql, sql);
            assert!(
                db.prepare(&format!("{sql} LIMIT 3")).is_err(),
                "query: {sql}"
            );
        }
        assert_eq!(limit("VALUES ROW(1), ROW(2)").outcome, LimitOutcome::Added);
        assert_eq!(
            written_limit("SELECT a FROM t LIMIT -1", Dialect::Sqlite),
            None
        );
        assert_eq!(
            written_limit("SELECT a FROM t LIMIT 2, 3", Dialect::Sqlite),
            Some(3)
        );
        for sql in [
            "PRAGMA table_info(t)",
            "DELETE FROM t WHERE a > 8 RETURNING a",
            "EXPLAIN QUERY PLAN SELECT a FROM t",
        ] {
            let limited = apply_default_limit(sql, Dialect::Sqlite, 2).unwrap();
            assert_eq!(limited.outcome, LimitOutcome::NotApplicable, "query: {sql}");
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
            apply_default_limit("  ; -- none", Dialect::MySql, 1000),
            Err(LimitError::Empty)
        );
        assert_eq!(
            apply_default_limit("SELECT 1; SELECT 2", Dialect::MySql, 1000),
            Err(LimitError::MultipleStatements(2))
        );
    }
}
