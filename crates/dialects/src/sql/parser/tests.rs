use prisme_plan::{AliasForm, LimitForm, ValuesForm};

use super::*;
use crate::sql::ast::{BinaryOp, Expr, ExprKind, Literal, UnaryOp};

/// Settings of MySQL 8.0.19.
fn mysql() -> ParseOptions {
    ParseOptions {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        mode: SqlMode::default(),
        conditional_version: Version::new(8, 0, 19),
    }
}

/// Settings of MariaDB 10.6.3.
fn mariadb() -> ParseOptions {
    ParseOptions {
        dialect: Dialect::MariaDb,
        version: Version::new(10, 6, 3),
        mode: SqlMode::default(),
        conditional_version: Version::new(10, 6, 3),
    }
}

/// Query of `text`.
fn query(text: &str, options: &ParseOptions) -> Query {
    let parsed = parse(text, options).unwrap_or_else(|error| panic!("{text}: {error}"));
    let StatementKind::Query(query) = parsed.statement.kind;
    query
}

/// `SELECT` block of `text`.
fn select(text: &str) -> Select {
    match query(text, &mysql()).body {
        QueryBody::Select(select) => *select,
        other => panic!("{other:?}"),
    }
}

/// Error message and faulty text of `text`.
fn error(text: &str, options: &ParseOptions) -> (String, String) {
    let error = parse(text, options).unwrap_err();
    (
        error.message,
        text[error.span.start..error.span.end].to_string(),
    )
}

/// Expression written as a prefix tree, to test the precedence.
fn tree(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Column(name) => name
            .parts
            .iter()
            .map(|part| part.value.as_str())
            .collect::<Vec<_>>()
            .join("."),
        ExprKind::Literal(literal) => match literal {
            Literal::Integer(text) | Literal::Decimal(text) | Literal::Float(text) => text.clone(),
            Literal::String(value) => format!("'{value}'"),
            Literal::Null => "NULL".to_string(),
            Literal::True => "TRUE".to_string(),
            Literal::False => "FALSE".to_string(),
        },
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnaryOp::Minus => "-",
                UnaryOp::Plus => "+",
                UnaryOp::Not => "not",
                UnaryOp::BitNot => "~",
            };
            format!("({op} {})", tree(operand))
        }
        ExprKind::Binary { op, left, right } => {
            let op = match op {
                BinaryOp::Add => "+",
                BinaryOp::Subtract => "-",
                BinaryOp::Multiply => "*",
                BinaryOp::Divide => "/",
                BinaryOp::IntegerDivide => "div",
                BinaryOp::Modulo => "%",
                BinaryOp::Eq => "=",
                BinaryOp::NotEq => "<>",
                BinaryOp::Lt => "<",
                BinaryOp::LtEq => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::GtEq => ">=",
                BinaryOp::NullSafeEq => "<=>",
                BinaryOp::And => "and",
                BinaryOp::Or => "or",
                BinaryOp::Xor => "xor",
                BinaryOp::BitAnd => "&",
                BinaryOp::BitOr => "|",
                BinaryOp::BitXor => "^",
                BinaryOp::ShiftLeft => "<<",
                BinaryOp::ShiftRight => ">>",
            };
            format!("({op} {} {})", tree(left), tree(right))
        }
        ExprKind::IsNull { operand, negated } => {
            format!(
                "({} {})",
                if *negated { "notnull" } else { "isnull" },
                tree(operand)
            )
        }
        ExprKind::Between {
            operand,
            low,
            high,
            negated,
        } => format!(
            "({}between {} {} {})",
            if *negated { "not" } else { "" },
            tree(operand),
            tree(low),
            tree(high)
        ),
        ExprKind::InList {
            operand,
            list,
            negated,
        } => format!(
            "({}in {} {})",
            if *negated { "not" } else { "" },
            tree(operand),
            list.iter().map(tree).collect::<Vec<_>>().join(" ")
        ),
        ExprKind::Like {
            operand, pattern, ..
        } => format!("(like {} {})", tree(operand), tree(pattern)),
        ExprKind::Nested(inner) => tree(inner),
    }
}

/// Tree of the first item of `SELECT <expr>`.
fn expr(text: &str) -> String {
    expr_with(text, &mysql())
}

/// Tree of the first item of `SELECT <expr>`, with `options`.
fn expr_with(text: &str, options: &ParseOptions) -> String {
    let QueryBody::Select(select) = query(&format!("SELECT {text}"), options).body else {
        panic!("not a SELECT");
    };
    match &select.items[0] {
        SelectItem::Expr { expr, .. } => tree(expr),
        other => panic!("{other:?}"),
    }
}

#[test]
fn operators_follow_the_precedence_of_the_mysql_family() {
    assert_eq!(expr("1 + 2 * 3"), "(+ 1 (* 2 3))");
    assert_eq!(expr("1 - 2 - 3"), "(- (- 1 2) 3)");
    assert_eq!(
        expr("a OR b XOR c AND NOT d"),
        "(or a (xor b (and c (not d))))"
    );
    assert_eq!(
        expr("a = 1 AND b <> 2 || c"),
        "(or (and (= a 1) (<> b 2)) c)"
    );
    assert_eq!(expr("NOT a = b"), "(not (= a b))");
    assert_eq!(expr("!a = b"), "(= (not a) b)");
    assert_eq!(expr("a = b = c"), "(= (= a b) c)");
    assert_eq!(
        expr("a | b & c << 1 + 2 ^ 3"),
        "(| a (& b (<< c (+ 1 (^ 2 3)))))"
    );
    assert_eq!(expr("-a * b DIV c MOD d"), "(% (div (* (- a) b) c) d)");
    assert_eq!(expr("(1 + 2) * 3"), "(* (+ 1 2) 3)");
    assert_eq!(expr("a <=> NULL"), "(<=> a NULL)");
}

#[test]
fn reads_the_predicates() {
    assert_eq!(expr("a IS NOT NULL"), "(notnull a)");
    assert_eq!(expr("a BETWEEN 1 AND 2 AND b"), "(and (between a 1 2) b)");
    assert_eq!(expr("a NOT IN (1, 2)"), "(notin a 1 2)");
    assert_eq!(expr("a LIKE 'x%'"), "(like a 'x%')");
    assert_eq!(expr("t.a + db.t.b"), "(+ t.a db.t.b)");
    assert_eq!(expr("'a' 'b'"), "'ab'");
}

#[test]
fn high_not_precedence_binds_not_tighter() {
    let mut options = mysql();
    options.mode = SqlMode::parse("HIGH_NOT_PRECEDENCE");
    assert_eq!(
        expr_with("NOT a BETWEEN 1 AND 2", &options),
        "(between (not a) 1 2)"
    );
    assert_eq!(expr("NOT a BETWEEN 1 AND 2"), "(not (between a 1 2))");
}

#[test]
fn reads_the_example_of_the_dialects() {
    let query = query(
        "WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds",
        &mysql(),
    );
    let with = query.with.unwrap();
    assert_eq!(with.ctes[0].name.value, "ds");
    assert_eq!(with.ctes[0].columns[0].value, "c");
    let QueryBody::Values(values) = &with.ctes[0].query.body else {
        panic!("VALUES expected");
    };
    assert_eq!(values.form, ValuesForm::Row);
    assert_eq!(values.rows.len(), 3);
}

#[test]
fn each_dialect_has_its_own_values() {
    let bare = "VALUES (1), (2)";
    let row = "VALUES ROW(1), ROW(2)";
    assert_eq!(
        error(bare, &mysql()),
        (
            "MySQL writes the rows of a `VALUES` statement `ROW (...)`".to_string(),
            "(".to_string()
        )
    );
    assert_eq!(
        error(row, &mariadb()).0,
        "`VALUES ROW` statement: absent from MariaDB"
    );
    let QueryBody::Values(values) = query(bare, &mariadb()).body else {
        panic!("VALUES expected");
    };
    assert_eq!(values.form, ValuesForm::Bare);
    // Before MySQL 8.0.19, no `VALUES` statement.
    let mut old = mysql();
    old.version = Version::new(8, 0, 18);
    assert_eq!(
        error(row, &old),
        (
            "`VALUES ROW` statement: from MySQL 8.0.19, chosen version 8.0.18".to_string(),
            "VALUES ROW".to_string()
        )
    );
}

#[test]
fn reads_aliases_with_their_form() {
    let select = select("SELECT a AS x, b y, c, 1 'one', t.* FROM db.t AS t");
    let aliases: Vec<Option<(String, AliasForm)>> = select
        .items
        .iter()
        .map(|item| match item {
            SelectItem::Expr { alias, .. } => alias
                .as_ref()
                .map(|alias| (alias.name.value.clone(), alias.form)),
            SelectItem::Wildcard { .. } => None,
        })
        .collect();
    assert_eq!(
        aliases,
        [
            Some(("x".to_string(), AliasForm::As)),
            Some(("y".to_string(), AliasForm::Bare)),
            None,
            Some(("one".to_string(), AliasForm::Bare)),
            None,
        ]
    );
    assert!(matches!(
        &select.items[4],
        SelectItem::Wildcard { qualifier: Some(name), .. } if name.parts[0].value == "t"
    ));
}

#[test]
fn reads_the_forms_of_limit() {
    let limit = |text: &str| query(text, &mysql()).limit.unwrap();
    let comma = limit("SELECT 1 LIMIT 10, 20");
    assert_eq!(comma.form, LimitForm::Comma);
    assert_eq!(tree(&comma.count), "20");
    assert_eq!(tree(comma.offset.as_ref().unwrap()), "10");
    let offset = limit("SELECT 1 LIMIT 20 OFFSET 10");
    assert_eq!(offset.form, LimitForm::Offset);
    assert_eq!(tree(&offset.count), "20");
}

#[test]
fn a_reserved_word_is_a_name_only_between_backticks_or_after_a_dot() {
    assert_eq!(error("SELECT order FROM t", &mysql()).1, "order");
    assert_eq!(expr("`order`"), "order");
    assert_eq!(expr("t.order"), "t.order");
}

#[test]
fn constructs_not_read_yet_are_explicit_errors() {
    for (text, faulty) in [
        ("SELECT COUNT(*) FROM t", "COUNT"),
        ("SELECT a FROM t GROUP BY a", "GROUP"),
        ("SELECT a FROM t JOIN u", "JOIN"),
        ("SELECT a FROM (SELECT 1) AS d", "("),
        ("SELECT 1 UNION SELECT 2", "UNION"),
        ("INSERT INTO t VALUES (1)", "INSERT"),
        ("SELECT @x", "@x"),
    ] {
        let (message, at) = error(text, &mysql());
        assert!(
            message.ends_with("not supported yet by the parser of Prisme"),
            "{text}: {message}"
        );
        assert_eq!(at, faulty, "{text}");
    }
}

#[test]
fn reports_syntax_errors_with_their_position() {
    assert_eq!(
        error("SELECT 1 +", &mysql()),
        (
            "expected an expression, found the end of the query".to_string(),
            String::new()
        )
    );
    assert_eq!(
        error("SELECT 1 2", &mysql()),
        (
            "expected the end of the query, found `2`".to_string(),
            "2".to_string()
        )
    );
    assert_eq!(error("SELECT 'a", &mysql()).0, "unterminated string");
}

#[test]
fn keeps_the_comments_and_the_executable_comments_apart() {
    let parsed = parse(
        "SELECT /*! 1 */ -- one\n/* two */ FROM DUAL /*!90000 three */",
        &mysql(),
    )
    .unwrap();
    let kinds: Vec<CommentKind> = parsed.comments.iter().map(|(_, kind)| *kind).collect();
    assert_eq!(
        kinds,
        [
            CommentKind::Dash,
            CommentKind::Block,
            CommentKind::Executable
        ]
    );
    assert_eq!(parsed.conditionals.len(), 1);
}

#[test]
fn refuses_a_query_nested_too_deeply() {
    let text = format!("SELECT {}1{}", "(".repeat(500), ")".repeat(500));
    assert_eq!(error(&text, &mysql()).0, "query nested too deeply");
}
