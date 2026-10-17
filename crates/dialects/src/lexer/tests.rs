use std::time::{Duration, Instant};

use sqlparser::dialect::MySqlDialect;
use sqlparser::tokenizer::{Token as OracleToken, Tokenizer, Whitespace};

use super::*;
use crate::offsets::OffsetIndex;

/// Settings of MySQL 8.0.19, default SQL mode.
fn mysql() -> LexerOptions {
    LexerOptions {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        mode: SqlMode::default(),
    }
}

/// Settings of MariaDB 10.6.3, default SQL mode.
fn mariadb() -> LexerOptions {
    LexerOptions {
        dialect: Dialect::MariaDb,
        version: Version::new(10, 6, 3),
        mode: SqlMode::default(),
    }
}

/// Tokens of `text`, each with its text.
fn lex(text: &str, options: &LexerOptions) -> Vec<(TokenKind, String)> {
    tokenize(text, options)
        .unwrap()
        .into_iter()
        .map(|token| {
            let text = token.text(text).to_string();
            (token.kind, text)
        })
        .collect()
}

/// Texts of the tokens of `text`.
fn texts(text: &str, options: &LexerOptions) -> Vec<String> {
    lex(text, options)
        .into_iter()
        .map(|(_, text)| text)
        .collect()
}

/// Kinds of the tokens of `text`.
fn kinds(text: &str, options: &LexerOptions) -> Vec<TokenKind> {
    lex(text, options)
        .into_iter()
        .map(|(kind, _)| kind)
        .collect()
}

/// Message and position of the error on `text`.
fn error(text: &str, options: &LexerOptions) -> (String, &'static str) {
    let error = tokenize(text, options).unwrap_err();
    let faulty: &'static str = text[error.span.start..error.span.end].to_string().leak();
    (error.message, faulty)
}

fn string(value: &str, quote: Quote) -> TokenKind {
    TokenKind::String {
        value: value.to_string(),
        quote,
    }
}

#[test]
fn cuts_a_query_into_words_operators_and_literals() {
    assert_eq!(
        texts("SELECT a, t.b FROM t WHERE c>=1.5 AND d<>'x'", &mysql()),
        [
            "SELECT", "a", ",", "t", ".", "b", "FROM", "t", "WHERE", "c", ">=", "1.5", "AND", "d",
            "<>", "'x'"
        ]
    );
    assert_eq!(
        kinds("a<=>b->>'$.c'", &mysql()),
        [
            TokenKind::Word,
            TokenKind::Punct(Punct::Spaceship),
            TokenKind::Word,
            TokenKind::Punct(Punct::LongArrow),
            string("$.c", Quote::Single),
        ]
    );
    assert_eq!(
        texts("@x:=1&&2||3!=4<<5", &mysql()),
        ["@x", ":=", "1", "&&", "2", "||", "3", "!=", "4", "<<", "5"]
    );
}

#[test]
fn resolves_the_escapes_and_doubled_quotes_of_strings() {
    assert_eq!(
        kinds(r#"'it''s' "say ""hi""" 'a\nb\'c\%' "#, &mysql()),
        [
            string("it's", Quote::Single),
            string("say \"hi\"", Quote::Double),
            string("a\nb'c\\%", Quote::Single),
        ]
    );
    // Backslash before a non-ASCII character: the character itself.
    assert_eq!(kinds(r"'\é'", &mysql()), [string("é", Quote::Single)]);
}

#[test]
fn follows_the_sql_mode_for_quotes_and_backslashes() {
    let mut options = mysql();
    options.mode = SqlMode::parse("ANSI_QUOTES,NO_BACKSLASH_ESCAPES");
    assert_eq!(
        kinds(r#""col" 'a\'"#, &options),
        [
            TokenKind::QuotedIdent("col".to_string()),
            string(r"a\", Quote::Single),
        ]
    );
    // Without NO_BACKSLASH_ESCAPES, the same string is left open.
    assert_eq!(error(r"'a\'", &mysql()).0, "unterminated string");
}

#[test]
fn reads_quoted_identifiers_without_escapes() {
    assert_eq!(
        kinds(r"`a``b` `c\`", &mysql()),
        [
            TokenKind::QuotedIdent("a`b".to_string()),
            TokenKind::QuotedIdent(r"c\".to_string()),
        ]
    );
}

#[test]
fn reads_the_numbers_and_the_words_that_start_with_digits() {
    let cases = [
        ("123", TokenKind::Integer),
        ("1.5", TokenKind::Decimal),
        (".5", TokenKind::Decimal),
        ("1.", TokenKind::Decimal),
        ("1e5", TokenKind::Float),
        ("1.5E-3", TokenKind::Float),
        ("0x4A", TokenKind::HexNumber("4A".to_string())),
        ("0b0101", TokenKind::BitNumber("0101".to_string())),
        ("1a", TokenKind::Word),
        ("1e", TokenKind::Word),
        ("0xZ", TokenKind::Word),
        ("0X4A", TokenKind::Word),
    ];
    for (text, kind) in cases {
        assert_eq!(lex(text, &mysql()), [(kind, text.to_string())], "{text}");
    }
    // After a qualifier, `.5a` is not a decimal: column `5a` of `t`.
    assert_eq!(texts("t.5a", &mysql()), ["t", ".", "5a"]);
    assert_eq!(texts("1+.5", &mysql()), ["1", "+", ".5"]);
}

#[test]
fn reads_the_prefixed_strings() {
    assert_eq!(
        kinds("N'a' X'4A' b'01' _utf8mb4'c'", &mysql()),
        [
            TokenKind::NationalString("a".to_string()),
            TokenKind::HexString("4A".to_string()),
            TokenKind::BitString("01".to_string()),
            TokenKind::Word,
            string("c", Quote::Single),
        ]
    );
    assert_eq!(
        error("X'4'", &mysql()),
        ("invalid hexadecimal literal".to_string(), "X'4'")
    );
    assert_eq!(
        error("B'12'", &mysql()),
        ("invalid bit literal".to_string(), "B'12'")
    );
}

#[test]
fn reads_user_and_system_variables() {
    assert_eq!(
        lex("@x @a.b @'my var' @@session.sql_mode @", &mysql()),
        [
            (
                TokenKind::UserVariable {
                    name: "x".to_string(),
                    quoted: false
                },
                "@x".to_string()
            ),
            (
                TokenKind::UserVariable {
                    name: "a.b".to_string(),
                    quoted: false
                },
                "@a.b".to_string()
            ),
            (
                TokenKind::UserVariable {
                    name: "my var".to_string(),
                    quoted: true
                },
                "@'my var'".to_string()
            ),
            (TokenKind::SystemVariable, "@@session".to_string()),
            (TokenKind::Punct(Punct::Dot), ".".to_string()),
            (TokenKind::Word, "sql_mode".to_string()),
            (TokenKind::Punct(Punct::At), "@".to_string()),
        ]
    );
}

#[test]
fn keeps_the_comments_as_tokens() {
    assert_eq!(
        lex(
            "SELECT 1 -- one\n# two\n/* three */ /*+ BKA(t) */",
            &mysql()
        )
        .into_iter()
        .filter(|(kind, _)| matches!(kind, TokenKind::Comment(_)))
        .collect::<Vec<_>>(),
        [
            (TokenKind::Comment(CommentKind::Dash), "-- one".to_string()),
            (TokenKind::Comment(CommentKind::Hash), "# two".to_string()),
            (
                TokenKind::Comment(CommentKind::Block),
                "/* three */".to_string()
            ),
            (
                TokenKind::Comment(CommentKind::Hint),
                "/*+ BKA(t) */".to_string()
            ),
        ]
    );
    // `--` without a space is two minus signs.
    assert_eq!(texts("5--2", &mysql()), ["5", "-", "-", "2"]);
}

#[test]
fn executable_comments_are_code_when_their_condition_holds() {
    let start = |version| TokenKind::ConditionalStart {
        version,
        mariadb_only: false,
    };
    assert_eq!(
        kinds("SELECT /*! 1 */", &mysql()),
        [
            TokenKind::Word,
            start(None),
            TokenKind::Integer,
            TokenKind::ConditionalEnd
        ]
    );
    assert_eq!(
        texts("SELECT /*!80019 STRAIGHT_JOIN */ 1", &mysql()),
        ["SELECT", "/*!80019", "STRAIGHT_JOIN", "*/", "1"]
    );
    // A version above the one chosen: a comment.
    assert_eq!(
        kinds("SELECT /*!80020 STRAIGHT_JOIN */ 1", &mysql()),
        [
            TokenKind::Word,
            TokenKind::Comment(CommentKind::Executable),
            TokenKind::Integer
        ]
    );
    // A string holding `*/` does not end the comment.
    assert_eq!(texts("/*! '*/' */", &mysql()), ["/*!", "'*/'", "*/"]);
}

#[test]
fn mariadb_comments_run_on_mariadb_only() {
    let text = "SELECT 1 /*M!100500 , 2 */";
    assert_eq!(
        kinds(text, &mysql()),
        [
            TokenKind::Word,
            TokenKind::Integer,
            TokenKind::Comment(CommentKind::Executable)
        ]
    );
    assert_eq!(
        kinds(text, &mariadb()),
        [
            TokenKind::Word,
            TokenKind::Integer,
            TokenKind::ConditionalStart {
                version: Some(Version::new(10, 5, 0)),
                mariadb_only: true
            },
            TokenKind::Punct(Punct::Comma),
            TokenKind::Integer,
            TokenKind::ConditionalEnd,
        ]
    );
}

#[test]
fn reads_five_or_six_digit_versions() {
    let version = |text: &str, options: &LexerOptions| match &kinds(text, options)[0] {
        TokenKind::ConditionalStart { version, .. } => *version,
        other => panic!("{other:?}"),
    };
    let mut recent = mysql();
    recent.version = Version::new(99, 0, 0);
    // MySQL: six digits followed by a space make a six-digit version.
    assert_eq!(
        version("/*!100000 1 */", &recent),
        Some(Version::new(10, 0, 0))
    );
    // Otherwise five digits, the sixth is content.
    assert_eq!(
        texts("/*!100000+2*/", &recent),
        ["/*!10000", "0", "+", "2", "*/"]
    );
    assert_eq!(
        version("/*!80019 1 */", &recent),
        Some(Version::new(8, 0, 19))
    );
    // MariaDB: six digits when there are six.
    assert_eq!(
        version("/*!100500+2*/", &mariadb()),
        Some(Version::new(10, 5, 0))
    );
    // Fewer than five digits: no version, the digits are content.
    assert_eq!(texts("/*!123 */", &mysql()), ["/*!", "123", "*/"]);
}

#[test]
fn refuses_what_the_dialect_does_not_read() {
    assert_eq!(
        error("SELECT 'a", &mysql()),
        ("unterminated string".to_string(), "'a")
    );
    assert_eq!(
        error("SELECT `a", &mysql()),
        ("unterminated quoted identifier".to_string(), "`a")
    );
    assert_eq!(error("SELECT /* a", &mysql()).0, "unterminated comment");
    assert_eq!(error("SELECT /*! a", &mysql()).0, "unterminated comment");
    assert_eq!(
        error("SELECT a😀", &mysql()),
        (
            "character not allowed in an unquoted identifier".to_string(),
            "😀"
        )
    );
    assert_eq!(error("SELECT \\N", &mysql()).1, "\\");
    // Non-ASCII characters up to U+FFFF are allowed.
    assert_eq!(texts("SELECT café", &mysql()), ["SELECT", "café"]);
}

/// Queries whose tokens are compared with those of `sqlparser`.
const ORACLE_CORPUS: [&str; 12] = [
    "SELECT * FROM t WHERE a = 1 AND b <> 'x' OR c IS NOT NULL",
    "WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds",
    "SELECT a.b, `c d`, \"str\", 'it''s', 'a\\'b' FROM db.t AS a",
    "SELECT 1.5, .5, 1e5, 1.5E-3, 0x4A, X'4A', N'nat', 123abc FROM t",
    "SELECT a <=> b, c -> '$.x', d ->> '$.y', e && f, g || h, ~i, j ^ k, l % m",
    "SELECT @x, @@sql_mode, @@session.sql_mode, ? FROM t",
    "SELECT a -- comment\nFROM t # other\nWHERE /* block */ b = 1",
    "SELECT /*+ BKA(t) */ * FROM t LIMIT 10, 20",
    "INSERT INTO t (a, b) VALUES (1, 'x') ON DUPLICATE KEY UPDATE b = VALUES(b)",
    "UPDATE t SET a = a + 1 WHERE b IN (1, 2, 3) ORDER BY c DESC LIMIT 5",
    "SELECT café, $x, _utf8mb4'a' COLLATE utf8mb4_bin FROM t",
    "SELECT CAST(a AS DECIMAL(10, 2)), a DIV 2, a MOD 3, !b FROM t",
];

/// Spans of the tokens of `sqlparser`, whitespace aside, trailing whitespace
/// of the comments trimmed.
fn oracle_spans(sql: &str) -> Vec<(usize, usize)> {
    let tokens = Tokenizer::new(&MySqlDialect {}, sql)
        .tokenize_with_location()
        .unwrap();
    let index = OffsetIndex::new(sql);
    tokens
        .iter()
        .filter(|token| {
            !matches!(
                token.token,
                OracleToken::Whitespace(Whitespace::Space | Whitespace::Newline | Whitespace::Tab)
                    | OracleToken::EOF
            )
        })
        .map(|token| {
            let start = index.offset(token.span.start).unwrap();
            let end = index.offset(token.span.end).unwrap();
            (start, start + sql[start..end].trim_end().len())
        })
        .collect()
}

#[test]
fn cuts_like_sqlparser_on_the_corpus() {
    // Listed difference: `sqlparser` stops an unquoted user variable at a
    // dot, MySQL does not (`@a.b`); none is in the corpus.
    for sql in ORACLE_CORPUS {
        let ours: Vec<(usize, usize)> = tokenize(sql, &mysql())
            .unwrap()
            .iter()
            .map(|token| (token.span.start, token.span.end))
            .collect();
        assert_eq!(ours, oracle_spans(sql), "{sql}");
    }
}

#[test]
fn time_is_linear_in_the_size_of_the_query() {
    // About 1 MB on a single line: a quadratic lexer would take minutes.
    let query = format!(
        "SELECT {} FROM t",
        (0..60_000)
            .map(|i| format!("a{i} + 'v{i}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let start = Instant::now();
    let tokens = tokenize(&query, &mysql()).unwrap();
    assert_eq!(tokens.len(), 60_000 * 4 + 2);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
}
