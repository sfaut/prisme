use prisme_plan::{
    AliasForm, CallForm, CastForm, CastTarget, CharTarget, CharWord, Charset, IndexHintAction,
    IndexHintScope, IntegerWord, JoinForm, LimitForm, ValuesForm,
};

use super::*;
use crate::sql::ast::{
    BinaryOp, Expr, ExprKind, FrameBound, FunctionCall, JoinConstraint, JoinKind, Literal,
    SetOperator, TableRef, UnaryOp, WindowSpec,
};

/// Settings of MySQL 8.0.19.
fn mysql() -> ParseOptions {
    ParseOptions {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        mode: SqlMode::default(),
        conditional_version: Version::new(8, 0, 19),
    }
}

/// Settings of MySQL in `version`.
fn mysql_at(major: u16, minor: u16, patch: u16) -> ParseOptions {
    let version = Version::new(major, minor, patch);
    ParseOptions {
        version,
        conditional_version: version,
        ..mysql()
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

/// Literal written for a prefix tree: bytes in hexadecimal after their form
/// (`HexNumber:41`), a character set before its literal (`_latin1'a'`).
fn literal_tree(literal: &Literal) -> String {
    match literal {
        Literal::Integer(text) | Literal::Decimal(text) | Literal::Float(text) => text.clone(),
        Literal::String(strings) => format!("'{}'", strings.value),
        Literal::National(strings) => format!("N'{}'", strings.value),
        Literal::Bytes { bytes, form } => {
            let digits: String = bytes.iter().map(|byte| format!("{byte:02X}")).collect();
            format!("{form:?}:{digits}")
        }
        Literal::Introduced { charset, literal } => format!("_{charset}{}", literal_tree(literal)),
        Literal::Null => "NULL".to_string(),
        Literal::True => "TRUE".to_string(),
        Literal::False => "FALSE".to_string(),
        Literal::Temporal { kind, value } => format!("{kind:?}'{value}'"),
    }
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
        ExprKind::Literal(literal) => literal_tree(literal),
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
            format!("({} {} {})", symbol(*op), tree(left), tree(right))
        }
        ExprKind::IsNull { operand, negated } => {
            format!(
                "({} {})",
                if *negated { "notnull" } else { "isnull" },
                tree(operand)
            )
        }
        ExprKind::IsTruth {
            operand,
            truth,
            negated,
        } => format!(
            "(is{} {truth:?} {})",
            if *negated { "not" } else { "" },
            tree(operand)
        ),
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
        ExprKind::Regexp {
            operand,
            pattern,
            negated,
        } => format!(
            "({}regexp {} {})",
            if *negated { "not" } else { "" },
            tree(operand),
            tree(pattern)
        ),
        ExprKind::SoundsLike { left, right } => {
            format!("(soundslike {} {})", tree(left), tree(right))
        }
        ExprKind::MemberOf { value, array } => {
            format!("(memberof {} {})", tree(value), tree(array))
        }
        ExprKind::Nested(inner) => tree(inner),
        ExprKind::Subquery(_) => "(query)".to_string(),
        ExprKind::Exists(_) => "(exists query)".to_string(),
        ExprKind::InSubquery {
            operand, negated, ..
        } => format!(
            "({}in {} query)",
            if *negated { "not" } else { "" },
            tree(operand)
        ),
        ExprKind::Quantified {
            op, operand, all, ..
        } => format!(
            "({} {} {} query)",
            symbol(*op),
            if *all { "all" } else { "any" },
            tree(operand)
        ),
        ExprKind::Row(values) => format!(
            "(row {})",
            values.iter().map(tree).collect::<Vec<_>>().join(" ")
        ),
        ExprKind::Function(call) => call_tree(call),
        ExprKind::Case {
            operand,
            branches,
            otherwise,
        } => {
            let mut parts = vec!["case".to_string()];
            parts.extend(operand.iter().map(|operand| tree(operand)));
            parts.extend(
                branches
                    .iter()
                    .map(|(when, then)| format!("(when {} {})", tree(when), tree(then))),
            );
            parts.extend(
                otherwise
                    .iter()
                    .map(|otherwise| format!("(else {})", tree(otherwise))),
            );
            format!("({})", parts.join(" "))
        }
        ExprKind::Cast(cast) => format!("(cast {} {:?})", tree(&cast.operand), cast.target),
        ExprKind::Extract(call) => format!("(Extract {:?} {})", call.unit, tree(&call.operand)),
        ExprKind::Trim(call) => {
            let mut parts = vec!["Trim".to_string()];
            parts.extend(call.side.map(|side| format!("{side:?}")));
            parts.extend(call.pattern.iter().map(tree));
            parts.push(tree(&call.operand));
            format!("({})", parts.join(" "))
        }
        ExprKind::Position(call) => format!(
            "(Position {} {})",
            tree(&call.substring),
            tree(&call.string)
        ),
        ExprKind::TimestampDiff(call) => format!(
            "(TimestampDiff {:?} {} {})",
            call.unit,
            tree(&call.start),
            tree(&call.end)
        ),
        ExprKind::DateArithmetic(call) => format!(
            "({:?} {} {} {:?})",
            call.form,
            tree(&call.operand),
            tree(&call.value),
            call.unit
        ),
    }
}

/// Target and form of the conversion `text`, with `options`.
fn cast_of(text: &str, options: &ParseOptions) -> (CastTarget, CastForm) {
    let QueryBody::Select(select) = query(&format!("SELECT {text}"), options).body else {
        panic!("not a SELECT");
    };
    match &select.items[0] {
        SelectItem::Expr {
            expr:
                Expr {
                    kind: ExprKind::Cast(cast),
                    ..
                },
            ..
        } => (cast.target.clone(), cast.form.clone()),
        other => panic!("{other:?}"),
    }
}

/// Character string target of `length`, `charset` and `binary`.
fn char_target(length: Option<u32>, charset: Option<Charset>, binary: bool) -> CastTarget {
    CastTarget::Char(CharTarget {
        word: CharWord::Char,
        length,
        charset,
        binary,
        collation: None,
    })
}

/// Character set `name` written after `CHARACTER SET`.
fn named(name: &str) -> Charset {
    Charset::Named {
        name: name.to_string(),
        short: false,
        quoted: false,
    }
}

/// Call written as a prefix tree: `(sum distinct a)`, `(count *)`, with its
/// window.
fn call_tree(call: &FunctionCall) -> String {
    let name = call
        .name
        .parts
        .iter()
        .map(|part| part.value.to_lowercase())
        .collect::<Vec<_>>()
        .join(".");
    let mut parts = vec![if call.stored {
        format!("stored {name}")
    } else {
        name
    }];
    if call.distinct {
        parts.push("distinct".to_string());
    }
    if call.star {
        parts.push("*".to_string());
    }
    parts.extend(call.args.iter().map(tree));
    match call.form {
        CallForm::Call => {}
        CallForm::Arrow => parts.push("(form ->)".to_string()),
        CallForm::LongArrow => parts.push("(form ->>)".to_string()),
    }
    if !call.order_by.is_empty() {
        let keys = call.order_by.iter().map(|item| tree(&item.expr));
        parts.push(format!("(order {})", keys.collect::<Vec<_>>().join(" ")));
    }
    if let Some(separator) = &call.separator {
        parts.push(format!("(separator '{separator}')"));
    }
    if let Some(over) = &call.over {
        parts.push(window_tree(over));
    }
    format!("({})", parts.join(" "))
}

/// Window written as a prefix tree: `(over w (partition a) (order b) rows)`.
fn window_tree(spec: &WindowSpec) -> String {
    let mut parts = vec!["over".to_string()];
    if let Some(base) = &spec.base {
        parts.push(base.value.clone());
    }
    if !spec.partition_by.is_empty() {
        let values = spec.partition_by.iter().map(tree);
        parts.push(format!(
            "(partition {})",
            values.collect::<Vec<_>>().join(" ")
        ));
    }
    if !spec.order_by.is_empty() {
        let keys = spec.order_by.iter().map(|item| tree(&item.expr));
        parts.push(format!("(order {})", keys.collect::<Vec<_>>().join(" ")));
    }
    if let Some(frame) = &spec.frame {
        let bound = |bound: &FrameBound| match bound {
            FrameBound::UnboundedPreceding => "unbounded-preceding".to_string(),
            FrameBound::Preceding(value) => format!("{}-preceding", tree(value)),
            FrameBound::CurrentRow => "current-row".to_string(),
            FrameBound::Following(value) => format!("{}-following", tree(value)),
            FrameBound::UnboundedFollowing => "unbounded-following".to_string(),
        };
        let units = match frame.units {
            prisme_plan::FrameUnits::Rows => "rows",
            prisme_plan::FrameUnits::Range => "range",
        };
        let mut text = format!("({units} {}", bound(&frame.start));
        if let Some(end) = &frame.end {
            text.push(' ');
            text.push_str(&bound(end));
        }
        text.push(')');
        parts.push(text);
    }
    format!("({})", parts.join(" "))
}

/// Symbol of a binary operator in the trees.
fn symbol(op: BinaryOp) -> &'static str {
    match op {
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
fn a_comparison_reads_tests_on_both_sides() {
    assert_eq!(expr("a = b BETWEEN 1 AND 2"), "(= a (between b 1 2))");
    assert_eq!(expr("a < b IN (1)"), "(< a (in b 1))");
    assert_eq!(expr("a IN (1) = b"), "(= (in a 1) b)");
    assert_eq!(expr("a = NULL IS NULL"), "(isnull (= a NULL))");
    // The high bound of `BETWEEN` is a test, its low bound a value in MySQL.
    assert_eq!(expr("a BETWEEN 0 AND b IN (1)"), "(between a 0 (in b 1))");
    assert_eq!(
        expr("a BETWEEN 0 AND b BETWEEN 0 AND 1"),
        "(between a 0 (between b 0 1))"
    );
    assert_eq!(expr("a BETWEEN 0 AND 1 = 0"), "(= (between a 0 1) 0)");
    assert_eq!(expr("a LIKE -1"), "(like a (- 1))");
    // MySQL reads a single test, a simple pattern.
    for text in [
        "SELECT a IN (1) IN (1)",
        "SELECT a LIKE 'a' LIKE 'b'",
        "SELECT a IN (1) BETWEEN 0 AND 1",
        "SELECT a BETWEEN 0 BETWEEN 0 AND 1 AND 2",
        "SELECT a LIKE 1 + 1",
        "SELECT a LIKE 'a' ESCAPE '!' + 1",
        "SELECT a IS NULL BETWEEN 0 AND 1",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
    // MariaDB chains them from left to right, its pattern at the level of
    // `|`, the bounds of `BETWEEN` tests.
    let mariadb = mariadb();
    assert_eq!(expr_with("a IN (1) IN (1)", &mariadb), "(in (in a 1) 1)");
    assert_eq!(
        expr_with("a LIKE 'a' BETWEEN 0 AND 2", &mariadb),
        "(between (like a 'a') 0 2)"
    );
    assert_eq!(
        expr_with("a BETWEEN 0 BETWEEN 0 AND 1 AND 2", &mariadb),
        "(between a (between 0 0 1) 2)"
    );
    assert_eq!(expr_with("a LIKE 1 + 1", &mariadb), "(like a (+ 1 1))");
}

#[test]
fn reads_the_json_arrows_after_a_column() {
    // A column, qualified or not, then a single string: `->` the
    // `JSON_EXTRACT` of both, `->>` its `JSON_UNQUOTE`, as simple as a
    // column (crates/probe/cases/json-arrows.sql).
    assert_eq!(expr("b->'$.a'"), "(json_extract b '$.a' (form ->))");
    assert_eq!(
        expr("t.b ->> \"$.a\""),
        "(json_unquote (json_extract t.b '$.a') (form ->>))"
    );
    assert_eq!(
        expr("-b->'$.n' + 1"),
        "(+ (- (json_extract b '$.n' (form ->))) 1)"
    );
    for text in [
        "SELECT b->_utf8mb4'$.a'",
        "SELECT b->N'$.a'",
        "SELECT b->X'24'",
        "SELECT (b)->'$.a'",
        "SELECT b->'$.n'->'$[0]'",
        "SELECT b->CONCAT('$', '.a')",
        "SELECT JSON_OBJECT('a', 1)->'$.a'",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
    assert_eq!(
        error("SELECT b->>'$.a'", &mariadb()),
        (
            "`->>` operator: absent from MariaDB".to_string(),
            "->>".to_string()
        )
    );
}

#[test]
fn reads_regexp_sounds_like_and_member_of() {
    assert_eq!(expr("a REGEXP 'x' AND b"), "(and (regexp a 'x') b)");
    assert_eq!(expr("a NOT RLIKE 'x'"), "(notregexp a 'x')");
    assert_eq!(expr("NOT a REGEXP 'x'"), "(not (regexp a 'x'))");
    // A pattern and the right operand of `SOUNDS LIKE` at the level of `|`,
    // tighter than a comparison.
    assert_eq!(expr("a REGEXP 1 + 1"), "(regexp a (+ 1 1))");
    assert_eq!(expr("a = b REGEXP 'x'"), "(= a (regexp b 'x'))");
    assert_eq!(expr("a REGEXP 'x' = 1"), "(= (regexp a 'x') 1)");
    assert_eq!(expr("a SOUNDS LIKE b | c"), "(soundslike a (| b c))");
    // The array of `MEMBER OF` a simple operand, `OF` optional.
    assert_eq!(expr("a + 0 MEMBER OF ('[1]')"), "(memberof (+ a 0) '[1]')");
    assert_eq!(expr("a MEMBER ((b))"), "(memberof a b)");
    assert_eq!(expr("a = b MEMBER OF (-1)"), "(= a (memberof b (- 1)))");
    for text in [
        "SELECT a REGEXP 'x' REGEXP 'y'",
        "SELECT a REGEXP 'x' IN (1)",
        "SELECT a SOUNDS LIKE b SOUNDS LIKE c",
        "SELECT a NOT SOUNDS LIKE b",
        "SELECT a sounds",
        "SELECT a MEMBER OF ('[1]') + 1",
        "SELECT a MEMBER OF '[1]'",
        "SELECT a MEMBER OF ('[1]' + 0)",
        "SELECT a NOT MEMBER OF ('[1]')",
        "SELECT a MEMBER OF ('[1]') IN (1)",
        "SELECT a member",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
    // MariaDB chains them.
    let mariadb = mariadb();
    assert_eq!(
        expr_with("a REGEXP 'x' REGEXP 'y'", &mariadb),
        "(regexp (regexp a 'x') 'y')"
    );
    assert_eq!(
        expr_with("a SOUNDS LIKE b IN (1)", &mariadb),
        "(in (soundslike a b) 1)"
    );
    // Before MySQL 8.0.17, and in MariaDB, `MEMBER` is an alias.
    let old = mysql_at(8, 0, 16);
    assert_eq!(
        error("SELECT a MEMBER OF ('[1]')", &old),
        (
            "`MEMBER OF`: from MySQL 8.0.17, chosen version 8.0.16".to_string(),
            "MEMBER".to_string()
        )
    );
    assert_eq!(error("SELECT a MEMBER ('[1]')", &mariadb).1, "MEMBER");
    for options in [old, mariadb] {
        assert!(parse("SELECT a member", &options).is_ok());
    }
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
    // A string after a column is its alias.
    let string_alias = self::select("SELECT a 'x y' FROM t");
    assert!(matches!(
        &string_alias.items[0],
        SelectItem::Expr { alias: Some(alias), .. } if alias.name.value == "x y"
    ));
    // A word `_` and a character set introduces the string; another word is
    // a column.
    let introduced = self::select("SELECT _utf8mb4'x', _foo 'y' FROM t");
    assert!(matches!(
        &introduced.items[0],
        SelectItem::Expr { alias: None, .. }
    ));
    assert!(matches!(
        &introduced.items[1],
        SelectItem::Expr { alias: Some(alias), .. } if alias.name.value == "y"
    ));
    assert!(matches!(
        &select.items[4],
        SelectItem::Wildcard { qualifier: Some(name), .. } if name.parts[0].value == "t"
    ));
}

#[test]
fn reads_national_hexadecimal_bit_and_introduced_literals() {
    for (text, expected) in [
        ("N'a' 'b'", "N'ab'"),
        ("n'a'", "N'a'"),
        ("_latin1'a' 'b'", "_latin1'ab'"),
        ("_UTF8MB4 'é'", "_utf8mb4'é'"),
        ("_binary /* bytes */ 'a' + 1", "(+ _binary'a' 1)"),
        ("X'41'", "HexString:41"),
        ("x''", "HexString:"),
        ("0x4", "HexNumber:04"),
        ("0x0102", "HexNumber:0102"),
        ("b'1'", "BitString:01"),
        ("0b101000001", "BitNumber:0141"),
        ("_UTF8MB4 X'C3A9'", "_utf8mb4HexString:C3A9"),
        ("_binary /* bytes */ b'1'", "_binaryBitString:01"),
        ("_binary 0x41 + 1", "(+ _binaryHexNumber:41 1)"),
    ] {
        assert_eq!(expr(text), expected, "{text}");
    }
    // `gb18030` in MySQL only, `filename` in MariaDB only.
    assert_eq!(expr("_gb18030'a'"), "_gb18030'a'");
    assert_eq!(expr_with("_filename'a'", &mariadb()), "_filename'a'");
    assert_eq!(
        error("SELECT _latin1 5", &mysql()),
        (
            "expected a string, a hexadecimal or a bit literal, found `5`".to_string(),
            "5".to_string()
        )
    );
    assert_eq!(
        error("SELECT _latin1 N'a'", &mysql()).0,
        "expected a string, a hexadecimal or a bit literal, found `N'a'`"
    );
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
fn reads_case_with_and_without_operand() {
    assert_eq!(
        expr("CASE WHEN a > 1 THEN 'x' WHEN a OR b THEN 'y' ELSE NULL END"),
        "(case (when (> a 1) 'x') (when (or a b) 'y') (else NULL))"
    );
    assert_eq!(
        expr("case a + 1 when 2 then b end + 1"),
        "(+ (case (+ a 1) (when 2 b)) 1)"
    );
    assert_eq!(
        expr("CASE WHEN a THEN CASE b WHEN 1 THEN 2 END END"),
        "(case (when a (case b (when 1 2))))"
    );
    assert_eq!(error("SELECT CASE a END", &mysql()).1, "END");
    assert_eq!(
        error("SELECT CASE WHEN a THEN 1", &mysql()).0,
        "expected `END`, found the end of the query"
    );
    assert_eq!(expr("`case`"), "case");
}

#[test]
fn reads_cast_and_convert_in_each_of_their_forms() {
    let mysql = mysql_at(8, 0, 22);
    for (text, target, form) in [
        (
            "CAST(a AS SIGNED)",
            CastTarget::Signed(None),
            CastForm::Cast,
        ),
        (
            "cast(a as unsigned integer)",
            CastTarget::Unsigned(Some(IntegerWord::Integer)),
            CastForm::Cast,
        ),
        (
            "CONVERT(a, DECIMAL(5, 2))",
            CastTarget::Decimal {
                dec: false,
                precision: Some(5),
                scale: Some(2),
            },
            CastForm::Convert,
        ),
        (
            "CAST(a AS DEC(7))",
            CastTarget::Decimal {
                dec: true,
                precision: Some(7),
                scale: None,
            },
            CastForm::Cast,
        ),
        (
            "CAST(a AS CHAR(10) CHARACTER SET utf8mb4 BINARY)",
            char_target(Some(10), Some(named("utf8mb4")), true),
            CastForm::Cast,
        ),
        (
            "CAST(a AS CHAR BINARY CHARSET 'latin1')",
            char_target(
                None,
                Some(Charset::Named {
                    name: "latin1".to_string(),
                    short: true,
                    quoted: true,
                }),
                true,
            ),
            CastForm::Cast,
        ),
        (
            "CAST(a AS CHAR ASCII)",
            char_target(None, Some(Charset::Ascii), false),
            CastForm::Cast,
        ),
        (
            "CONVERT(a USING utf8mb4)",
            char_target(None, Some(named("utf8mb4")), false),
            CastForm::ConvertUsing,
        ),
        (
            "CAST(a AS BINARY(4))",
            CastTarget::Binary(Some(4)),
            CastForm::Cast,
        ),
        (
            "CAST(a AS DATETIME(3))",
            CastTarget::DateTime(Some(3)),
            CastForm::Cast,
        ),
        ("CAST(a AS TIME)", CastTarget::Time(None), CastForm::Cast),
        ("CAST(a AS DATE)", CastTarget::Date, CastForm::Cast),
        (
            "CAST(a AS DOUBLE PRECISION)",
            CastTarget::Double {
                precision: true,
                digits: None,
            },
            CastForm::Cast,
        ),
        (
            "CAST(a AS FLOAT(30))",
            CastTarget::Float(Some(30)),
            CastForm::Cast,
        ),
        (
            "CAST(a AS REAL)",
            CastTarget::Real { float: false },
            CastForm::Cast,
        ),
        ("CAST(a AS YEAR)", CastTarget::Year, CastForm::Cast),
        ("CAST(a AS JSON)", CastTarget::Json, CastForm::Cast),
        (
            "CAST(a AT TIME ZONE INTERVAL '+00:00' AS DATETIME(6))",
            CastTarget::DateTime(Some(6)),
            CastForm::AtTimeZone {
                zone: "+00:00".to_string(),
                interval: true,
            },
        ),
    ] {
        assert_eq!(cast_of(text, &mysql), (target, form), "{text}");
    }
    for (text, target) in [
        ("CAST(a AS INT)", CastTarget::Integer(IntegerWord::Int)),
        (
            "CAST(a AS DOUBLE(5, 2))",
            CastTarget::Double {
                precision: false,
                digits: Some((5, 2)),
            },
        ),
        (
            "CAST(a AS VARCHAR(5) CHARACTER SET latin1 COLLATE latin1_bin)",
            CastTarget::Char(CharTarget {
                word: CharWord::VarChar,
                length: Some(5),
                charset: Some(named("latin1")),
                binary: false,
                collation: Some("latin1_bin".to_string()),
            }),
        ),
        ("CAST(a AS INTERVAL DAY_SECOND(2))", CastTarget::Interval(2)),
        ("CAST(a AS INET6)", CastTarget::Named("INET6".to_string())),
    ] {
        assert_eq!(cast_of(text, &mariadb()).0, target, "{text}");
    }
    assert_eq!(
        expr("CAST(a + 1 AS SIGNED) * 2"),
        "(* (cast (+ a 1) Signed(None)) 2)"
    );
}

#[test]
fn cast_after_a_space_or_between_backticks_calls_a_stored_function() {
    assert_eq!(expr("CAST (a)"), "(stored cast a)");
    assert_eq!(expr("`CAST`(a)"), "(stored cast a)");
    let ignore_space = ParseOptions {
        mode: SqlMode {
            ignore_space: true,
            ..SqlMode::default()
        },
        ..mysql()
    };
    assert_eq!(
        expr_with("CAST (a AS DATE)", &ignore_space),
        "(cast a Date)"
    );
    // `CONVERT` is a reserved word: a space changes nothing, a name is
    // between backticks.
    assert_eq!(expr("CONVERT (a, DATE)"), "(cast a Date)");
    assert_eq!(error("SELECT convert FROM t", &mysql()).1, "convert");
}

#[test]
fn a_target_of_cast_follows_the_dialect_and_its_version() {
    for (text, options, faulty) in [
        ("SELECT CAST(a AS DOUBLE)", mysql_at(8, 0, 16), "DOUBLE"),
        ("SELECT CAST(a AS YEAR)", mysql_at(8, 0, 21), "YEAR"),
        ("SELECT CAST(a AS INT)", mysql(), "INT"),
        ("SELECT CAST(a AS VARCHAR(4))", mysql(), "VARCHAR"),
        ("SELECT CAST(a AS JSON)", mariadb(), "JSON"),
        ("SELECT CAST(a AS DOUBLE PRECISION)", mariadb(), "PRECISION"),
        ("SELECT CAST(a AS NATIONAL CHAR)", mariadb(), "NATIONAL"),
        (
            "SELECT CAST(a AT TIME ZONE 'UTC' AS DATETIME)",
            mariadb(),
            "AT",
        ),
        ("SELECT CAST(a AS POINT)", mysql(), "POINT"),
        ("SELECT CAST(a AS NUMERIC)", mysql(), "NUMERIC"),
        ("SELECT CAST(a AS DATETIME(7))", mysql(), "7"),
        ("SELECT CAST(a AS DECIMAL(66))", mysql(), "66"),
        ("SELECT CAST(a AS DECIMAL(40, 31))", mysql(), "31"),
        ("SELECT CAST(a AS DECIMAL(5, 6))", mariadb(), "6"),
        ("SELECT CAST(a AS FLOAT(54))", mysql(), "54"),
        (
            "SELECT CAST(a AT TIME ZONE 'UTC' AS DATE)",
            mysql_at(8, 0, 22),
            "DATE",
        ),
        (
            "SELECT CAST(a AT TIME ZONE UTC AS DATETIME)",
            mysql_at(8, 0, 22),
            "UTC",
        ),
        ("SELECT CAST(a AS UNSIGNED ARRAY)", mysql(), "ARRAY"),
        ("SELECT CONVERT(a AS DATE)", mysql(), "AS"),
    ] {
        assert_eq!(error(text, &options).1, faulty, "{text}");
    }
    assert_eq!(
        cast_of("CAST(a AS DECIMAL(40, 38))", &mariadb()).0,
        CastTarget::Decimal {
            dec: false,
            precision: Some(40),
            scale: Some(38),
        }
    );
}

#[test]
fn a_word_of_value_calls_its_function_without_parentheses() {
    assert_eq!(expr("CURRENT_DATE - 1"), "(- (current_date) 1)");
    assert_eq!(expr("LOCALTIME"), "(localtime)");
    assert_eq!(expr("localtimestamp(3)"), "(localtimestamp 3)");
    assert_eq!(expr("`localtime`"), "localtime");
    assert_eq!(expr("t.localtime"), "t.localtime");
    // `CURRENT_ROLE` without parentheses in MariaDB only.
    assert_eq!(expr_with("CURRENT_ROLE", &mariadb()), "(current_role)");
    assert_eq!(expr("CURRENT_ROLE"), "CURRENT_ROLE");
}

#[test]
fn constructs_not_read_yet_are_explicit_errors() {
    for (text, faulty) in [
        ("SELECT WEIGHT_STRING(a AS CHAR(3)) FROM t", "WEIGHT_STRING"),
        ("SELECT a FROM t FOR UPDATE", "FOR"),
        ("SELECT GROUP_CONCAT(a LIMIT 1) FROM t", "LIMIT"),
        (
            "SELECT x FROM JSON_TABLE('[1]', '$[*]' COLUMNS (x INT PATH '$')) AS j",
            "JSON_TABLE",
        ),
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

/// Sources of the `FROM` of `text`, as a tree: `(a join b on c)`.
fn from(text: &str, options: &ParseOptions) -> String {
    let QueryBody::Select(select) = query(text, options).body else {
        panic!("not a SELECT");
    };
    table_tree(select.from.as_ref().expect("a FROM"))
}

/// Tree of a source of a `FROM`.
fn table_tree(table: &TableRef) -> String {
    match table {
        TableRef::Named { name, alias, .. } => {
            let name = name
                .parts
                .iter()
                .map(|part| part.value.as_str())
                .collect::<Vec<_>>()
                .join(".");
            match alias {
                Some(alias) => format!("{name} {}", alias.name.value),
                None => name,
            }
        }
        TableRef::Derived {
            lateral,
            alias,
            columns,
            ..
        } => {
            let columns = columns
                .iter()
                .map(|column| column.value.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            format!(
                "{}(query) {}{}",
                if *lateral { "lateral " } else { "" },
                alias.name.value,
                if columns.is_empty() {
                    String::new()
                } else {
                    format!(" ({columns})")
                }
            )
        }
        TableRef::Join(join) => {
            let operator = match (join.kind, join.form) {
                (_, JoinForm::Comma) => ",",
                (JoinKind::Inner, JoinForm::Straight) => "straight",
                (JoinKind::Inner, _) => "join",
                (JoinKind::Left, _) => "left",
                (JoinKind::Right, _) => "right",
            };
            let natural = if join.natural { "natural " } else { "" };
            let constraint = match &join.constraint {
                Some(JoinConstraint::On(condition)) => format!(" on {}", tree(condition)),
                Some(JoinConstraint::Using(names)) => format!(
                    " using {}",
                    names
                        .iter()
                        .map(|name| name.value.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                None => String::new(),
            };
            format!(
                "({} {natural}{operator} {}{constraint})",
                table_tree(&join.left),
                table_tree(&join.right)
            )
        }
        TableRef::Nested { inner, .. } => format!("[{}]", table_tree(inner)),
    }
}

/// Body of `text` as a tree: `(union a b)`, the `SELECT` blocks named by
/// their first item.
fn body(text: &str, options: &ParseOptions) -> String {
    body_tree(&query(text, options).body)
}

/// Tree of a query body.
fn body_tree(body: &QueryBody) -> String {
    match body {
        QueryBody::Select(select) => match &select.items[0] {
            SelectItem::Expr { expr, .. } => tree(expr),
            SelectItem::Wildcard { .. } => "*".to_string(),
        },
        QueryBody::Values(_) => "values".to_string(),
        QueryBody::Table(name) => format!("table {}", name.parts[0].value),
        QueryBody::Nested(query) => format!("[{}]", body_tree(&query.body)),
        QueryBody::SetOperation(operation) => {
            let operator = match operation.operator {
                SetOperator::Union => "union",
                SetOperator::Except => "except",
                SetOperator::Intersect => "intersect",
            };
            format!(
                "({operator}{} {} {})",
                if operation.all { " all" } else { "" },
                body_tree(&operation.left),
                body_tree(&operation.right)
            )
        }
    }
}

#[test]
fn the_comma_joins_loosest_and_a_join_reads_its_right_side_greedily() {
    let options = mysql();
    for (text, expected) in [
        ("SELECT 1 FROM a, b", "(a , b)"),
        ("SELECT 1 FROM a, b, c", "((a , b) , c)"),
        ("SELECT 1 FROM a, b JOIN c ON x", "(a , (b join c on x))"),
        (
            "SELECT 1 FROM a JOIN b ON x JOIN c ON y",
            "((a join b on x) join c on y)",
        ),
        (
            "SELECT 1 FROM a JOIN b JOIN c ON x ON y",
            "(a join (b join c on x) on y)",
        ),
        (
            "SELECT 1 FROM a JOIN b JOIN c ON x",
            "((a join b) join c on x)",
        ),
        (
            "SELECT 1 FROM a CROSS JOIN b JOIN c ON x",
            "((a join b) join c on x)",
        ),
        (
            "SELECT 1 FROM a LEFT JOIN b USING (k, j)",
            "(a left b using k j)",
        ),
        (
            "SELECT 1 FROM a RIGHT OUTER JOIN b ON x",
            "(a right b on x)",
        ),
        (
            "SELECT 1 FROM a NATURAL JOIN b JOIN c ON x",
            "((a natural join b) join c on x)",
        ),
        ("SELECT 1 FROM a NATURAL LEFT JOIN b", "(a natural left b)"),
        (
            "SELECT 1 FROM a STRAIGHT_JOIN b ON x",
            "(a straight b on x)",
        ),
        (
            "SELECT 1 FROM (a, b) JOIN c ON x",
            "([(a , b)] join c on x)",
        ),
        ("SELECT 1 FROM a AS t1 JOIN b t2", "(a t1 join b t2)"),
    ] {
        assert_eq!(from(text, &options), expected, "{text}");
    }
}

#[test]
fn an_outer_join_needs_its_condition() {
    assert_eq!(
        error("SELECT 1 FROM a LEFT JOIN b", &mysql()).0,
        "expected `ON` or `USING`, found the end of the query"
    );
    assert_eq!(
        error("SELECT 1 FROM a STRAIGHT_JOIN b USING (k)", &mysql()).1,
        "USING"
    );
}

#[test]
fn reads_the_derived_tables() {
    let options = mysql();
    assert_eq!(from("SELECT 1 FROM (SELECT 1) AS d", &options), "(query) d");
    assert_eq!(
        from("SELECT 1 FROM ((SELECT 1)) d (x)", &options),
        "(query) d (x)"
    );
    assert_eq!(
        from("SELECT 1 FROM a, LATERAL (SELECT a.k) AS d", &options),
        "(a , lateral (query) d)"
    );
    assert_eq!(
        error("SELECT 1 FROM (SELECT 1)", &options),
        (
            "every derived table must have its own alias".to_string(),
            "(SELECT 1)".to_string()
        )
    );
    let (message, at) = error(
        "SELECT 1 FROM a, LATERAL (SELECT 1) AS d",
        &mysql_at(8, 0, 13),
    );
    assert!(message.contains("8.0.14"), "{message}");
    assert_eq!(at, "LATERAL");
    let (message, _) = error("SELECT 1 FROM (SELECT 1) AS d (x)", &mariadb());
    assert!(message.contains("11.7.1"), "{message}");
}

#[test]
fn reads_the_partitions_and_the_index_hints() {
    let QueryBody::Select(select) = query(
        "SELECT 1 FROM t PARTITION (p0, p1) AS u USE INDEX () FORCE KEY FOR ORDER BY (PRIMARY, i)",
        &mysql(),
    )
    .body
    else {
        panic!("not a SELECT");
    };
    let Some(TableRef::Named {
        partitions,
        alias,
        index_hints,
        ..
    }) = &select.from
    else {
        panic!("not a table");
    };
    assert_eq!(partitions.len(), 2);
    assert_eq!(
        alias.as_ref().map(|alias| alias.name.value.as_str()),
        Some("u")
    );
    assert_eq!(index_hints.len(), 2);
    assert_eq!(index_hints[0].action, IndexHintAction::Use);
    assert!(index_hints[0].indexes.is_empty());
    assert_eq!(index_hints[1].action, IndexHintAction::Force);
    assert!(index_hints[1].key);
    assert_eq!(index_hints[1].scope, Some(IndexHintScope::OrderBy));
    assert_eq!(index_hints[1].indexes[0].value, "PRIMARY");
}

#[test]
fn intersect_binds_tighter_than_union_and_except() {
    let options = mysql_at(8, 0, 31);
    for (text, expected) in [
        (
            "SELECT 1 UNION SELECT 2 UNION ALL SELECT 3",
            "(union all (union 1 2) 3)",
        ),
        (
            "SELECT 1 UNION SELECT 2 INTERSECT SELECT 3",
            "(union 1 (intersect 2 3))",
        ),
        (
            "SELECT 1 INTERSECT SELECT 2 EXCEPT SELECT 3",
            "(except (intersect 1 2) 3)",
        ),
        (
            "SELECT 1 UNION DISTINCT (SELECT 2 UNION SELECT 3)",
            "(union 1 [(union 2 3)])",
        ),
        ("TABLE t UNION VALUES ROW(1)", "(union table t values)"),
    ] {
        assert_eq!(body(text, &options), expected, "{text}");
    }
}

#[test]
fn set_operations_follow_the_versions() {
    // `INTERSECT` is reserved from MySQL 8.0.31 on, an alias before.
    assert_eq!(error("SELECT 1 INTERSECT SELECT 2", &mysql()).1, "SELECT");
    assert_eq!(
        body("SELECT 1 EXCEPT ALL SELECT 2", &mariadb()),
        "(except all 1 2)"
    );
    let (message, at) = error(
        "SELECT 1 UNION (SELECT 2 UNION SELECT 3)",
        &mysql_at(8, 0, 30),
    );
    assert!(message.contains("8.0.31"), "{message}");
    assert_eq!(at, "SELECT 2 UNION SELECT 3");
    assert_eq!(error("TABLE t", &mariadb()).1, "TABLE");
}

#[test]
fn reads_the_subqueries_and_the_rows() {
    for (text, expected) in [
        ("(SELECT 1)", "(query)"),
        ("((SELECT 1)) + 1", "(+ (query) 1)"),
        ("a IN (SELECT 1)", "(in a query)"),
        ("a NOT IN (VALUES ROW(1))", "(notin a query)"),
        ("NOT EXISTS (SELECT 1)", "(not (exists query))"),
        ("a > ALL (SELECT 1)", "(> all a query)"),
        ("a = SOME (TABLE t)", "(= any a query)"),
        ("(a, b) = ROW(1, 2)", "(= (row a b) (row 1 2))"),
        (
            "(a, (b, c)) IN ((1, (2, 3)))",
            "(in (row a (row b c)) (row 1 (row 2 3)))",
        ),
    ] {
        assert_eq!(expr(text), expected, "{text}");
    }
    assert_eq!(error("SELECT ROW(1)", &mysql()).1, ")");
    assert_eq!(error("SELECT 1 <=> ANY (SELECT 1)", &mysql()).1, "ANY");
}

#[test]
fn reads_the_function_calls_and_the_windows() {
    for (text, expected) in [
        ("COUNT(*)", "(count *)"),
        ("count(DISTINCT a, b)", "(count distinct a b)"),
        ("SUM(ALL a)", "(sum a)"),
        ("SUM (a)", "(stored sum a)"),
        ("AVG (a)", "(avg a)"),
        ("db.f(a, 1)", "(stored db.f a 1)"),
        ("LEFT(a, 1)", "(left a 1)"),
        ("LEFT (a, 1)", "(left a 1)"),
        ("`LEFT`(a, 1)", "(stored left a 1)"),
        ("`ABS`(a)", "(abs a)"),
        ("NOW()", "(now)"),
        ("NOW ()", "(stored now)"),
        ("f(a) + 1", "(+ (stored f a) 1)"),
        // `FORMAT_PICO_TIME` appeared in MySQL 8.0.16.
        ("FORMAT_PICO_TIME(a)", "(format_pico_time a)"),
        (
            "GROUP_CONCAT(DISTINCT a ORDER BY b DESC SEPARATOR ';')",
            "(group_concat distinct a (order b) (separator ';'))",
        ),
        ("ROW_NUMBER() OVER ()", "(row_number (over))"),
        ("SUM(a) OVER w", "(sum a (over w))"),
        ("RANK() OVER (w ORDER BY a)", "(rank (over w (order a)))"),
        (
            "SUM(a) OVER (PARTITION BY b, c ORDER BY d ROWS BETWEEN 1 PRECEDING AND CURRENT ROW)",
            "(sum a (over (partition b c) (order d) (rows 1-preceding current-row)))",
        ),
        (
            "AVG(a) OVER (RANGE UNBOUNDED PRECEDING)",
            "(avg a (over (range unbounded-preceding)))",
        ),
    ] {
        assert_eq!(expr(text), expected, "{text}");
    }
    assert_eq!(error("SELECT COUNT (*)", &mysql()).1, "*");
    let ignore_space = ParseOptions {
        mode: SqlMode {
            ignore_space: true,
            ..SqlMode::default()
        },
        ..mysql()
    };
    assert_eq!(expr_with("SUM (a)", &ignore_space), "(sum a)");
    assert_eq!(
        expr_with("FORMAT_PICO_TIME(a)", &mysql_at(8, 0, 11)),
        "(stored format_pico_time a)"
    );
    // MariaDB reads `RANK` followed by a space as a stored function.
    assert_eq!(expr_with("RANK ()", &mariadb()), "(stored rank)");
    assert_eq!(error("SELECT f(a) OVER ()", &mysql()).1, "OVER");
    assert_eq!(error("SELECT RANK () OVER ()", &mariadb()).1, "OVER");
    assert_eq!(
        error("SELECT LAG(a) RESPECT NULLS OVER ()", &mysql()).1,
        "RESPECT"
    );
    assert_eq!(
        error(
            "SELECT PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY a)",
            &mariadb()
        )
        .1,
        "WITHIN"
    );
}

#[test]
fn reads_the_grouping_and_the_named_windows() {
    let select = select(
        "SELECT a, SUM(b) FROM t GROUP BY a, 2 WITH ROLLUP HAVING SUM(b) > 1 \
         WINDOW w AS (ORDER BY a), v AS (w ROWS 2 PRECEDING)",
    );
    assert_eq!(
        select.group_by.iter().map(tree).collect::<Vec<_>>(),
        ["a", "2"]
    );
    assert!(select.rollup);
    assert_eq!(
        select.having.as_ref().map(tree).as_deref(),
        Some("(> (sum b) 1)")
    );
    let windows: Vec<String> = select
        .windows
        .iter()
        .map(|window| format!("{} {}", window.name.value, window_tree(&window.spec)))
        .collect();
    assert_eq!(
        windows,
        ["w (over (order a))", "v (over w (rows 2-preceding))"]
    );
    assert_eq!(error("SELECT a FROM t GROUP BY a DESC", &mysql()).1, "DESC");
    assert!(
        query(
            "SELECT a FROM t GROUP BY a WITH ROLLUP ORDER BY a",
            &mysql()
        )
        .limit
        .is_none()
    );
    assert_eq!(
        error(
            "SELECT a FROM t GROUP BY a WITH ROLLUP ORDER BY a",
            &mysql_at(8, 0, 11)
        )
        .1,
        "ORDER"
    );
}

#[test]
fn reads_typed_literals_and_their_odbc_form() {
    assert_eq!(expr("DATE '2024-05-06'"), "Date'2024-05-06'");
    assert_eq!(expr("time '07:08:09.5'"), "Time'07:08:09.5'");
    assert_eq!(
        expr("TIMESTAMP '2024-05-06 07:08:09'"),
        "Timestamp'2024-05-06 07:08:09'"
    );
    assert_eq!(expr("{d '2024-05-06'}"), "Date'2024-05-06'");
    assert_eq!(expr("{ t '07:08:09' }"), "Time'07:08:09'");
    assert_eq!(
        expr("{ts '2024-05-06 07:08:09'}"),
        "Timestamp'2024-05-06 07:08:09'"
    );
    // The ODBC form gives its string when the dialect reads no date in it.
    assert_eq!(expr("{D '2024-05-06'}"), "'2024-05-06'");
    assert_eq!(expr("{x '2024-05-06'}"), "'2024-05-06'");
    assert_eq!(expr("{d '2024-02-30'}"), "'2024-02-30'");
    assert_eq!(expr("{d 'x' 'y'}"), "'xy'");
    assert_eq!(expr("{t '838:59:59.5'}"), "'838:59:59.5'");
    assert_eq!(
        expr_with("{t '838:59:59.5'}", &mariadb()),
        "Time'838:59:59.5'"
    );
    let strict = ParseOptions {
        mode: SqlMode::parse("NO_ZERO_DATE,NO_ZERO_IN_DATE"),
        ..mysql()
    };
    assert_eq!(expr_with("{d '0000-00-00'}", &strict), "'0000-00-00'");
    assert_eq!(expr_with("{d '2024-05-00'}", &strict), "'2024-05-00'");
    assert_eq!(expr("{d '0000-00-00'}"), "Date'0000-00-00'");
    // An offset from MySQL 8.0.19.
    let offset = "{ts '2024-05-06 01:00:00+02:00'}";
    assert_eq!(expr(offset), "Timestamp'2024-05-06 01:00:00+02:00'");
    assert_eq!(
        expr_with(offset, &mysql_at(8, 0, 18)),
        "'2024-05-06 01:00:00+02:00'"
    );
    assert_eq!(expr_with(offset, &mariadb()), "'2024-05-06 01:00:00+02:00'");
    // MariaDB refuses the offset as an invalid date, MySQL before 8.0.19 as
    // an absent construct.
    assert_eq!(
        expr_with("TIMESTAMP '2024-05-06 01:00:00+02:00'", &mariadb()),
        "Timestamp'2024-05-06 01:00:00+02:00'"
    );
    let (message, at) = error(
        "SELECT TIMESTAMP '2024-05-06 01:00:00+02:00'",
        &mysql_at(8, 0, 18),
    );
    assert!(
        message.contains("time zone offset") && message.contains("8.0.19"),
        "{message}"
    );
    assert_eq!(at, "'2024-05-06 01:00:00+02:00'");
    // `DATETIME` is a column, and a second string an alias.
    let alias = self::select("SELECT DATETIME '2024-05-06', DATE '2024-05' '-06'");
    assert!(matches!(
        &alias.items[0],
        SelectItem::Expr { expr, alias: Some(alias), .. }
            if tree(expr) == "DATETIME" && alias.name.value == "2024-05-06"
    ));
    assert!(matches!(
        &alias.items[1],
        SelectItem::Expr { expr, alias: Some(alias), .. }
            if tree(expr) == "Date'2024-05'" && alias.name.value == "-06"
    ));
    // A form Prisme does not read, and the other ODBC escapes.
    for (text, faulty) in [
        ("SELECT {d '2024-05-06T'}", "{d '2024-05-06T'}"),
        ("SELECT {fn NOW()}", "{"),
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
fn reads_intervals_with_the_precedence_of_the_servers() {
    assert_eq!(expr("d + INTERVAL 1 DAY"), "(Plus d 1 Day)");
    assert_eq!(
        expr("d - interval '1:30' hour_minute"),
        "(Minus d '1:30' HourMinute)"
    );
    assert_eq!(expr("d + INTERVAL 1 + 1 DAY"), "(Plus d (+ 1 1) Day)");
    assert_eq!(expr("d + INTERVAL 1 OR 0 DAY"), "(Plus d (or 1 0) Day)");
    assert_eq!(expr("d + INTERVAL 1 SQL_TSI_WEEK"), "(Plus d 1 Week)");
    // The sum closes at its unit, then takes the operators of a higher level.
    assert_eq!(expr("d + INTERVAL 1 DAY * 2"), "(* (Plus d 1 Day) 2)");
    assert_eq!(
        expr("d + INTERVAL 1 DAY ^ 1 * 2"),
        "(* (^ (Plus d 1 Day) 1) 2)"
    );
    assert_eq!(
        expr("0 | d + INTERVAL 1 DAY * 2 + 1"),
        "(| 0 (+ (* (Plus d 1 Day) 2) 1))"
    );
    assert_eq!(expr("2 * d + INTERVAL 1 DAY"), "(Plus (* 2 d) 1 Day)");
    assert_eq!(
        expr("d - INTERVAL 1 DAY - INTERVAL 1 HOUR"),
        "(Minus (Minus d 1 Day) 1 Hour)"
    );
    // The interval first: its operand runs to the next `AND`, `OR` or `XOR`.
    assert_eq!(
        expr("INTERVAL 1 DAY + d = e"),
        "(IntervalFirst (= d e) 1 Day)"
    );
    assert_eq!(
        expr("INTERVAL 1 DAY + d IS NULL AND 1"),
        "(and (IntervalFirst (isnull d) 1 Day) 1)"
    );
    assert_eq!(
        expr("2 * INTERVAL 1 DAY + d"),
        "(* 2 (IntervalFirst d 1 Day))"
    );
    assert_eq!(expr("- INTERVAL 1 DAY + d"), "(- (IntervalFirst d 1 Day))");
    assert_eq!(
        expr("d + INTERVAL INTERVAL 1 DAY + 1 DAY"),
        "(Plus d (IntervalFirst 1 1 Day) Day)"
    );
    // `INTERVAL(n, n1, ...)` is a function.
    assert_eq!(expr("INTERVAL(5, 1, 10)"), "(interval 5 1 10)");
    assert_eq!(expr("d + INTERVAL (5, 1)"), "(+ d (interval 5 1))");
    assert_eq!(expr("d + INTERVAL (1) DAY"), "(Plus d 1 Day)");
    // The functions.
    assert_eq!(expr("DATE_ADD(d, INTERVAL 1 DAY)"), "(DateAdd d 1 Day)");
    assert_eq!(
        expr("date_sub(d, INTERVAL 1 YEAR_MONTH)"),
        "(DateSub d 1 YearMonth)"
    );
    assert_eq!(expr("ADDDATE(d, 1)"), "(AddDays d 1 Day)");
    assert_eq!(expr("SUBDATE(d, INTERVAL 1 MONTH)"), "(SubDate d 1 Month)");
    assert_eq!(
        expr("ADDDATE(d, INTERVAL 1 DAY + 1)"),
        "(AddDays d (IntervalFirst 1 1 Day) Day)"
    );
    assert_eq!(
        expr("TIMESTAMPADD(SQL_TSI_HOUR, 1, d)"),
        "(TimestampAdd d 1 Hour)"
    );
    assert_eq!(
        expr("TIMESTAMPADD (MICROSECOND, 1, d)"),
        "(TimestampAdd d 1 Microsecond)"
    );
    // Forms the servers refuse.
    for text in [
        "SELECT INTERVAL 1 DAY",
        "SELECT INTERVAL 1 DAY - d",
        "SELECT d * INTERVAL 1 DAY",
        "SELECT d + - INTERVAL 1 DAY",
        "SELECT (INTERVAL 1 DAY) + d",
        "SELECT d + INTERVAL 1 DAYS",
        "SELECT d + INTERVAL 1 'DAY'",
        "SELECT d + INTERVAL 1 SQL_TSI_MICROSECOND",
        "SELECT d + INTERVAL 1 SQL_TSI_DAY_HOUR",
        "SELECT DATE_ADD(d, 1)",
        "SELECT DATE_ADD(d, INTERVAL 1 DAY + 1)",
        "SELECT DATE_ADD (d, INTERVAL 1 DAY)",
        "SELECT TIMESTAMPADD(DAY_HOUR, 1, d)",
        "SELECT TIMESTAMPADD(FRAC_SECOND, 1, d)",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
    // MariaDB reads `- INTERVAL 1 DAY + d` as `d - INTERVAL 1 DAY`, and
    // refuses `~` and `!` before an interval.
    assert_eq!(
        error("SELECT - INTERVAL 1 DAY + d", &mariadb()).0,
        "`- INTERVAL ... + x`, which MariaDB reads as `x - INTERVAL ...`: not supported yet by the parser of Prisme"
    );
    assert!(parse("SELECT ~ INTERVAL 1 DAY + d", &mariadb()).is_err());
    assert_eq!(
        expr_with("+ INTERVAL 1 DAY + d", &mariadb()),
        "(+ (IntervalFirst d 1 Day))"
    );
}

#[test]
fn reads_timestampdiff_of_a_simple_unit() {
    assert_eq!(
        expr("TIMESTAMPDIFF(DAY, d, e + 1)"),
        "(TimestampDiff Day d (+ e 1))"
    );
    assert_eq!(
        expr("timestampdiff (sql_tsi_month, d, e)"),
        "(TimestampDiff Month d e)"
    );
    for text in [
        "SELECT TIMESTAMPDIFF(DAY_HOUR, d, e)",
        "SELECT TIMESTAMPDIFF('DAY', d, e)",
        "SELECT TIMESTAMPDIFF(FRAC_SECOND, d, e)",
        "SELECT TIMESTAMPDIFF(DAY, d)",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
}

#[test]
fn reads_extract_of_a_simple_or_a_compound_unit() {
    assert_eq!(expr("EXTRACT(DAY FROM d + 1)"), "(Extract Day (+ d 1))");
    assert_eq!(expr("extract(sql_tsi_month from d)"), "(Extract Month d)");
    assert_eq!(
        expr("EXTRACT(DAY_MICROSECOND FROM d)"),
        "(Extract DayMicrosecond d)"
    );
    for text in [
        "SELECT EXTRACT('DAY' FROM d)",
        "SELECT EXTRACT(DAY, d)",
        "SELECT EXTRACT(FRAC_SECOND FROM d)",
        "SELECT EXTRACT(SQL_TSI_DAY_HOUR FROM d)",
        "SELECT EXTRACT(SQL_TSI_MICROSECOND FROM d)",
        "SELECT EXTRACT (DAY FROM d)",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
}

#[test]
fn reads_trim_and_position_in_their_forms() {
    assert_eq!(expr("TRIM(s)"), "(Trim s)");
    assert_eq!(expr("trim(both from s)"), "(Trim Both s)");
    assert_eq!(expr("TRIM('x' FROM s)"), "(Trim 'x' s)");
    assert_eq!(
        expr("TRIM(LEADING 'x' FROM CONCAT(s, 'x'))"),
        "(Trim Leading 'x' (concat s 'x'))"
    );
    assert_eq!(expr("TRIM(TRAILING FROM s)"), "(Trim Trailing s)");
    // The pattern and the string are whole expressions.
    assert_eq!(
        expr("TRIM('a' = 'a' FROM s = 'x')"),
        "(Trim (= 'a' 'a') (= s 'x'))"
    );
    assert_eq!(expr("POSITION('b' IN s)"), "(Position 'b' s)");
    assert_eq!(
        expr("position(1 + 1 in s = 'a')"),
        "(Position (+ 1 1) (= s 'a'))"
    );
    for text in [
        "SELECT TRIM(BOTH)",
        "SELECT TRIM(BOTH 'x' s)",
        "SELECT TRIM(FROM s)",
        "SELECT TRIM('x' FROM)",
        "SELECT POSITION('a' = 'a' IN s)",
        "SELECT POSITION('a', s)",
        "SELECT POSITION('a' IN s, 2)",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
    // A space after the name calls a stored function.
    assert_eq!(expr("TRIM (s)"), "(stored trim s)");
}

#[test]
fn reads_is_true_is_false_and_is_unknown() {
    assert_eq!(expr("a IS TRUE"), "(is True a)");
    assert_eq!(expr("a is not false"), "(isnot False a)");
    assert_eq!(expr("a IS UNKNOWN"), "(is Unknown a)");
    // Looser than the comparisons and `IS NULL`, tighter than `NOT` and
    // `AND`.
    assert_eq!(expr("a = b IS TRUE"), "(is True (= a b))");
    assert_eq!(expr("a IS NULL IS FALSE"), "(is False (isnull a))");
    assert_eq!(expr("NOT a IS TRUE"), "(not (is True a))");
    assert_eq!(expr("a AND b IS FALSE"), "(and a (is False b))");
    assert_eq!(expr("!a IS TRUE"), "(is True (not a))");
    // MySQL ends the expression there, MariaDB goes on.
    for text in [
        "SELECT a IS TRUE = 1",
        "SELECT a IS TRUE IS TRUE",
        "SELECT a IS TRUE IS NULL",
        "SELECT a IS NOT NOT TRUE",
        "SELECT a IS TRUE BETWEEN 0 AND 1",
    ] {
        assert!(parse(text, &mysql()).is_err(), "{text}");
    }
    let mariadb = mariadb();
    assert_eq!(expr_with("a IS TRUE = 1", &mariadb), "(= (is True a) 1)");
    assert_eq!(
        expr_with("a IS TRUE IS NULL", &mariadb),
        "(isnull (is True a))"
    );
    assert!(parse("SELECT a IS TRUE IN (1)", &mariadb).is_err());
}

#[test]
fn reads_with_recursive_and_refuses_cycle() {
    let text = "WITH RECURSIVE r (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 3) \
                SELECT n FROM r";
    for options in [mysql(), mariadb()] {
        let with = query(text, &options).with.expect("a `WITH`");
        assert!(with.recursive);
        assert_eq!(with.ctes.len(), 1);
    }
    let cycle = "WITH RECURSIVE r (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 3) \
                 CYCLE n RESTRICT SELECT n FROM r";
    assert_eq!(
        error(cycle, &mariadb()),
        (
            "`CYCLE`: not supported yet by the parser of Prisme".to_string(),
            "CYCLE".to_string()
        )
    );
}

/// Options of the first `SELECT` of `text`, in order, and its `DISTINCT`.
fn options_of(text: &str, options: &ParseOptions) -> (Vec<&'static str>, bool) {
    let mut body = query(text, options).body;
    loop {
        body = match body {
            QueryBody::Select(select) => {
                let keywords = (select.options.iter())
                    .map(|written| written.option.keyword())
                    .collect();
                return (keywords, select.distinct);
            }
            QueryBody::SetOperation(operation) => operation.left,
            QueryBody::Nested(query) => query.body,
            other => panic!("{text}: {other:?}"),
        };
    }
}

#[test]
fn reads_the_options_of_select_in_any_order() {
    // Each option once, in the order written, `ALL`, `DISTINCT` and
    // `DISTINCTROW` among them.
    for (text, expected, distinct) in [
        ("SELECT SQL_NO_CACHE a FROM t", vec!["SQL_NO_CACHE"], false),
        (
            "SELECT DISTINCT SQL_CALC_FOUND_ROWS HIGH_PRIORITY a FROM t LIMIT 1",
            vec!["SQL_CALC_FOUND_ROWS", "HIGH_PRIORITY"],
            true,
        ),
        (
            "SELECT SQL_BUFFER_RESULT STRAIGHT_JOIN SQL_SMALL_RESULT DISTINCTROW a FROM t",
            vec!["SQL_BUFFER_RESULT", "STRAIGHT_JOIN", "SQL_SMALL_RESULT"],
            true,
        ),
        (
            "SELECT SQL_BIG_RESULT ALL sql_no_cache a FROM t",
            vec!["SQL_BIG_RESULT", "SQL_NO_CACHE"],
            false,
        ),
        (
            "SELECT HIGH_PRIORITY HIGH_PRIORITY STRAIGHT_JOIN a FROM t",
            vec!["HIGH_PRIORITY", "STRAIGHT_JOIN"],
            false,
        ),
        (
            "SELECT SQL_SMALL_RESULT SQL_BIG_RESULT a FROM t GROUP BY a",
            vec!["SQL_SMALL_RESULT", "SQL_BIG_RESULT"],
            false,
        ),
        ("SELECT DISTINCT DISTINCTROW a FROM t", vec![], true),
        ("SELECT ALL ALL a FROM t", vec![], false),
    ] {
        for options in [mysql(), mariadb()] {
            assert_eq!(
                options_of(text, &options),
                (expected.clone(), distinct),
                "{text}"
            );
        }
    }
    // `ALL` with `DISTINCT`, error 1221 of both.
    for text in [
        "SELECT ALL DISTINCT a FROM t",
        "SELECT DISTINCTROW ALL a FROM t",
        "SELECT ALL SQL_NO_CACHE DISTINCT a FROM t",
    ] {
        for options in [mysql(), mariadb()] {
            assert_eq!(
                error(text, &options).0,
                "incorrect usage of `ALL` and `DISTINCT`",
                "{text}"
            );
        }
    }
    // An option alone is not a name; after a table or between backticks, it
    // is.
    for options in [mysql(), mariadb()] {
        assert!(parse("SELECT sql_no_cache FROM t", &options).is_err());
        let counts = |text: &str| match query(text, &options).body {
            QueryBody::Select(select) => (select.options.len(), select.items.len()),
            other => panic!("{other:?}"),
        };
        assert_eq!(counts("SELECT t.sql_no_cache FROM t"), (0, 1));
        assert_eq!(counts("SELECT `sql_no_cache` FROM t"), (0, 1));
    }
}

#[test]
fn sql_cache_is_an_option_of_mariadb_and_a_name_of_mysql() {
    assert_eq!(
        options_of("SELECT SQL_CACHE a FROM t", &mariadb()),
        (vec!["SQL_CACHE"], false)
    );
    // MySQL reads the column `SQL_CACHE` under the alias `a`.
    let select = select("SELECT SQL_CACHE a FROM t");
    assert!(select.options.is_empty());
    assert_eq!(select.items.len(), 1);
    // MariaDB: errors 1225 and 1221; MySQL takes `SQL_NO_CACHE` twice.
    assert_eq!(
        error("SELECT SQL_NO_CACHE SQL_NO_CACHE a FROM t", &mariadb()),
        (
            "option `SQL_NO_CACHE` used twice in statement".to_string(),
            "SQL_NO_CACHE".to_string()
        )
    );
    assert_eq!(
        error("SELECT SQL_CACHE SQL_CACHE a FROM t", &mariadb()).0,
        "option `SQL_CACHE` used twice in statement"
    );
    for text in [
        "SELECT SQL_CACHE SQL_NO_CACHE a FROM t",
        "SELECT SQL_NO_CACHE SQL_CACHE a FROM t",
    ] {
        assert_eq!(
            error(text, &mariadb()).0,
            "incorrect usage of `SQL_NO_CACHE` and `SQL_CACHE`"
        );
    }
    assert_eq!(
        options_of("SELECT SQL_NO_CACHE SQL_NO_CACHE a FROM t", &mysql()),
        (vec!["SQL_NO_CACHE"], false)
    );
}

#[test]
fn the_options_of_the_first_select_only_are_refused_elsewhere() {
    let placement = |keyword: &str| {
        format!(
            "incorrect usage/placement of `{keyword}`: only on the first `SELECT` of the \
             statement"
        )
    };
    // Error 1234 of both: a subquery, a derived table, a common table
    // expression, another operand of a set operation.
    for (text, keyword) in [
        ("SELECT (SELECT HIGH_PRIORITY 1) AS s", "HIGH_PRIORITY"),
        (
            "SELECT * FROM (SELECT SQL_CALC_FOUND_ROWS a FROM t LIMIT 1) AS d",
            "SQL_CALC_FOUND_ROWS",
        ),
        (
            "WITH c AS (SELECT SQL_BUFFER_RESULT a FROM t) SELECT * FROM c",
            "SQL_BUFFER_RESULT",
        ),
        (
            "SELECT a FROM t UNION SELECT SQL_CALC_FOUND_ROWS 1 LIMIT 2",
            "SQL_CALC_FOUND_ROWS",
        ),
        (
            "SELECT a FROM t WHERE EXISTS (SELECT HIGH_PRIORITY 1)",
            "HIGH_PRIORITY",
        ),
        (
            "SELECT a FROM t WHERE a IN (SELECT SQL_BUFFER_RESULT a FROM t)",
            "SQL_BUFFER_RESULT",
        ),
        (
            "SELECT a FROM t WHERE a = ANY (SELECT SQL_BUFFER_RESULT a FROM t)",
            "SQL_BUFFER_RESULT",
        ),
    ] {
        for options in [mysql(), mariadb()] {
            assert_eq!(
                error(text, &options),
                (placement(keyword), keyword.to_string()),
                "{text}"
            );
        }
    }
    // A `SELECT` after a `TABLE` or a `VALUES`.
    assert_eq!(
        error("TABLE t UNION SELECT HIGH_PRIORITY 1, 2", &mysql()).0,
        placement("HIGH_PRIORITY")
    );
    assert_eq!(
        error("VALUES (9) UNION SELECT SQL_CALC_FOUND_ROWS 8", &mariadb()).0,
        placement("SQL_CALC_FOUND_ROWS")
    );
    // The first `SELECT` of the statement, between parentheses too.
    for text in [
        "SELECT SQL_CALC_FOUND_ROWS a FROM t UNION SELECT 9 LIMIT 2",
        "WITH c AS (SELECT a FROM t) SELECT SQL_CALC_FOUND_ROWS * FROM c LIMIT 1",
        "((SELECT SQL_CALC_FOUND_ROWS a FROM t LIMIT 2)) UNION ALL SELECT 9 LIMIT 2",
        "(SELECT SQL_CALC_FOUND_ROWS a FROM t LIMIT 2) UNION (SELECT 9) LIMIT 1",
        "((SELECT HIGH_PRIORITY a FROM t LIMIT 2) UNION SELECT 9) LIMIT 1",
        "(SELECT SQL_BUFFER_RESULT a FROM t ORDER BY a)",
        "SELECT (SELECT STRAIGHT_JOIN SQL_SMALL_RESULT SQL_BIG_RESULT 1) AS s",
    ] {
        for options in [mysql(), mariadb()] {
            assert!(parse(text, &options).is_ok(), "{text}");
        }
    }
    // MySQL takes `SQL_NO_CACHE` anywhere, and the options of a `SELECT`
    // between parentheses followed by `ORDER BY` or `LIMIT`; MariaDB does
    // not.
    for (text, keyword) in [
        ("SELECT (SELECT SQL_NO_CACHE 1) AS s", "SQL_NO_CACHE"),
        (
            "SELECT a FROM t UNION SELECT SQL_NO_CACHE 1",
            "SQL_NO_CACHE",
        ),
        (
            "SELECT * FROM ((SELECT SQL_NO_CACHE a FROM t)) AS d",
            "SQL_NO_CACHE",
        ),
    ] {
        assert!(parse(text, &mysql()).is_ok(), "{text}");
        assert_eq!(error(text, &mariadb()).0, placement(keyword), "{text}");
    }
    for (text, keyword) in [
        (
            "(SELECT SQL_CALC_FOUND_ROWS a FROM t LIMIT 2) LIMIT 1",
            "SQL_CALC_FOUND_ROWS",
        ),
        (
            "(SELECT SQL_CALC_FOUND_ROWS a FROM t LIMIT 2) ORDER BY 1 DESC",
            "SQL_CALC_FOUND_ROWS",
        ),
        (
            "((SELECT HIGH_PRIORITY a FROM t LIMIT 2)) LIMIT 1",
            "HIGH_PRIORITY",
        ),
        (
            "((SELECT SQL_NO_CACHE a FROM t LIMIT 2) LIMIT 1)",
            "SQL_NO_CACHE",
        ),
        (
            "WITH c AS (SELECT a FROM t) (SELECT SQL_BUFFER_RESULT a FROM c LIMIT 2) LIMIT 1",
            "SQL_BUFFER_RESULT",
        ),
    ] {
        assert!(parse(text, &mysql()).is_ok(), "{text}");
        assert_eq!(
            error(text, &mariadb()),
            (
                format!(
                    "incorrect usage/placement of `{keyword}`: MariaDB refuses it in a \
                     `SELECT` between parentheses followed by `ORDER BY` or `LIMIT`"
                ),
                keyword.to_string()
            ),
            "{text}"
        );
    }
}
