use prisme_plan::{Expr, Filter as FilterNode, Project as ProjectNode, Scan, SortForm, SortKey};
use prisme_types::{DataType, IntegerSize, IntegerType, Type};

use super::*;
use crate::build::Tables;
use crate::report::Transpilation;
use crate::transpile::{InputDialect, TranspileError, transpile};

fn mysql() -> InputDialect {
    InputDialect {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
    }
}

fn mariadb() -> InputDialect {
    InputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(10, 6, 3),
    }
}

/// Output dialect of the same dialect and version as `input`.
fn output(input: InputDialect) -> OutputDialect {
    OutputDialect {
        dialect: input.dialect,
        version: input.version,
        mode: SqlMode::default(),
    }
}

/// `t (a INT NOT NULL, b INT NULL)`, `u (a INT NOT NULL, c INT NULL)` and
/// `d (dt DATETIME NOT NULL, dn DATETIME NULL, ts TIMESTAMP NULL, n BIGINT
/// NOT NULL, i INT NULL, iu INT UNSIGNED NULL, f DOUBLE NULL, p DECIMAL(10,2)
/// NULL)`.
fn catalog() -> Tables {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let mut tables = Tables::default();
    tables.insert(
        None,
        "d",
        vec![
            (
                Ident::new("dt"),
                Type::not_null(DataType::DateTime { fraction: 0 }),
            ),
            (
                Ident::new("dn"),
                Type::nullable(DataType::DateTime { fraction: 0 }),
            ),
            (
                Ident::new("ts"),
                Type::nullable(DataType::Timestamp { fraction: 0 }),
            ),
            (
                Ident::new("n"),
                Type::not_null(DataType::Integer(IntegerType::signed(IntegerSize::Big))),
            ),
            (Ident::new("i"), Type::nullable(int.clone())),
            (
                Ident::new("iu"),
                Type::nullable(DataType::Integer(IntegerType::unsigned(IntegerSize::Int))),
            ),
            (
                Ident::new("f"),
                Type::nullable(DataType::Float(prisme_types::FloatType::Double)),
            ),
            (
                Ident::new("p"),
                Type::nullable(DataType::Decimal(
                    prisme_types::DecimalType::new(10, 2).unwrap(),
                )),
            ),
        ],
    );
    let text = DataType::Text(prisme_types::TextType {
        length: Some(30),
        fixed: false,
        charset: None,
        collation: None,
    });
    tables.insert(
        None,
        "g",
        vec![
            (Ident::new("s"), Type::not_null(text)),
            (
                Ident::new("vb"),
                Type::not_null(DataType::Binary(prisme_types::BinaryType {
                    length: Some(10),
                    fixed: false,
                })),
            ),
            (Ident::new("e"), Type::not_null(DataType::Enum)),
            (Ident::new("js"), Type::nullable(DataType::Json)),
            (Ident::new("dd"), Type::not_null(DataType::Date)),
            (
                Ident::new("dt3"),
                Type::not_null(DataType::DateTime { fraction: 3 }),
            ),
            (
                Ident::new("tm"),
                Type::not_null(DataType::Time { fraction: 0 }),
            ),
            (Ident::new("y"), Type::not_null(DataType::Year)),
            (
                Ident::new("de"),
                Type::not_null(DataType::Decimal(
                    prisme_types::DecimalType::new(25, 1).unwrap(),
                )),
            ),
        ],
    );
    tables.insert(
        None,
        "u",
        vec![
            (Ident::new("a"), Type::not_null(int.clone())),
            (Ident::new("c"), Type::nullable(int.clone())),
        ],
    );
    tables.insert(
        None,
        "t",
        vec![
            (Ident::new("a"), Type::not_null(int.clone())),
            (Ident::new("b"), Type::nullable(int)),
        ],
    );
    tables
}

/// Transpilation of `text` from `input` to `output`.
fn run(
    text: &str,
    input: InputDialect,
    output: OutputDialect,
) -> Result<Transpilation, TranspileError> {
    transpile(text, input, output, &catalog())
}

/// Query of `text`, from `input` to `output`, and the rules of its steps.
fn transpiled(
    text: &str,
    input: InputDialect,
    output: OutputDialect,
) -> (String, Vec<&'static str>) {
    let transpilation = run(text, input, output).unwrap_or_else(|error| panic!("{text}: {error}"));
    assert!(
        transpilation.errors.is_empty(),
        "{:?}",
        transpilation.errors
    );
    let rules = transpilation.steps.iter().map(|step| step.rule).collect();
    (transpilation.sql, rules)
}

#[test]
fn the_example_of_mysql_runs_on_mariadb() {
    let text = "WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds";
    let transpilation = run(text, mysql(), output(mariadb())).unwrap();
    assert_eq!(
        transpilation.sql,
        "WITH ds (c) AS (VALUES (123), (234), (345)) SELECT * FROM ds"
    );
    let [step] = transpilation.steps.as_slice() else {
        panic!("{:?}", transpilation.steps);
    };
    assert_eq!(step.rule, "values.form");
    assert_eq!(step.kind, StepKind::Rewrite);
    let span = step.span.unwrap();
    assert_eq!(
        &text[span.start..span.end],
        "VALUES ROW (123), ROW (234), ROW (345)"
    );
}

#[test]
fn the_example_comes_back_to_mysql_with_its_form() {
    let (sql, rules) = transpiled(
        "WITH ds (c) AS (VALUES (123), (234), (345)) SELECT * FROM ds",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "WITH ds (c) AS (VALUES ROW(123), ROW(234), ROW(345)) SELECT * FROM ds"
    );
    assert_eq!(rules, ["values.form"]);
}

#[test]
fn a_query_towards_its_own_dialect_keeps_its_forms_without_step() {
    for text in [
        "SELECT a AS x, b y, `a` FROM t AS u WHERE a > 1 AND b IS NOT NULL ORDER BY x DESC, 2 LIMIT 5, 10",
        "SELECT DISTINCT a FROM t ORDER BY a LIMIT 10 OFFSET 20",
        "SELECT a FROM t WHERE a BETWEEN 1 AND 2 OR b NOT IN (1, 2) OR b LIKE 'x%' ESCAPE '!'",
        "SELECT 1.50, 1e5, 'it''s', \"dq\", NULL, TRUE",
        "SELECT (a + 1) * 2 AS v, -a AS n, ~a AS i, a <=> b AS s FROM t",
        "VALUES ROW(1, 'a'), ROW(2, 'b') ORDER BY column_0 DESC LIMIT 1",
    ] {
        let (sql, rules) = transpiled(text, mysql(), output(mysql()));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}: {rules:?}");
    }
}

#[test]
fn the_output_is_a_fixpoint() {
    for text in [
        "select a+1, b from t where not a=1 && b <> 2 order by 1",
        "SELECT a FROM t WHERE (a = 1 OR a = 2) AND (b = 3 XOR b = 4)",
        "SELECT a - (b - 1), a - b - 1, a DIV 2 MOD 3 FROM t",
    ] {
        let (once, _) = transpiled(text, mysql(), output(mysql()));
        let (twice, rules) = transpiled(&once, mysql(), output(mysql()));
        assert_eq!(twice, once, "{text}");
        assert!(rules.is_empty(), "{once}: {rules:?}");
    }
}

#[test]
fn an_implicit_name_is_kept_by_an_alias() {
    let (sql, rules) = transpiled("SELECT a+1 FROM t", mysql(), output(mysql()));
    assert_eq!(sql, "SELECT a + 1 AS `a+1` FROM t");
    assert_eq!(rules, ["column.implicit-name"]);
}

#[test]
fn values_whose_names_change_become_a_union() {
    let (sql, rules) = transpiled(
        "VALUES ROW(1, 'x'), ROW(2, 'y')",
        mysql(),
        output(mariadb()),
    );
    assert_eq!(
        sql,
        "SELECT 1 AS `column_0`, 'x' AS `column_1` UNION ALL SELECT 2, 'y'"
    );
    assert_eq!(rules, ["values.union"]);
    // MariaDB names its columns after the first row: the same names in MariaDB.
    let (sql, rules) = transpiled("VALUES (1, 'x')", mariadb(), output(mariadb()));
    assert_eq!(sql, "VALUES (1, 'x')");
    assert!(rules.is_empty());
}

#[test]
fn a_word_reserved_in_the_output_is_quoted() {
    let mut old = mysql();
    old.version = Version::new(8, 0, 11);
    // `LATERAL` is reserved from MySQL 8.0.14.
    let (sql, rules) = transpiled("SELECT a AS lateral FROM t", old, output(mysql()));
    assert_eq!(sql, "SELECT a AS `lateral` FROM t");
    assert_eq!(rules, ["identifier.quoted"]);
}

#[test]
fn comments_are_written_again_near_their_place() {
    let (sql, _) = transpiled(
        "-- title\nSELECT a /* first */, b # second\nFROM t",
        mysql(),
        output(mysql()),
    );
    assert_eq!(sql, "-- title\nSELECT a, /* first */ b # second\nFROM t");
}

#[test]
fn executable_comments_become_code() {
    let transpilation = run("SELECT 1 /*!80000 + 1 */ AS x", mysql(), output(mysql())).unwrap();
    assert_eq!(transpilation.sql, "SELECT 1 + 1 AS x");
    assert_eq!(transpilation.steps[0].rule, "executable-comment.code");
}

#[test]
fn strings_follow_the_sql_mode_of_the_output() {
    let mut ansi = output(mysql());
    ansi.mode = SqlMode::parse("ANSI_QUOTES,NO_BACKSLASH_ESCAPES");
    // Read with the mode of the session: under ANSI_QUOTES, "x" is a name.
    let (sql, _) = transpiled("SELECT 'a\\b' AS \"x\"", mysql(), ansi);
    assert_eq!(sql, "SELECT 'a\\b' AS `x`");
    let source = Source {
        text: "",
        mode: SqlMode::default(),
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        comments: &[],
    };
    let mut emitter = Emitter {
        out: String::new(),
        output: &ansi,
        source: &source,
        next_comment: 0,
        quiet: 0,
        blocks: Vec::new(),
        top: true,
        column: false,
        number: false,
        json_as_is: false,
        compared_rows: false,
        steps: Vec::new(),
        errors: Vec::new(),
    };
    let literal = Expr::literal(Value::Text("it's a \\".to_string()), DataType::Null);
    emitter.expr(&literal, 0);
    assert_eq!(emitter.out, "'it''s a \\'");
}

#[test]
fn nulls_placed_against_the_rule_of_the_output_are_emulated() {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let scan = Plan::Scan(Scan::new(
        prisme_plan::TableName {
            database: None,
            name: Ident::new("t"),
        },
        vec![(Ident::new("b"), Type::nullable(int.clone()))],
    ));
    let column = Expr::column(
        prisme_plan::ColumnRef {
            qualifier: None,
            name: Ident::new("b"),
        },
        Type::nullable(int),
    );
    let key = SortKey {
        expr: column.clone(),
        direction: Direction::Ascending,
        nulls: Nulls::Last,
        form: SortForm::default(),
    };
    let sort = Plan::Sort(Sort {
        input: Box::new(scan),
        keys: vec![key],
        span: None,
    });
    let item = ProjectItem::Expr(prisme_plan::NamedExpr {
        expr: column,
        name: Ident::new("b"),
        alias: AliasForm::Implicit,
    });
    let plan = Plan::Project(ProjectNode::new(sort, vec![item]).unwrap());
    let source = Source {
        text: "",
        mode: SqlMode::default(),
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        comments: &[],
    };
    let emitted = emit(&Statement::Query(plan), &output(mysql()), &source);
    assert_eq!(emitted.sql, "SELECT b FROM t ORDER BY b IS NULL, b");
    assert_eq!(emitted.steps[0].rule, "sort.nulls");
    // The filter node, unused here, keeps its place in a block.
    let _ = FilterNode {
        input: Box::new(Plan::Values(
            Values::new(vec![Vec::new()], Vec::new(), Vec::new(), ValuesForm::Row).unwrap(),
        )),
        predicate: Expr::literal(Value::Boolean(true), DataType::Boolean),
        span: None,
    };
}

#[test]
fn deep_expressions_go_through() {
    let text = format!("SELECT {}1", "- ".repeat(90));
    let (sql, _) = transpiled(&text, mysql(), output(mysql()));
    assert!(sql.starts_with("SELECT -(-(-("), "{sql}");
}

/// MySQL in `version`, as an input.
fn mysql_at(major: u16, minor: u16, patch: u16) -> InputDialect {
    InputDialect {
        dialect: Dialect::MySql,
        version: Version::new(major, minor, patch),
    }
}

/// Rules of the errors of `text`, from `input` to `output`.
fn errors(text: &str, input: InputDialect, output: OutputDialect) -> Vec<&'static str> {
    run(text, input, output)
        .unwrap_or_else(|error| panic!("{text}: {error}"))
        .errors
        .iter()
        .map(|error| error.rule)
        .collect()
}

#[test]
fn joins_derived_tables_and_subqueries_keep_their_form() {
    for text in [
        "SELECT * FROM t JOIN u USING (a)",
        "SELECT t.a, u.c FROM t LEFT JOIN u ON t.a = u.a WHERE u.c IS NULL",
        "SELECT * FROM t, u WHERE t.a = u.a",
        "SELECT * FROM t NATURAL JOIN u",
        "SELECT * FROM t NATURAL RIGHT OUTER JOIN u",
        "SELECT * FROM t STRAIGHT_JOIN u ON t.a = u.a",
        "SELECT * FROM t CROSS JOIN u INNER JOIN t AS v ON v.a = u.a",
        "SELECT * FROM (t, u) JOIN t AS v ON v.a = u.a",
        "SELECT * FROM t, u JOIN t AS v ON v.a = u.a",
        "SELECT * FROM t JOIN (u JOIN t AS v ON v.a = u.a) ON t.a = u.a",
        "SELECT * FROM t PARTITION (p0) AS x USE INDEX () IGNORE KEY FOR JOIN (PRIMARY, i)",
        "SELECT d.x FROM (SELECT a AS x FROM t) AS d",
        "SELECT * FROM (SELECT 1, 2) AS d (x, y)",
        "SELECT * FROM t, LATERAL (SELECT t.a + 1 AS n) AS d",
        "SELECT a FROM t UNION SELECT a FROM u ORDER BY a LIMIT 3",
        "(SELECT a FROM t LIMIT 1) UNION SELECT 2",
        "TABLE t ORDER BY a LIMIT 1",
        "SELECT a, (SELECT c FROM u WHERE u.a = t.a LIMIT 1) AS c FROM t",
        "SELECT a FROM t WHERE NOT EXISTS (SELECT 1 FROM u) AND a IN (SELECT a FROM u)",
        "SELECT a FROM t WHERE a > ALL (SELECT a FROM u) OR (a, b) = ANY (TABLE u)",
        "SELECT a FROM t WHERE (a, b) IN ((1, 2), (3, 4)) AND (a, b) <> (1, 2)",
    ] {
        let (sql, rules) = transpiled(text, mysql(), output(mysql()));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}: {rules:?}");
    }
}

#[test]
fn set_operations_keep_the_parentheses_their_precedence_needs() {
    let input = mysql_at(8, 0, 31);
    for text in [
        "SELECT 1 UNION SELECT 2 INTERSECT SELECT 3",
        "(SELECT 1 UNION SELECT 2) INTERSECT SELECT 3",
        "SELECT 1 EXCEPT (SELECT 2 EXCEPT SELECT 3)",
        "SELECT 1 INTERSECT ALL SELECT 2 EXCEPT ALL SELECT 3",
    ] {
        let (sql, rules) = transpiled(text, input, output(input));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}: {rules:?}");
    }
    let (sql, _) = transpiled(
        "SELECT 1 UNION (SELECT 2 UNION SELECT 3) UNION ALL SELECT 4",
        input,
        output(input),
    );
    assert_eq!(
        sql,
        "SELECT 1 UNION (SELECT 2 UNION SELECT 3) UNION ALL SELECT 4"
    );
}

#[test]
fn constructs_missing_in_the_output_are_rewritten_or_emulated() {
    let (sql, rules) = transpiled("TABLE t", mysql(), output(mariadb()));
    assert_eq!(sql, "SELECT * FROM t");
    assert_eq!(rules, ["table.select"]);
    let (sql, rules) = transpiled(
        "SELECT * FROM (SELECT 1, 2) AS d (x, y)",
        mysql(),
        output(mariadb()),
    );
    assert_eq!(
        sql,
        "SELECT * FROM (WITH d (x, y) AS (SELECT 1, 2) SELECT * FROM d) AS d"
    );
    assert_eq!(rules, ["derived.columns"]);
    let input = mysql_at(8, 0, 31);
    let (sql, rules) = transpiled(
        "SELECT a FROM t UNION ALL (SELECT a FROM u UNION SELECT 1)",
        input,
        output(mysql_at(8, 0, 21)),
    );
    assert_eq!(
        sql,
        "SELECT a FROM t UNION ALL SELECT * FROM (SELECT a FROM u UNION SELECT 1) AS `d`"
    );
    assert_eq!(rules, ["set.nested"]);
}

#[test]
fn constructs_without_emulation_are_explicit_errors() {
    assert_eq!(
        errors(
            "SELECT * FROM t, LATERAL (SELECT t.a) AS d",
            mysql(),
            output(mariadb())
        ),
        ["derived.lateral"]
    );
    assert_eq!(
        errors(
            "SELECT (SELECT n FROM (SELECT t.a AS n) AS d) FROM t",
            mysql(),
            output(mariadb())
        ),
        ["derived.outer-reference"]
    );
    let input = mysql_at(8, 0, 31);
    assert_eq!(
        errors(
            "SELECT a FROM t INTERSECT SELECT a FROM u",
            input,
            output(mysql_at(8, 0, 30))
        ),
        ["set.operator"]
    );
}

#[test]
fn calls_grouping_and_windows_keep_their_form() {
    for text in [
        "SELECT a, COUNT(*) FROM t GROUP BY a",
        "SELECT a AS x, SUM(b) AS s FROM t WHERE a > 1 GROUP BY x HAVING SUM(b) > 10 ORDER BY s DESC LIMIT 5",
        "SELECT a, b FROM t GROUP BY 1, 2 WITH ROLLUP",
        "SELECT a, b FROM t GROUP BY a",
        "SELECT a + 1, MAX(b) FROM t GROUP BY a + 1",
        "SELECT count(DISTINCT b), GROUP_CONCAT(b ORDER BY a DESC SEPARATOR ';') FROM t",
        "SELECT a FROM t GROUP BY a HAVING COUNT(*) > 1 ORDER BY COUNT(*)",
        "SELECT a, (SELECT MAX(c) FROM u WHERE u.a = t.a) FROM t GROUP BY a",
        "SELECT NOW(), f(a), db.g(1), SUM (a), `LEFT`(b) FROM t",
        "SELECT a, ROW_NUMBER() OVER (PARTITION BY b ORDER BY a ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) AS r FROM t",
        "SELECT SUM(a) OVER w, RANK() OVER (w ORDER BY b) FROM t WINDOW w AS (PARTITION BY a)",
        "SELECT SUM(a) OVER w FROM t WINDOW w AS (ORDER BY a RANGE UNBOUNDED PRECEDING)",
        "SELECT a, SUM(SUM(b)) OVER (ORDER BY a) FROM t GROUP BY a",
        "SELECT COUNT(*) OVER () FROM t",
        "SELECT a, COUNT(*) FROM t /* f */ GROUP BY a",
    ] {
        let (sql, rules) = transpiled(text, mysql(), output(mysql()));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}: {rules:?}");
    }
}

#[test]
fn windows_take_the_forms_mariadb_reads() {
    for (text, expected, rule) in [
        (
            "SELECT RANK() OVER () AS r FROM t",
            "SELECT RANK() OVER (ORDER BY NULL) AS r FROM t",
            "window.order",
        ),
        (
            "SELECT LAG(a) OVER w AS r FROM t WINDOW w AS (PARTITION BY b)",
            "SELECT LAG(a) OVER (w ORDER BY NULL) AS r FROM t WINDOW w AS (PARTITION BY b)",
            "window.order",
        ),
        (
            "SELECT SUM(a) OVER w1 AS s FROM t WINDOW w1 AS (w2), w2 AS (ORDER BY a)",
            "SELECT SUM(a) OVER w1 AS s FROM t WINDOW w2 AS (ORDER BY a), w1 AS (w2)",
            "window.definitions",
        ),
    ] {
        let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
        assert_eq!(sql, expected);
        assert_eq!(rules, [rule], "{text}");
    }
    // The aggregate functions both dialects share go through.
    let text = "SELECT a, COUNT(*), GROUP_CONCAT(b SEPARATOR ',') FROM t GROUP BY a WITH ROLLUP";
    assert_eq!(transpiled(text, mysql(), output(mariadb())).0, text);
}

#[test]
fn built_in_functions_follow_the_reference_files() {
    let text = "SELECT CONCAT(a, 'x'), ABS(-1), `ABS`(b), LEFT(a, 1), LEFT (a, 1), AVG (a) FROM t";
    let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    for (text, output, rule) in [
        (
            "SELECT ANY_VALUE(a) FROM t",
            output(mariadb()),
            "function.absent",
        ),
        (
            "SELECT COERCIBILITY(a) FROM t",
            output(mariadb()),
            "function.untranslatable",
        ),
        (
            "SELECT JSON_DEPTH('[]')",
            output(mariadb()),
            "function.unstudied",
        ),
        (
            "SELECT REGEXP_LIKE(a, 'x') FROM t",
            output(mariadb()),
            "function.regexp",
        ),
        (
            "SELECT FORMAT_PICO_TIME(a) FROM t",
            output(mysql_at(8, 0, 11)),
            "function.version",
        ),
        (
            "SELECT WAIT_UNTIL_SQL_THREAD_AFTER_GTIDS('', 1)",
            output(mysql_at(8, 4, 0)),
            "function.version",
        ),
    ] {
        assert_eq!(errors(text, mysql(), output), [rule], "{text}");
    }
    assert_eq!(
        errors("SELECT ADD_MONTHS(a, 1) FROM t", mariadb(), output(mysql())),
        ["function.add-months"]
    );
    // Without a window, `LAST_VALUE` of MariaDB is a scalar function MySQL lacks.
    assert_eq!(
        errors("SELECT LAST_VALUE(a, b) FROM t", mariadb(), output(mysql())),
        ["function.absent"]
    );
}

#[test]
fn time_to_sec_drops_the_fraction_towards_mariadb() {
    for (text, expected, rules) in [
        (
            "SELECT TIME_TO_SEC(a) AS s FROM t",
            "SELECT TIME_TO_SEC(a) AS s FROM t",
            vec![],
        ),
        (
            "SELECT TIME_TO_SEC('00:39:38.5') AS s",
            "SELECT TIME_TO_SEC('00:39:38.5') DIV 1 AS s",
            vec!["function.time-to-sec"],
        ),
        (
            "SELECT 2 * TIME_TO_SEC('00:39:38.5') AS s",
            "SELECT 2 * (TIME_TO_SEC('00:39:38.5') DIV 1) AS s",
            vec!["function.time-to-sec"],
        ),
    ] {
        let (sql, written) = transpiled(text, mysql(), output(mariadb()));
        assert_eq!(sql, expected, "{text}");
        assert_eq!(written, rules, "{text}");
    }
    let text = "SELECT TIME_TO_SEC(a) AS s FROM t";
    assert_eq!(transpiled(text, mariadb(), output(mysql())).0, text);
    assert_eq!(
        errors(
            "SELECT TIME_TO_SEC('00:39:38.5')",
            mariadb(),
            output(mysql())
        ),
        ["function.time-to-sec"]
    );
}

#[test]
fn now_keeps_the_datetime_of_mysql() {
    let mariadb_11_8 = |input| {
        let mut output = output(input);
        output.dialect = Dialect::MariaDb;
        output.version = Version::new(11, 8, 0);
        output
    };
    for (text, expected, rules) in [
        (
            "SELECT NOW() AS n",
            "SELECT LOCALTIMESTAMP() AS n",
            vec!["function.now"],
        ),
        (
            "SELECT CURRENT_TIMESTAMP(3) AS n",
            "SELECT LOCALTIMESTAMP(3) AS n",
            vec!["function.now"],
        ),
        (
            "SELECT LOCALTIME() AS n",
            "SELECT LOCALTIMESTAMP() AS n",
            vec!["function.now"],
        ),
        (
            "SELECT SYSDATE(3) AS n",
            "SELECT CAST(SYSDATE(3) AS DATETIME(3)) AS n",
            vec!["function.now"],
        ),
        (
            "SELECT SYSDATE() AS n",
            "SELECT CAST(SYSDATE() AS DATETIME) AS n",
            vec!["function.now"],
        ),
    ] {
        let (sql, written) = transpiled(text, mysql(), mariadb_11_8(mysql()));
        assert_eq!(sql, expected, "{text}");
        assert_eq!(written, rules, "{text}");
    }
    // Before MariaDB 11.6.2 and 11.7.1, the same types as MySQL.
    let text = "SELECT NOW() AS n, SYSDATE() AS s, LOCALTIME() AS l";
    let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    // `LOCALTIME` of MariaDB 11.8 is the time of day.
    let input = InputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(11, 8, 0),
    };
    let (sql, rules) = transpiled(
        "SELECT LOCALTIME(2) AS l, NOW() AS n",
        input,
        output(mysql()),
    );
    assert_eq!(sql, "SELECT CURRENT_TIME(2) AS l, NOW() AS n");
    assert_eq!(rules, ["function.now"]);
}

#[test]
fn calls_without_parentheses_keep_their_form() {
    let text = concat!(
        "SELECT CURRENT_DATE AS a, current_time AS b, CURRENT_TIMESTAMP AS c, LOCALTIME AS d, ",
        "LOCALTIMESTAMP AS e, UTC_DATE AS f, UTC_TIME AS g, UTC_TIMESTAMP AS h, CURRENT_USER AS i, ",
        "CURRENT_DATE() AS j"
    );
    for (input, target) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mysql(), mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // A rule keeps the form, the column its name.
    let mariadb_11_8 = OutputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(11, 8, 0),
        mode: SqlMode::default(),
    };
    let (sql, rules) = transpiled("SELECT LOCALTIME, NOW() AS n", mysql(), mariadb_11_8);
    assert_eq!(
        sql,
        "SELECT LOCALTIMESTAMP AS `LOCALTIME`, LOCALTIMESTAMP() AS n"
    );
    assert_eq!(
        rules,
        ["function.now", "column.implicit-name", "function.now"]
    );
    let input = InputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(11, 8, 0),
    };
    let (sql, rules) = transpiled("SELECT LOCALTIME AS l", input, output(mysql()));
    assert_eq!(sql, "SELECT CURRENT_TIME AS l");
    assert_eq!(rules, ["function.now"]);
    // A quoted name is a column.
    let text = "SELECT `localtime` FROM (SELECT 1 AS `localtime`) AS q";
    let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    assert_eq!(
        errors("SELECT CURRENT_ROLE AS r", mariadb(), output(mysql())),
        ["function.untranslatable"]
    );
}

#[test]
fn pad_keeps_its_result_with_an_empty_pad() {
    let mysql_out = |major, minor, patch| OutputDialect {
        dialect: Dialect::MySql,
        version: Version::new(major, minor, patch),
        mode: SqlMode::default(),
    };
    // A pad of at least one character gives the same result everywhere.
    let text = "SELECT LPAD(a, 5, '0') AS l, RPAD(a, 5, 0) AS r, LPAD(a, 5, NULL) AS n FROM t";
    let (sql, rules) = transpiled(text, mysql_at(8, 4, 0), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    // MySQL from 8.0.13 gives an empty string, MariaDB NULL.
    for text in [
        "SELECT LPAD(a, 5, b) AS l FROM t",
        "SELECT RPAD(a, 5, '') AS r FROM t",
    ] {
        assert_eq!(
            errors(text, mysql_at(8, 4, 0), output(mariadb())),
            ["function.pad"],
            "{text}"
        );
        assert_eq!(
            errors(text, mariadb(), mysql_out(9, 3, 0)),
            ["function.pad"],
            "{text}"
        );
    }
    // MySQL before 8.0.13 gives NULL, as MariaDB does.
    let text = "SELECT LPAD(a, 5, b) AS l FROM t";
    let (sql, rules) = transpiled(text, mysql_at(8, 0, 12), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    let (sql, rules) = transpiled(text, mariadb(), mysql_out(8, 0, 11));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    // Without a pad, MariaDB pads with spaces.
    let (sql, rules) = transpiled("SELECT RPAD(a, 5) AS r FROM t", mariadb(), output(mysql()));
    assert_eq!(sql, "SELECT RPAD(a, 5, ' ') AS r FROM t");
    assert_eq!(rules, ["function.pad"]);
}

#[test]
fn uuid_stays_a_string() {
    let mariadb_at = |major, minor, patch| OutputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(major, minor, patch),
        mode: SqlMode::default(),
    };
    let text = "SELECT UUID() AS u";
    let (sql, rules) = transpiled(text, mysql(), mariadb_at(10, 7, 1));
    assert_eq!(sql, "SELECT CAST(UUID() AS CHAR(36)) AS u");
    assert_eq!(rules, ["function.uuid"]);
    let (sql, rules) = transpiled(text, mysql(), mariadb_at(10, 6, 28));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    let (sql, rules) = transpiled(text, mariadb(), output(mysql()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
}

/// `dialect` in `version`, as an input.
fn input_at(dialect: Dialect, major: u16, minor: u16, patch: u16) -> InputDialect {
    InputDialect {
        dialect,
        version: Version::new(major, minor, patch),
    }
}

#[test]
fn unix_timestamp_keeps_the_range_of_the_input() {
    let mysql_8_0 = input_at(Dialect::MySql, 8, 0, 19);
    let mysql_8_4 = input_at(Dialect::MySql, 8, 4, 0);
    let mariadb_10_6 = input_at(Dialect::MariaDb, 10, 6, 3);
    let mariadb_11_8 = input_at(Dialect::MariaDb, 11, 8, 0);
    for (input, target, argument, expected) in [
        // Out of the range, 0 in MySQL, NULL in MariaDB.
        (
            mysql_8_0,
            mariadb_10_6,
            "dt",
            "IFNULL(UNIX_TIMESTAMP(dt), 0)",
        ),
        (
            mysql_8_0,
            mariadb_10_6,
            "dn",
            "IF(dn IS NULL, NULL, IFNULL(UNIX_TIMESTAMP(dn), 0))",
        ),
        // MariaDB 11.8 goes to 2106, without the epoch.
        (
            mysql_8_0,
            mariadb_11_8,
            "dt",
            "CASE WHEN UNIX_TIMESTAMP(dt) < 2147483648 THEN UNIX_TIMESTAMP(dt) ELSE 0 END",
        ),
        (
            mysql_8_0,
            mariadb_11_8,
            "ts",
            "CASE WHEN UNIX_TIMESTAMP(ts) < 2147483648 THEN UNIX_TIMESTAMP(ts) WHEN ts IS NOT NULL THEN 0 END",
        ),
        // MySQL gives 0 at the epoch and out of its range.
        (
            mariadb_10_6,
            mysql_8_0,
            "dn",
            "CASE WHEN UNIX_TIMESTAMP(dn) > 0 THEN UNIX_TIMESTAMP(dn) WHEN dn = FROM_UNIXTIME(0) THEN 0 END",
        ),
        (
            mariadb_10_6,
            mysql_8_4,
            "dn",
            "CASE WHEN UNIX_TIMESTAMP(dn) > 0 AND UNIX_TIMESTAMP(dn) < 2147483648 THEN UNIX_TIMESTAMP(dn) WHEN dn = FROM_UNIXTIME(0) THEN 0 END",
        ),
        (
            mariadb_11_8,
            mysql_8_4,
            "dn",
            "CASE WHEN UNIX_TIMESTAMP(dn) > 0 AND UNIX_TIMESTAMP(dn) < 4294967296 THEN UNIX_TIMESTAMP(dn) END",
        ),
        // MySQL gives 0 in the second after the epoch, its fraction dropped.
        (
            mysql_8_0,
            mariadb_10_6,
            "'1960-01-01 00:00:00.5'",
            "CASE WHEN UNIX_TIMESTAMP('1960-01-01 00:00:00.5') >= 1 THEN UNIX_TIMESTAMP('1960-01-01 00:00:00.5') ELSE 0 END",
        ),
        (
            mariadb_10_6,
            mysql_8_0,
            "'1970-01-01 00:00:00.5'",
            "CASE WHEN UNIX_TIMESTAMP('1970-01-01 00:00:00.5') > 0 THEN UNIX_TIMESTAMP('1970-01-01 00:00:00.5') WHEN '1970-01-01 00:00:00.5' >= FROM_UNIXTIME(0) AND '1970-01-01 00:00:00.5' < FROM_UNIXTIME(1) THEN TRUNCATE(UNIX_TIMESTAMP('1970-01-01 00:00:00.5' + INTERVAL 1 SECOND) - 1, 1) END",
        ),
    ] {
        let output = output(target);
        let text = format!("SELECT UNIX_TIMESTAMP({argument}) AS u FROM d");
        let (sql, rules) = transpiled(&text, input, output);
        assert_eq!(sql, format!("SELECT {expected} AS u FROM d"), "{text}");
        assert_eq!(rules, ["function.unix-timestamp"], "{text}");
    }
    // Within both ranges: the current time, a `TIMESTAMP` of the output, a
    // literal date from 1971 to 2037.
    for (input, target, argument) in [
        (mysql_8_0, mariadb_10_6, ""),
        (mysql_8_0, mariadb_10_6, "ts"),
        (mysql_8_4, mariadb_11_8, "ts"),
        (mariadb_11_8, mysql_8_0, "ts"),
        (mysql_8_4, mariadb_11_8, "'2024-05-06 07:08:09.5'"),
        (mysql_8_4, mariadb_11_8, "NULL"),
    ] {
        let text = format!("SELECT UNIX_TIMESTAMP({argument}) AS u FROM d");
        let (sql, rules) = transpiled(&text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}: {rules:?}");
    }
    // Beyond the range of the output, or an argument that would be
    // evaluated twice.
    for (input, target, argument) in [
        (mysql_8_4, mariadb_11_8, "dt"),
        (mysql_8_4, mariadb_11_8, "'2200-01-01'"),
        (mysql_8_4, mariadb_11_8, "'2024-02-30'"),
        (mariadb_11_8, mysql_8_0, "dn"),
        (mysql_8_0, mariadb_11_8, "(SELECT MAX(dt) FROM d)"),
        (mysql_8_0, mariadb_11_8, "SYSDATE()"),
    ] {
        let text = format!("SELECT UNIX_TIMESTAMP({argument}) AS u FROM d");
        assert_eq!(
            errors(&text, input, output(target)),
            ["function.unix-timestamp"],
            "{text}"
        );
    }
    // The steps of an argument written twice are recorded once.
    let (sql, rules) = transpiled(
        "SELECT UNIX_TIMESTAMP(NOW()) AS u",
        mysql_8_0,
        output(mariadb_11_8),
    );
    assert_eq!(sql.matches("LOCALTIMESTAMP()").count(), 2, "{sql}");
    assert_eq!(rules, ["function.unix-timestamp", "function.now"]);
}

#[test]
fn from_unixtime_keeps_the_range_and_the_type_of_the_input() {
    let mysql_8_0 = input_at(Dialect::MySql, 8, 0, 19);
    let mysql_8_4 = input_at(Dialect::MySql, 8, 4, 0);
    let mariadb_10_6 = input_at(Dialect::MariaDb, 10, 6, 3);
    let mariadb_11_8 = input_at(Dialect::MariaDb, 11, 8, 0);
    for (input, target, arguments, expected) in [
        // MariaDB 11.8 goes to 2106, without the epoch, and gives a
        // `TIMESTAMP`.
        (
            mysql_8_0,
            mariadb_11_8,
            "n",
            "CAST(IF(n = 0, FROM_UNIXTIME(1) - INTERVAL 1 SECOND, FROM_UNIXTIME(IF(n < 2147483648, n, NULL))) AS DATETIME)",
        ),
        (
            mysql_8_4,
            mariadb_11_8,
            "p",
            "CAST(IF(p = 0, FROM_UNIXTIME(1) - INTERVAL 1 SECOND, FROM_UNIXTIME(p)) AS DATETIME(2))",
        ),
        (
            mysql_8_4,
            mariadb_11_8,
            "iu, '%Y'",
            "IF(iu = 0, DATE_FORMAT(FROM_UNIXTIME(1) - INTERVAL 1 SECOND, '%Y'), FROM_UNIXTIME(iu, '%Y'))",
        ),
        (
            mysql_8_4,
            mariadb_11_8,
            "0",
            "CAST(FROM_UNIXTIME(1) - INTERVAL 1 SECOND AS DATETIME)",
        ),
        (
            mysql_8_4,
            mariadb_11_8,
            "1700000000.5",
            "CAST(FROM_UNIXTIME(1700000000.5) AS DATETIME(1))",
        ),
        // MySQL 8.4 goes to 3001, with the epoch.
        (
            mariadb_11_8,
            mysql_8_4,
            "n",
            "FROM_UNIXTIME(NULLIF(IF(n < 4294967296, n, NULL), 0))",
        ),
        (mariadb_11_8, mysql_8_4, "i", "FROM_UNIXTIME(NULLIF(i, 0))"),
    ] {
        let text = format!("SELECT FROM_UNIXTIME({arguments}) AS f FROM d");
        let (sql, rules) = transpiled(&text, input, output(target));
        assert_eq!(sql, format!("SELECT {expected} AS f FROM d"), "{text}");
        assert_eq!(rules, ["function.from-unixtime"], "{text}");
    }
    let text = "SELECT FROM_UNIXTIME(n) AS f FROM d";
    let (sql, rules) = transpiled(text, mysql_8_0, output(mariadb_10_6));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    // Beyond the range of the output, an argument of unknown type, or an
    // argument that would be evaluated twice.
    for (input, target, arguments) in [
        (mysql_8_4, mariadb_11_8, "n"),
        (mysql_8_4, mariadb_11_8, "f"),
        (mariadb_11_8, mysql_8_0, "iu"),
        (mysql_8_0, mariadb_11_8, "ABS(i)"),
        (mysql_8_0, mariadb_11_8, "i + RAND()"),
        // `NULLIF` evaluates its first argument twice.
        (mariadb_11_8, mysql_8_4, "(SELECT i FROM d LIMIT 1)"),
    ] {
        let text = format!("SELECT FROM_UNIXTIME({arguments}) AS f FROM d");
        assert_eq!(
            errors(&text, input, output(target)),
            ["function.from-unixtime"],
            "{text}"
        );
    }
}

#[test]
fn stored_functions_keep_calling_the_database() {
    for (text, expected, rules) in [
        (
            "SELECT f(a), db.g(1) FROM t",
            "SELECT f(a), db.g(1) FROM t",
            vec![],
        ),
        // `NOW` followed by a space calls a stored function in both dialects.
        ("SELECT NOW () FROM t", "SELECT NOW () FROM t", vec![]),
        // MariaDB reads `SYSDATE ()` as its built-in function.
        (
            "SELECT SYSDATE () FROM t",
            "SELECT `SYSDATE`() AS `SYSDATE ()` FROM t",
            vec!["function.stored", "column.implicit-name"],
        ),
        (
            "SELECT `LEFT`(a, 1) FROM t",
            "SELECT `LEFT`(a, 1) FROM t",
            vec![],
        ),
    ] {
        let (sql, written) = transpiled(text, mysql(), output(mariadb()));
        assert_eq!(sql, expected, "{text}");
        assert_eq!(written, rules, "{text}");
    }
    // `ADD_MONTHS` is a stored function in MySQL, a built-in one in MariaDB.
    assert_eq!(
        errors("SELECT ADD_MONTHS(a, 1) FROM t", mysql(), output(mariadb())),
        ["function.stored"]
    );
    // A stored function named like a function of a later version.
    assert_eq!(
        errors(
            "SELECT FORMAT_PICO_TIME(a) FROM t",
            mysql_at(8, 0, 11),
            output(mysql_at(8, 4, 0))
        ),
        ["function.stored"]
    );
    // `MEDIAN (a)` calls a stored function in MariaDB, MySQL has no `MEDIAN`.
    let (sql, rules) = transpiled("SELECT MEDIAN (a) FROM t", mariadb(), output(mysql()));
    assert_eq!(sql, "SELECT MEDIAN (a) FROM t");
    assert!(rules.is_empty(), "{rules:?}");
}

#[test]
fn what_the_output_lacks_is_an_explicit_error() {
    for (text, output, rule) in [
        (
            "SELECT LAG(a, 1, 0) OVER (ORDER BY a) FROM t",
            output(mariadb()),
            "window.default",
        ),
        (
            "SELECT JSON_ARRAYAGG(a) FROM t",
            output(mariadb()),
            "function.untranslatable",
        ),
        (
            "SELECT a, COUNT(*) FROM t GROUP BY a WITH ROLLUP ORDER BY a",
            output(mariadb()),
            "rollup.order",
        ),
        (
            "SELECT a, COUNT(*) FROM t GROUP BY a WITH ROLLUP ORDER BY a",
            output(mysql_at(8, 0, 11)),
            "rollup.order",
        ),
    ] {
        assert_eq!(errors(text, mysql(), output), [rule], "{text}");
    }
}

#[test]
fn greatest_compares_as_the_input_does() {
    let rule = ["function.greatest"];
    // From MySQL, which compares a string with a number as strings, a time
    // with a string as strings, and gives a date compared with a string as
    // a string.
    for (text, expected) in [
        (
            "SELECT GREATEST(a, b) AS v FROM t",
            "SELECT GREATEST(a, b) AS v FROM t",
        ),
        (
            "SELECT GREATEST(dd, dt3) AS v FROM g",
            "SELECT GREATEST(dd, dt3) AS v FROM g",
        ),
        (
            "SELECT LEAST(s, 'x') AS v FROM g",
            "SELECT LEAST(s, 'x') AS v FROM g",
        ),
        (
            "SELECT s FROM g ORDER BY LEAST(s, 'x')",
            "SELECT s FROM g ORDER BY LEAST(s, 'x')",
        ),
        (
            "SELECT COUNT(*) AS c FROM g GROUP BY LEAST(s, 'x')",
            "SELECT COUNT(*) AS c FROM g GROUP BY LEAST(s, 'x')",
        ),
    ] {
        let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
        assert_eq!(sql, expected);
        assert!(rules.is_empty(), "{text}");
    }
    for (text, expected) in [
        (
            "SELECT LEAST(s, 'x') + 0 AS v FROM g",
            "SELECT CONCAT(LEAST(s, 'x')) + 0 AS v FROM g",
        ),
        (
            "SELECT (SELECT LEAST(s, 'x') FROM g LIMIT 1) AS v",
            "SELECT (SELECT CONCAT(LEAST(s, 'x')) FROM g LIMIT 1) AS v",
        ),
        (
            "WITH c AS (SELECT LEAST(s, 'x') AS v FROM g) SELECT v FROM c",
            "WITH c AS (SELECT CONCAT(LEAST(s, 'x')) AS v FROM g) SELECT v FROM c",
        ),
        (
            "SELECT LEAST(s, 9) AS v FROM g",
            "SELECT LEAST(s, CONCAT(9)) AS v FROM g",
        ),
        (
            "SELECT LEAST(s, 9) + 0 AS v FROM g",
            "SELECT CONCAT(LEAST(s, CONCAT(9))) + 0 AS v FROM g",
        ),
        (
            "SELECT GREATEST(dd, '2024-01-09', NULL) AS v FROM g",
            "SELECT CONCAT(GREATEST(dd, '2024-01-09', NULL)) AS v FROM g",
        ),
        (
            "SELECT LEAST(tm, '9:00:00', 5) AS v FROM g",
            "SELECT LEAST(CONCAT(tm), '9:00:00', CONCAT(5)) AS v FROM g",
        ),
        (
            "SELECT LEAST(a, NULL) AS v FROM t",
            "SELECT LEAST(a, IF(FALSE, a, NULL)) AS v FROM t",
        ),
    ] {
        let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
        assert_eq!(sql, expected);
        assert_eq!(rules, rule, "{text}");
    }
    // From MariaDB, which compares a string with a number as numbers, a
    // date or a time with a string as dates or times.
    for (text, expected) in [
        (
            "SELECT LEAST(s, 'x') AS v FROM g",
            "SELECT LEAST(s, 'x') AS v FROM g",
        ),
        (
            "SELECT LEAST(a, 1.5e0, NULL) AS v FROM t",
            "SELECT LEAST(a, 1.5e0, NULL) AS v FROM t",
        ),
    ] {
        let (sql, rules) = transpiled(text, mariadb(), output(mysql()));
        assert_eq!(sql, expected);
        assert!(rules.is_empty(), "{text}");
    }
    for (text, expected) in [
        (
            "SELECT LEAST(s, 9) AS v FROM g",
            "SELECT LEAST(s + 0e0, 9) AS v FROM g",
        ),
        (
            "SELECT LEAST(e, vb, 1) + 0 AS v FROM g",
            "SELECT LEAST(e + 0e0, vb + 0e0, 1) + 0 AS v FROM g",
        ),
        (
            "SELECT GREATEST(dd, '2024-01-09') + 0 AS v FROM g",
            "SELECT GREATEST(dd, CAST('2024-01-09' AS DATE)) + 0 AS v FROM g",
        ),
        (
            "SELECT LEAST(dt3, '2000-01-01 00:00:00.123456') AS v FROM g",
            "SELECT LEAST(dt3, CAST('2000-01-01 00:00:00.123456' AS DATETIME(6))) AS v FROM g",
        ),
        (
            "SELECT LEAST(dd, dt3, '2024-05-06 03:00:00') AS v FROM g",
            "SELECT LEAST(dd, dt3, CAST('2024-05-06 03:00:00' AS DATETIME)) AS v FROM g",
        ),
        (
            "SELECT LEAST(tm, '-1:02:03.4567') AS v FROM g",
            "SELECT LEAST(tm, CAST('-1:02:03.4567' AS TIME(4))) AS v FROM g",
        ),
        (
            "SELECT LEAST(a, NULL) AS v FROM t",
            "SELECT LEAST(a, NULL + 0e0) AS v FROM t",
        ),
    ] {
        let (sql, rules) = transpiled(text, mariadb(), output(mysql()));
        assert_eq!(sql, expected);
        assert_eq!(rules, rule, "{text}");
    }
}

#[test]
fn greatest_refuses_what_compares_otherwise() {
    for (text, input, target) in [
        // A string that may not be a valid date.
        ("SELECT LEAST(dd, s) AS v FROM g", mysql(), mariadb()),
        (
            "SELECT LEAST(dd, '2024-1-9') AS v FROM g",
            mysql(),
            mariadb(),
        ),
        (
            "SELECT LEAST(dd, '2024-02-30') AS v FROM g",
            mariadb(),
            mysql(),
        ),
        // Read as a number, which changes the comparison.
        (
            "SELECT LEAST(dd, '2024-01-09') + 0 AS v FROM g",
            mysql(),
            mariadb(),
        ),
        ("SELECT LEAST(tm, 's') + 0 AS v FROM g", mysql(), mariadb()),
        ("SELECT LEAST(s, 'x') + 0 AS v FROM g", mariadb(), mysql()),
        // A time with a number, or a string that may not be a valid time.
        ("SELECT LEAST(tm, 5) AS v FROM g", mariadb(), mysql()),
        ("SELECT LEAST(tm, s) AS v FROM g", mariadb(), mysql()),
        (
            "SELECT LEAST(tm, '25:61:00') AS v FROM g",
            mariadb(),
            mysql(),
        ),
        // A date with a number, a type compared otherwise, a type unknown.
        ("SELECT LEAST(dd, 5) AS v FROM g", mysql(), mariadb()),
        ("SELECT LEAST(y, 2000) AS v FROM g", mysql(), mariadb()),
        ("SELECT LEAST(ABS(a), 1) AS v FROM t", mysql(), mariadb()),
    ] {
        assert_eq!(
            errors(text, input, output(target)),
            ["function.greatest"],
            "{text}"
        );
    }
}

#[test]
fn bit_count_reads_its_argument_as_the_input_does() {
    for (text, input, target) in [
        (
            "SELECT BIT_COUNT(a), BIT_COUNT(NULL) AS v FROM t",
            mysql(),
            mariadb(),
        ),
        (
            "SELECT BIT_COUNT(dd), BIT_COUNT(e), BIT_COUNT(y) AS v FROM g",
            mysql(),
            mariadb(),
        ),
        (
            "SELECT BIT_COUNT(p), BIT_COUNT(3.7) AS v FROM d",
            mariadb(),
            mysql(),
        ),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}");
    }
    let bytes = concat!(
        "LENGTH(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(",
        "REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(HEX(vb), '0', ''), '1', 'x'), '2', 'x'), ",
        "'3', 'xx'), '4', 'x'), '5', 'xx'), '6', 'xx'), '7', 'xxx'), '8', 'x'), '9', 'xx'), ",
        "'A', 'xx'), 'B', 'xxx'), 'C', 'xx'), 'D', 'xxx'), 'E', 'xxx'), 'F', 'xxxx'))"
    );
    let mariadb_bits = |value: &str| {
        format!(
            "BIT_COUNT(CAST((GREATEST(LEAST(ROUND({value}), 18446744073709551615), -9223372036854775808) + 18446744073709551616) % 18446744073709551616 AS UNSIGNED))"
        )
    };
    for (text, input, target, expected) in [
        (
            "SELECT BIT_COUNT('18446744073709551615') AS v",
            mysql(),
            mariadb(),
            "SELECT BIT_COUNT(CAST('18446744073709551615' AS SIGNED)) AS v".to_string(),
        ),
        (
            "SELECT BIT_COUNT(vb) AS v FROM g",
            mysql(),
            mariadb(),
            format!("SELECT {bytes} AS v FROM g"),
        ),
        (
            "SELECT BIT_COUNT(de) AS v FROM g",
            mysql(),
            mariadb(),
            "SELECT BIT_COUNT(LEAST(de, 9223372036854775807)) AS v FROM g".to_string(),
        ),
        (
            "SELECT BIT_COUNT(18446744073709551616) AS v",
            mysql(),
            mariadb(),
            "SELECT BIT_COUNT(LEAST(18446744073709551616, 9223372036854775807)) AS v".to_string(),
        ),
        (
            "SELECT BIT_COUNT(s) AS v FROM g",
            mariadb(),
            mysql(),
            format!(
                "SELECT {} AS v FROM g",
                mariadb_bits("CAST(s AS DECIMAL(65, 30))")
            ),
        ),
        (
            "SELECT BIT_COUNT(vb) AS v FROM g",
            mariadb(),
            mysql(),
            format!(
                "SELECT {} AS v FROM g",
                mariadb_bits("CAST(vb AS DECIMAL(65, 30))")
            ),
        ),
        (
            "SELECT BIT_COUNT(de) AS v FROM g",
            mariadb(),
            mysql(),
            format!("SELECT {} AS v FROM g", mariadb_bits("de")),
        ),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, expected);
        assert_eq!(rules, ["function.bit-count"], "{text}");
    }
    // A floating point number, a type unknown, a string other than a
    // literal towards MariaDB, a literal starting with a line break.
    for (text, input, target) in [
        ("SELECT BIT_COUNT(f) AS v FROM d", mysql(), mariadb()),
        ("SELECT BIT_COUNT(f) AS v FROM d", mariadb(), mysql()),
        ("SELECT BIT_COUNT(ABS(a)) AS v FROM t", mysql(), mariadb()),
        ("SELECT BIT_COUNT(s) AS v FROM g", mysql(), mariadb()),
        (
            "SELECT BIT_COUNT(CONCAT(s)) AS v FROM g",
            mysql(),
            mariadb(),
        ),
        (
            "SELECT BIT_COUNT((SELECT s FROM g LIMIT 1)) AS v",
            mysql(),
            mariadb(),
        ),
        ("SELECT BIT_COUNT(' \\n5') AS v", mysql(), mariadb()),
    ] {
        assert_eq!(
            errors(text, input, output(target)),
            ["function.bit-count"],
            "{text}"
        );
    }
}

#[test]
fn aes_keeps_the_arguments_the_output_reads_alike() {
    let mysql_8_4 = input_at(Dialect::MySql, 8, 4, 0);
    let mariadb_10_6 = input_at(Dialect::MariaDb, 10, 6, 0);
    let mariadb_11_8 = input_at(Dialect::MariaDb, 11, 8, 0);
    for (text, input, target) in [
        (
            "SELECT AES_ENCRYPT(a, 'k'), AES_DECRYPT(a, 'k') AS v FROM t",
            mysql_8_4,
            mariadb_10_6,
        ),
        (
            "SELECT AES_ENCRYPT(a, 'k'), AES_DECRYPT(a, 'k') AS v FROM t",
            mariadb_10_6,
            mysql_8_4,
        ),
        (
            "SELECT AES_ENCRYPT(a, 'k', 'abcdefghijklmnop') AS v FROM t",
            mysql_8_4,
            mariadb_11_8,
        ),
        (
            "SELECT AES_DECRYPT(a, 'k', 'abcdefghijklmnop') AS v FROM t",
            mariadb_11_8,
            mysql_8_4,
        ),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}");
    }
    for (text, input, target, expected) in [
        (
            "SELECT AES_ENCRYPT(a, 'k', 'abcdefghijklmnop') AS v FROM t",
            mysql_8_4,
            mariadb_10_6,
            "SELECT AES_ENCRYPT(a, 'k') AS v FROM t",
        ),
        (
            "SELECT AES_DECRYPT(a, 'k', NULL, 'AES-128-ECB') AS v FROM t",
            mariadb_11_8,
            mysql_8_4,
            "SELECT AES_DECRYPT(a, 'k', NULL) AS v FROM t",
        ),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, expected);
        assert_eq!(rules, ["function.aes"], "{text}");
    }
    for (text, input, target) in [
        (
            "SELECT AES_ENCRYPT(a, 'k', NULL, 'hkdf') AS v FROM t",
            mysql_8_4,
            mariadb_11_8,
        ),
        (
            "SELECT AES_ENCRYPT(a, 'k', NULL, 'pbkdf2_hmac', 'salt', 1000) AS v FROM t",
            mysql_8_4,
            mariadb_11_8,
        ),
        (
            "SELECT AES_ENCRYPT(a, 'k', 'abcdefghijklmnop', 'aes-256-cbc') AS v FROM t",
            mariadb_11_8,
            mysql_8_4,
        ),
        (
            "SELECT AES_ENCRYPT(a, 'k', NULL, CONCAT('aes-128', '-ecb')) AS v FROM t",
            mariadb_11_8,
            mysql_8_4,
        ),
    ] {
        assert_eq!(
            errors(text, input, output(target)),
            ["function.aes"],
            "{text}"
        );
    }
}

#[test]
fn case_keeps_the_family_of_its_type() {
    let text = concat!(
        "SELECT CASE WHEN i > 0 THEN 'a' WHEN i IS NULL OR n = 1 THEN 'b' ELSE 'c' END AS x, ",
        "CASE i + 1 WHEN 2 THEN i ELSE -i END + 1 AS y, CASE WHEN i > 0 THEN iu END AS z, ",
        "CASE WHEN i > 0 THEN ABS(i) ELSE 'x' END AS w FROM d"
    );
    for (input, target) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mysql(), mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // An unsigned integer with `NULL` or a signed integer: an integer in
    // MySQL, a `DECIMAL` in MariaDB.
    let text = concat!(
        "SELECT CASE WHEN i > 0 THEN iu ELSE NULL END AS x, ",
        "CASE WHEN i > 0 THEN iu ELSE i END AS y FROM d"
    );
    let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
    assert_eq!(
        sql,
        concat!(
            "SELECT CAST(CASE WHEN i > 0 THEN iu ELSE NULL END AS UNSIGNED) AS x, ",
            "CAST(CASE WHEN i > 0 THEN iu ELSE i END AS SIGNED) AS y FROM d"
        )
    );
    assert_eq!(rules, ["case.type", "case.type"]);
    let (sql, rules) = transpiled(text, mariadb(), output(mysql()));
    assert_eq!(
        sql,
        concat!(
            "SELECT CAST(CASE WHEN i > 0 THEN iu ELSE NULL END AS DECIMAL(10, 0)) AS x, ",
            "CAST(CASE WHEN i > 0 THEN iu ELSE i END AS DECIMAL(10, 0)) AS y FROM d"
        )
    );
    assert_eq!(rules, ["case.type", "case.type"]);
    // The column keeps its name.
    let (sql, rules) = transpiled(
        "SELECT CASE WHEN i > 0 THEN iu ELSE NULL END FROM d",
        mysql(),
        output(mariadb()),
    );
    assert_eq!(
        sql,
        "SELECT CAST(CASE WHEN i > 0 THEN iu ELSE NULL END AS UNSIGNED) AS `CASE WHEN i > 0 THEN iu ELSE NULL END` FROM d"
    );
    assert_eq!(rules, ["case.type", "column.implicit-name"]);
    // A result of unknown type among integers hides the difference.
    let text = "SELECT CASE WHEN i > 0 THEN ABS(i) ELSE NULL END AS x FROM d";
    assert_eq!(errors(text, mysql(), output(mariadb())), ["case.type"]);
    assert_eq!(errors(text, mariadb(), output(mysql())), ["case.type"]);
    assert!(errors(text, mysql(), output(mysql())).is_empty());
}

#[test]
fn a_decimal_case_keeps_the_scale_of_its_values() {
    // Read as numbers, or a column towards MariaDB: the same values.
    let text = concat!(
        "SELECT SUM(CASE WHEN i > 0 THEN i ELSE p END) AS s, ",
        "CASE WHEN i > 0 THEN i ELSE p END + 1 AS t FROM d ",
        "WHERE CASE WHEN i > 0 THEN 1 ELSE p END BETWEEN 1 AND 2"
    );
    for (input, target) in [(mysql(), mariadb()), (mariadb(), mysql())] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    let text = "SELECT CASE WHEN i > 0 THEN i ELSE p END AS x FROM d";
    let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    // Towards MySQL, the results take the scale of the type.
    let (sql, rules) = transpiled(
        "SELECT CONCAT(CASE WHEN i > 0 THEN i WHEN i = 0 THEN 0 WHEN i < -5 THEN 1.5 ELSE p END) AS x FROM d",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "SELECT CONCAT(CASE WHEN i > 0 THEN CAST(i AS DECIMAL(12, 2)) WHEN i = 0 THEN 0.00 WHEN i < -5 THEN 1.50 ELSE p END) AS x FROM d"
    );
    assert_eq!(rules, ["case.scale"]);
    // Towards MariaDB, an expression that may read the values as strings.
    for text in [
        "SELECT CONCAT(CASE WHEN i > 0 THEN i ELSE p END) AS x FROM d",
        "SELECT i FROM d WHERE CASE WHEN i > 0 THEN i ELSE p END LIKE '4%'",
        "SELECT x FROM (SELECT CASE WHEN i > 0 THEN i ELSE p END AS x FROM d) AS q",
        "SELECT CASE WHEN i > 0 THEN CASE WHEN i > 1 THEN i ELSE p END ELSE 'x' END AS x FROM d",
    ] {
        assert_eq!(
            errors(text, mysql(), output(mariadb())),
            ["case.scale"],
            "{text}"
        );
    }
    // A decimal CASE in a decimal CASE is read as the outer one.
    let text =
        "SELECT CASE WHEN i > 0 THEN CASE WHEN i > 1 THEN i ELSE p END ELSE 1.5 END AS x FROM d";
    assert!(errors(text, mysql(), output(mariadb())).is_empty());
    // A result of unknown type with a decimal.
    let text = "SELECT CONCAT(CASE WHEN i > 0 THEN ABS(i) ELSE p END) AS x FROM d";
    assert_eq!(errors(text, mysql(), output(mariadb())), ["case.scale"]);
    assert_eq!(errors(text, mariadb(), output(mysql())), ["case.scale"]);
    let text = "SELECT CASE WHEN i > 0 THEN ABS(i) ELSE p END + 1 AS x FROM d";
    assert!(errors(text, mysql(), output(mariadb())).is_empty());
}

#[test]
fn cast_keeps_its_form_within_a_dialect() {
    let text = concat!(
        "SELECT CAST(i AS SIGNED) AS a, CAST(i AS UNSIGNED INTEGER) AS b, ",
        "CONVERT(p, DECIMAL(5, 2)) AS c, CAST(i AS CHAR(10) CHARACTER SET utf8mb4) AS e, ",
        "CONVERT(i USING latin1) AS g, CAST(i AS BINARY(4)) AS h, CAST(dt AS DATE) AS j, ",
        "CAST(f AS DOUBLE PRECISION) AS k, CAST(f AS JSON) AS l, CAST(i AS YEAR) AS m, ",
        "CAST(ts AT TIME ZONE 'UTC' AS DATETIME) AS o FROM d"
    );
    let (sql, rules) = transpiled(text, mysql_at(8, 4, 0), output(mysql_at(8, 4, 0)));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    let text = concat!(
        "SELECT CAST(i AS INT) AS a, CAST(f AS DOUBLE(5, 2)) AS b, ",
        "CAST(i AS VARCHAR(5) CHARACTER SET latin1 COLLATE latin1_bin) AS c, ",
        "CAST(i AS INTERVAL DAY_SECOND(2)) AS e, CAST('::1' AS INET6) AS g FROM d"
    );
    let (sql, rules) = transpiled(text, mariadb(), output(mariadb()));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    // A target older than the output version is an error.
    assert_eq!(
        errors(
            "SELECT CAST(f AS DOUBLE) FROM d",
            mysql_at(8, 4, 0),
            output(mysql_at(8, 0, 11))
        ),
        ["cast.target"]
    );
}

#[test]
fn a_target_of_cast_takes_its_equivalent_in_the_other_dialect() {
    let (sql, rules) = transpiled(
        concat!(
            "SELECT CAST(n AS DOUBLE PRECISION) AS a, CAST(n AS REAL) AS b, ",
            "CAST(n AS FLOAT(30)) AS c, CAST(i AS FLOAT(10)) AS e, ",
            "CAST(i AS NATIONAL CHAR(3)) AS g FROM d"
        ),
        mysql_at(8, 4, 0),
        output(mariadb()),
    );
    assert_eq!(
        sql,
        concat!(
            "SELECT CAST(n AS DOUBLE) AS a, CAST(n AS DOUBLE) AS b, ",
            "CAST(n AS DOUBLE) AS c, CAST(i AS FLOAT) AS e, ",
            "CAST(i AS NCHAR(3)) AS g FROM d"
        )
    );
    assert_eq!(rules, ["cast.target"; 5]);
    let (sql, rules) = transpiled(
        concat!(
            "SELECT CAST(de AS INT) AS a, CAST(s AS VARCHAR(5)) AS b, ",
            "CAST(s AS CHAR(5) CHARACTER SET latin1 COLLATE latin1_bin) AS c FROM g"
        ),
        mariadb(),
        output(mysql_at(8, 4, 0)),
    );
    assert_eq!(
        sql,
        concat!(
            "SELECT CAST(de AS SIGNED INT) AS a, CAST(s AS CHAR(5)) AS b, ",
            "CAST(s AS CHAR(5) CHARACTER SET latin1) COLLATE latin1_bin AS c FROM g"
        )
    );
    assert_eq!(rules, ["cast.target"; 3]);
    // Without equivalent: an error.
    for text in [
        "SELECT CAST(i AS JSON) FROM d",
        "SELECT CAST(i AS YEAR) FROM d",
        "SELECT CAST(ts AT TIME ZONE 'UTC' AS DATETIME) FROM d",
    ] {
        assert_eq!(
            errors(text, mysql_at(8, 4, 0), output(mariadb())),
            ["cast.target"],
            "{text}"
        );
    }
    for text in [
        "SELECT CAST(s AS INET6) FROM g",
        "SELECT CAST(de AS DOUBLE(5, 2)) FROM g",
        "SELECT CAST(s AS INTERVAL DAY_SECOND(2)) FROM g",
        "SELECT CAST(de AS DECIMAL(40, 35)) FROM g",
    ] {
        assert_eq!(
            errors(text, mariadb(), output(mysql_at(8, 4, 0))),
            ["cast.target"],
            "{text}"
        );
    }
}

#[test]
fn cast_values_that_differ_between_the_dialects_are_errors() {
    let mysql = mysql_at(8, 4, 0);
    for (text, expected) in [
        ("SELECT CAST(dt3 AS SIGNED) FROM g", vec!["cast.fraction"]),
        ("SELECT CAST(dt3 AS DATETIME) FROM g", vec!["cast.fraction"]),
        ("SELECT CAST(dt3 AS TIME(3)) FROM g", vec![]),
        ("SELECT CAST(dt3 AS DATE) FROM g", vec![]),
        ("SELECT CAST(dd AS DATETIME) FROM g", vec![]),
        ("SELECT CAST(s AS SIGNED) FROM g", vec!["cast.spaces"]),
        ("SELECT CAST('12' AS SIGNED)", vec![]),
        ("SELECT CAST('\n12' AS SIGNED)", vec!["cast.spaces"]),
        ("SELECT CAST(f AS UNSIGNED) FROM d", vec!["cast.unsigned"]),
        ("SELECT CAST(tm AS UNSIGNED) FROM g", vec!["cast.unsigned"]),
        ("SELECT CAST(p AS UNSIGNED) FROM d", vec!["cast.unsigned"]),
        ("SELECT CAST(3.7 AS UNSIGNED)", vec![]),
        ("SELECT CAST(i AS UNSIGNED) FROM d", vec![]),
        ("SELECT CAST(f AS FLOAT) FROM d", vec!["cast.float"]),
        ("SELECT CAST(i AS FLOAT) FROM d", vec![]),
        ("SELECT CAST(vb AS CHAR) FROM g", vec!["cast.bytes"]),
        (
            "SELECT CAST(vb AS CHAR CHARACTER SET latin1) FROM g",
            vec![],
        ),
        ("SELECT CAST(s AS DATE) FROM g", vec!["cast.temporal"]),
        (
            "SELECT CAST('2024-05-06 07:08:09.5' AS DATETIME(1))",
            vec![],
        ),
        ("SELECT CAST('2024-05-06' AS TIME)", vec![]),
        (
            "SELECT CAST('2024-05-06 07:08:09.5' AS DATETIME)",
            vec!["cast.temporal"],
        ),
        (
            "SELECT CAST('2024-05-06 23:00:00+02:00' AS DATETIME)",
            vec!["cast.temporal"],
        ),
        ("SELECT CAST(i AS DATE) FROM d", vec!["cast.invalid"]),
        ("SELECT CAST(e AS DATETIME) FROM g", vec!["cast.invalid"]),
        ("SELECT CAST(y AS TIME) FROM g", vec!["cast.invalid"]),
        ("SELECT CAST(ABS(i) AS SIGNED) FROM d", vec!["cast.unknown"]),
        ("SELECT CAST(ABS(i) AS DECIMAL) FROM d", vec![]),
    ] {
        assert_eq!(errors(text, mysql, output(mariadb())), expected, "{text}");
        assert!(errors(text, mysql, output(mysql)).is_empty(), "{text}");
    }
    // A negative decimal to `UNSIGNED` gives 0 in MySQL before 8.0.19, as
    // in MariaDB.
    let text = "SELECT CAST(p AS UNSIGNED) FROM d";
    assert!(errors(text, mysql_at(8, 0, 18), output(mariadb())).is_empty());
    assert!(errors(text, mariadb(), output(mysql_at(8, 0, 18))).is_empty());
    assert_eq!(errors(text, mariadb(), output(mysql)), ["cast.unsigned"]);
}

#[test]
fn typed_literals_keep_their_form_within_a_dialect() {
    let text = concat!(
        "SELECT DATE '2024-05-06' AS a, TIME '07:08:09.1234567' AS b, ",
        "TIMESTAMP '2024.05.06 7:8:9' AS c, {d '2024-05-06'} AS e, {t 'x'} AS g, ",
        "TIMESTAMP '2024-05-06 01:00:00+02:00' AS h FROM d"
    );
    let (sql, rules) = transpiled(text, mysql_at(8, 4, 0), output(mysql_at(8, 4, 0)));
    assert_eq!(
        sql,
        "SELECT DATE '2024-05-06' AS a, TIME '07:08:09.1234567' AS b, TIMESTAMP '2024.05.06 7:8:9' AS c, {d '2024-05-06'} AS e, 'x' AS g, TIMESTAMP '2024-05-06 01:00:00+02:00' AS h FROM d"
    );
    assert!(rules.is_empty(), "{rules:?}");
    // An offset is absent before MySQL 8.0.19.
    assert_eq!(
        errors(
            "SELECT TIMESTAMP '2024-05-06 01:00:00+02:00'",
            mysql_at(8, 4, 0),
            output(mysql_at(8, 0, 18))
        ),
        ["literal.offset"]
    );
}

#[test]
fn a_typed_literal_read_otherwise_takes_its_value() {
    // The same value in both dialects: as is.
    let text = "SELECT DATE '24/5/6' AS a, TIME '1 07:08' AS b, {ts '2024-05-06T07:08:09.5'} AS c, TIME '07:08:09.1234564' AS e FROM d";
    for (input, output) in [(mysql(), mariadb()), (mariadb(), mysql())] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // Rounded by MySQL, truncated by MariaDB.
    let text =
        "SELECT TIME '07:08:59.9999995' AS a, TIMESTAMP '2024-12-31 23:59:59.9999995' AS b FROM d";
    let (sql, rules) = transpiled(text, mysql(), output(mariadb()));
    assert_eq!(
        sql,
        "SELECT TIME '07:09:00.000000' AS a, TIMESTAMP '2025-01-01 00:00:00.000000' AS b FROM d"
    );
    assert_eq!(rules, ["literal.temporal", "literal.temporal"]);
    let (sql, _) = transpiled(text, mariadb(), output(mysql()));
    assert_eq!(
        sql,
        "SELECT TIME '07:08:59.999999' AS a, TIMESTAMP '2024-12-31 23:59:59.999999' AS b FROM d"
    );
    // A zero day keeps its value; the implicit name is kept by an alias.
    let (sql, _) = transpiled(
        "SELECT TIMESTAMP '2024-05-00 23:59:59.9999995'",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "SELECT TIMESTAMP '2024-05-00 23:59:59.999999' AS `TIMESTAMP '2024-05-00 23:59:59.9999995'`"
    );
}

#[test]
fn typed_literals_the_dialects_read_otherwise_are_errors() {
    for (text, input, output) in [
        // Beyond 838:59:59 in MySQL.
        ("SELECT TIME '838:59:59.5'", mariadb(), mysql()),
        // An error in MySQL, rounded beyond 9999-12-31.
        (
            "SELECT TIMESTAMP '9999-12-31 23:59:59.9999995'",
            mysql(),
            mariadb(),
        ),
        // Read by MySQL only.
        ("SELECT DATE '2024--05-06'", mysql(), mariadb()),
        ("SELECT TIME ':08:09'", mysql(), mariadb()),
    ] {
        assert_eq!(
            errors(text, input, self::output(output)),
            ["literal.temporal"],
            "{text}"
        );
    }
    assert_eq!(
        errors(
            "SELECT TIMESTAMP '2024-05-06 01:00:00+02:00'",
            mysql(),
            output(mariadb())
        ),
        ["literal.offset"]
    );
    // Refused by both: as is.
    assert!(errors("SELECT DATE '2024-02-30'", mysql(), output(mariadb())).is_empty());
}

#[test]
fn intervals_keep_their_form_and_precedence() {
    let text = concat!(
        "SELECT dd + INTERVAL 1 DAY AS a, dt - INTERVAL '1:30' HOUR_MINUTE AS b, ",
        "INTERVAL 1 DAY + dd AS c, DATE_ADD(dd, INTERVAL 1 + i MONTH) AS e, ",
        "DATE_SUB(dt, INTERVAL 2 WEEK) AS g, ADDDATE(dd, INTERVAL 1 YEAR) AS h, ",
        "ADDDATE(dd, 3) AS j, SUBDATE(dt, i) AS k, SUBDATE(dd, INTERVAL 1 QUARTER) AS l, ",
        "TIMESTAMPADD(MINUTE, 5, dt) AS m, INTERVAL(i, 1, 10) AS o FROM d, g"
    );
    for (input, output) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mysql(), mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // The sum takes the operators of a higher level; the interval first
    // takes its operand to the next `AND`.
    let cases = [
        ("dd + INTERVAL 1 DAY * 2", "(dd + INTERVAL 1 DAY) * 2"),
        (
            "dd + INTERVAL 1 DAY ^ 1 * 2",
            "(dd + INTERVAL 1 DAY) ^ 1 * 2",
        ),
        (
            "0 | dd + INTERVAL 1 DAY * 2",
            "0 | (dd + INTERVAL 1 DAY) * 2",
        ),
        ("i + (dd + INTERVAL 1 DAY)", "i + (dd + INTERVAL 1 DAY)"),
        (
            "dd + INTERVAL 1 DAY + INTERVAL 1 HOUR",
            "dd + INTERVAL 1 DAY + INTERVAL 1 HOUR",
        ),
        ("INTERVAL 1 DAY + dd = dd", "INTERVAL 1 DAY + dd = dd"),
        ("(INTERVAL 1 DAY + dd) = dd", "(INTERVAL 1 DAY + dd) = dd"),
        ("2 * INTERVAL 1 DAY + dd", "2 * (INTERVAL 1 DAY + dd)"),
        ("INTERVAL 1 DAY + dd AND 1", "INTERVAL 1 DAY + dd AND 1"),
        ("NOT INTERVAL 1 DAY + dd", "NOT INTERVAL 1 DAY + dd"),
        ("INTERVAL 1 DAY + NOT i", "INTERVAL 1 DAY + (NOT i)"),
        ("dd + INTERVAL 1 OR 0 DAY", "dd + INTERVAL 1 OR 0 DAY"),
        ("dd + INTERVAL (1) DAY", "dd + INTERVAL 1 DAY"),
        ("dd + INTERVAL 1 SQL_TSI_DAY", "dd + INTERVAL 1 DAY"),
        (
            "ADDDATE(dd, INTERVAL 1 DAY + 1)",
            "ADDDATE(dd, INTERVAL 1 DAY + 1)",
        ),
        (
            "date_add(dd, interval 1 day)",
            "DATE_ADD(dd, INTERVAL 1 DAY)",
        ),
    ];
    for (written, expected) in cases {
        let (sql, rules) = transpiled(
            &format!("SELECT {written} AS x FROM d, g"),
            mysql(),
            output(mysql()),
        );
        assert_eq!(
            sql,
            format!("SELECT {expected} AS x FROM d, g"),
            "{written}"
        );
        assert!(rules.is_empty(), "{written}: {rules:?}");
    }
}

#[test]
fn a_time_moved_by_days_follows_the_version() {
    let mariadb_at = |major, minor, patch| OutputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(major, minor, patch),
        mode: SqlMode::default(),
    };
    // A `DATETIME` read on the current date from MySQL 8.0.28.
    let text = "SELECT tm + INTERVAL 1 DAY AS a, TIMESTAMPADD(WEEK, 1, tm) AS b FROM d, g";
    let (sql, rules) = transpiled(text, mysql_at(8, 0, 28), mariadb_at(11, 8, 9));
    assert_eq!(
        sql,
        "SELECT TIMESTAMP(CURRENT_DATE, tm) + INTERVAL 1 DAY AS a, TIMESTAMPADD(WEEK, 1, TIMESTAMP(CURRENT_DATE, tm)) AS b FROM d, g"
    );
    assert_eq!(rules, ["interval.time", "interval.time"]);
    let (sql, rules) = transpiled(text, mysql_at(8, 4, 0), output(mysql_at(8, 0, 27)));
    assert!(sql.starts_with("SELECT TIMESTAMP(CURRENT_DATE, tm) + INTERVAL 1 DAY"));
    assert_eq!(rules, ["interval.time", "interval.time"]);
    // The hours added by `DAY` before, as in MariaDB; NULL by the longer
    // units, of another type.
    let text = "SELECT tm + INTERVAL 1 DAY AS a, tm - INTERVAL '1 2' DAY_HOUR AS b, tm + INTERVAL 1 HOUR AS c FROM d, g";
    let (sql, rules) = transpiled(text, mysql_at(8, 0, 27), mariadb_at(11, 8, 9));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
    for unit in ["WEEK", "MONTH"] {
        let text = format!("SELECT tm + INTERVAL 1 {unit} FROM d, g");
        assert_eq!(
            errors(&text, mysql_at(8, 0, 27), mariadb_at(11, 8, 9)),
            ["interval.time"]
        );
    }
    assert_eq!(
        errors(
            "SELECT tm + INTERVAL 1 DAY FROM d, g",
            mariadb(),
            output(mysql_at(8, 4, 0))
        ),
        ["interval.time"]
    );
}

#[test]
fn intervals_of_values_read_otherwise_are_errors() {
    let cases = [
        // A string or a number, read as a date and time.
        ("s + INTERVAL 1 DAY", "interval.string"),
        ("i + INTERVAL 1 DAY", "interval.string"),
        (
            "'2024-05-06 07:08:09.5' + INTERVAL 1 DAY",
            "interval.string",
        ),
        ("'2024-05-06' + INTERVAL 1 MICROSECOND", "interval.string"),
        ("'2024-05-06' + INTERVAL 1.5 SECOND", "interval.string"),
        // An operand of unknown type.
        ("IFNULL(s, dd) + INTERVAL 1 DAY", "interval.unknown"),
        // Years that may reach 2^31.
        ("dd + INTERVAL 4294967297 YEAR", "interval.year"),
        ("dd + INTERVAL n YEAR", "interval.year"),
        ("dd + INTERVAL iu YEAR", "interval.year"),
        ("TIMESTAMPADD(YEAR, n, dd)", "interval.year"),
        // Microseconds of more than 6 digits.
        (
            "dt + INTERVAL '1.1234567' SECOND_MICROSECOND",
            "interval.microseconds",
        ),
        (
            "dt + INTERVAL 1234567 SECOND_MICROSECOND",
            "interval.microseconds",
        ),
        ("dt + INTERVAL s DAY_MICROSECOND", "interval.microseconds"),
    ];
    for (written, rule) in cases {
        let text = format!("SELECT {written} FROM d, g");
        for (input, output) in [(mysql(), mariadb()), (mariadb(), mysql())] {
            assert_eq!(
                errors(&text, input, self::output(output)),
                [rule],
                "{written}"
            );
        }
        // The same dialect gives the same value.
        assert!(
            errors(&text, mysql(), output(mysql())).is_empty(),
            "{written}"
        );
    }
    // Values read alike.
    let text = concat!(
        "SELECT '2024-05-06' + INTERVAL 1 DAY AS a, '2024-05-06 07:08:09' + INTERVAL 2 SECOND AS b, ",
        "NULL + INTERVAL 1 DAY AS c, dd + INTERVAL -1 YEAR AS e, dd + INTERVAL i YEAR AS g, ",
        "dd + INTERVAL '-2147483647' YEAR AS h, dt + INTERVAL '1.123456' SECOND_MICROSECOND AS j, ",
        "dt + INTERVAL p SECOND_MICROSECOND AS k, dt + INTERVAL 1.5 SECOND AS l FROM d, g"
    );
    for (input, output) in [(mysql(), mariadb()), (mariadb(), mysql())] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
}

#[test]
fn timestampdiff_keeps_its_form_and_refuses_values_read_otherwise() {
    let text = concat!(
        "SELECT TIMESTAMPDIFF(DAY, dd, dt) AS a, TIMESTAMPDIFF(MICROSECOND, tm, dt3) AS b, ",
        "TIMESTAMPDIFF(MONTH, '2024-05-06', ts) AS c, TIMESTAMPDIFF(SECOND, 20240506, p) AS e, ",
        "TIMESTAMPDIFF(HOUR, NULL, i) AS g, TIMESTAMPDIFF(DAY, '2024-05-06 07:08:09.123456', dd) AS h FROM d, g"
    );
    for (input, output) in [(mysql(), mariadb()), (mariadb(), mysql())] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    let (sql, _) = transpiled(
        "SELECT timestampdiff(sql_tsi_day, dd, dt) AS a FROM d, g",
        mysql(),
        output(mariadb()),
    );
    assert_eq!(sql, "SELECT TIMESTAMPDIFF(DAY, dd, dt) AS a FROM d, g");
    let cases = [
        ("TIMESTAMPDIFF(DAY, s, dd)", "timestampdiff.operand"),
        ("TIMESTAMPDIFF(DAY, dd, f)", "timestampdiff.operand"),
        (
            "TIMESTAMPDIFF(DAY, '2024-05-06 07:08:09.1234567', dd)",
            "timestampdiff.operand",
        ),
        (
            "TIMESTAMPDIFF(DAY, '2024--05-06', dd)",
            "timestampdiff.operand",
        ),
        (
            "TIMESTAMPDIFF(DAY, 20240506.1234567, dd)",
            "timestampdiff.operand",
        ),
        (
            "TIMESTAMPDIFF(DAY, IFNULL(s, dd), dd)",
            "timestampdiff.unknown",
        ),
    ];
    for (written, rule) in cases {
        let text = format!("SELECT {written} FROM d, g");
        for (input, output) in [(mysql(), mariadb()), (mariadb(), mysql())] {
            assert_eq!(
                errors(&text, input, self::output(output)),
                [rule],
                "{written}"
            );
        }
        assert!(
            errors(&text, mysql(), output(mysql())).is_empty(),
            "{written}"
        );
    }
}

#[test]
fn the_current_date_and_time_move_and_count_alike() {
    let mariadb_11_8 = input_at(Dialect::MariaDb, 11, 8, 0);
    let text = concat!(
        "SELECT NOW() + INTERVAL 1 DAY AS a, CURDATE() - INTERVAL 1 HOUR AS b, ",
        "TIMESTAMPDIFF(DAY, dd, CURRENT_DATE) AS c, UTC_TIMESTAMP(3) + INTERVAL 1 SECOND AS e, ",
        "CURTIME() + INTERVAL 1 MINUTE AS g, TIMESTAMPDIFF(SECOND, SYSDATE(), dt) AS h FROM d, g"
    );
    for (input, output) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mariadb_11_8, mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // A time of day moved by days: on the current date from MySQL 8.0.28,
    // the hours added in MariaDB.
    let (sql, rules) = transpiled(
        "SELECT CURTIME() + INTERVAL 1 DAY AS a",
        mysql_at(8, 4, 0),
        output(mariadb()),
    );
    assert_eq!(
        sql,
        "SELECT TIMESTAMP(CURRENT_DATE, CURTIME()) + INTERVAL 1 DAY AS a"
    );
    assert_eq!(rules, ["interval.time"]);
    assert_eq!(
        errors(
            "SELECT CURTIME() + INTERVAL 1 DAY",
            mariadb(),
            output(mysql_at(8, 4, 0))
        ),
        ["interval.time"]
    );
}

#[test]
fn extract_keeps_the_parts_read_alike() {
    let text = concat!(
        "SELECT EXTRACT(MICROSECOND FROM tm) AS a, EXTRACT(MINUTE_SECOND FROM dt) AS b, ",
        "EXTRACT(HOUR FROM dt3) AS c, EXTRACT(HOUR_MINUTE FROM ts) AS e, ",
        "EXTRACT(DAY FROM dd) AS h, EXTRACT(WEEK FROM dt) AS j, EXTRACT(YEAR_MONTH FROM s) AS k, ",
        "EXTRACT(YEAR FROM i) AS l, EXTRACT(DAY_HOUR FROM '2024-05-06 07:08:09.5') AS m, ",
        "EXTRACT(DAY FROM '2024-05-06') AS o, EXTRACT(DAY_SECOND FROM NULL) AS q, ",
        "EXTRACT(DAY_MINUTE FROM dd + INTERVAL 1 HOUR) AS r FROM d, g"
    );
    for (input, output) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mysql(), mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // The unit is written in capitals, without `SQL_TSI_`.
    let (sql, _) = transpiled(
        "SELECT extract(sql_tsi_day from dd) AS a FROM g",
        mysql(),
        output(mariadb()),
    );
    assert_eq!(sql, "SELECT EXTRACT(DAY FROM dd) AS a FROM g");
}

#[test]
fn extract_of_a_time_counts_its_hours_as_the_input() {
    let towards_mariadb = [
        ("HOUR", "TIME_TO_SEC(tm) DIV 3600"),
        ("DAY_HOUR", "TIME_TO_SEC(tm) DIV 3600"),
        (
            "HOUR_MINUTE",
            "EXTRACT(HOUR_MINUTE FROM tm) + EXTRACT(DAY FROM tm) * 2400",
        ),
        (
            "DAY_SECOND",
            "EXTRACT(HOUR_SECOND FROM tm) + EXTRACT(DAY FROM tm) * 240000",
        ),
        (
            "HOUR_MICROSECOND",
            "EXTRACT(HOUR_MICROSECOND FROM tm) + EXTRACT(DAY FROM tm) * 240000000000",
        ),
        ("DAY", "EXTRACT(DAY FROM TIMESTAMP(CURRENT_DATE, tm))"),
        ("WEEK", "EXTRACT(WEEK FROM TIMESTAMP(CURRENT_DATE, tm))"),
        ("MONTH", "EXTRACT(MONTH FROM TIMESTAMP(CURRENT_DATE, tm))"),
    ];
    let towards_mysql = [
        ("HOUR", "MOD(EXTRACT(HOUR FROM tm), 24)"),
        ("DAY", "EXTRACT(HOUR FROM tm) DIV 24"),
        (
            "HOUR_SECOND",
            "EXTRACT(HOUR_SECOND FROM tm) - EXTRACT(HOUR FROM tm) DIV 24 * 240000",
        ),
        (
            "DAY_HOUR",
            "EXTRACT(HOUR FROM tm) + EXTRACT(HOUR FROM tm) DIV 24 * 76",
        ),
        (
            "DAY_MICROSECOND",
            "EXTRACT(HOUR_MICROSECOND FROM tm) + EXTRACT(HOUR FROM tm) DIV 24 * 760000000000",
        ),
        ("YEAR_MONTH", "EXTRACT(MINUTE FROM tm) * 0"),
    ];
    for (cases, input, target) in [
        (&towards_mariadb[..], mysql(), mariadb()),
        (&towards_mysql[..], mariadb(), mysql()),
    ] {
        for (unit, expected) in cases {
            let text = format!("SELECT EXTRACT({unit} FROM tm) AS a FROM g");
            let (sql, rules) = transpiled(&text, input, output(target));
            assert_eq!(sql, format!("SELECT {expected} AS a FROM g"), "{text}");
            assert_eq!(rules, ["function.extract"], "{text}");
        }
    }
    // The parts of an hour are read alike.
    let text = "SELECT EXTRACT(MINUTE_MICROSECOND FROM tm) AS a FROM g";
    assert_eq!(transpiled(text, mysql(), output(mariadb())).0, text);
    // The parentheses follow the level of the emulation.
    for (text, input, target, expected) in [
        (
            "SELECT 2 * EXTRACT(HOUR FROM tm) AS a FROM g",
            mysql(),
            mariadb(),
            "SELECT 2 * (TIME_TO_SEC(tm) DIV 3600) AS a FROM g",
        ),
        (
            "SELECT EXTRACT(HOUR FROM tm) * 2 AS a FROM g",
            mysql(),
            mariadb(),
            "SELECT TIME_TO_SEC(tm) DIV 3600 * 2 AS a FROM g",
        ),
        (
            "SELECT -EXTRACT(HOUR_MINUTE FROM tm) AS a FROM g",
            mysql(),
            mariadb(),
            "SELECT -(EXTRACT(HOUR_MINUTE FROM tm) + EXTRACT(DAY FROM tm) * 2400) AS a FROM g",
        ),
        (
            "SELECT 1 - EXTRACT(DAY FROM tm) AS a FROM g",
            mariadb(),
            mysql(),
            "SELECT 1 - EXTRACT(HOUR FROM tm) DIV 24 AS a FROM g",
        ),
    ] {
        assert_eq!(transpiled(text, input, output(target)).0, expected);
    }
}

#[test]
fn extract_takes_the_day_of_a_date_as_the_input() {
    // MySQL drops the day of a literal, a `CAST` and the current date and
    // time.
    for (operand, unit, expected) in [
        (
            "TIMESTAMP '2024-05-06 07:08:09'",
            "DAY_HOUR",
            "EXTRACT(HOUR FROM TIMESTAMP '2024-05-06 07:08:09')",
        ),
        (
            "CAST(dt AS DATE)",
            "DAY_MINUTE",
            "EXTRACT(HOUR_MINUTE FROM CAST(dt AS DATE))",
        ),
        (
            "NOW(6)",
            "DAY_MICROSECOND",
            "EXTRACT(HOUR_MICROSECOND FROM NOW(6))",
        ),
    ] {
        let text = format!("SELECT EXTRACT({unit} FROM {operand}) AS a FROM d");
        let (sql, rules) = transpiled(&text, mysql(), output(mariadb()));
        assert_eq!(sql, format!("SELECT {expected} AS a FROM d"), "{text}");
        assert_eq!(rules, ["function.extract"], "{text}");
    }
    // MariaDB keeps it: the day and the part of the hours taken apart.
    for (operand, unit, expected) in [
        (
            "dt",
            "DAY_MINUTE",
            "EXTRACT(DAY FROM dt) * 10000 + EXTRACT(HOUR_MINUTE FROM dt)",
        ),
        (
            "NOW()",
            "DAY_HOUR",
            "EXTRACT(DAY FROM NOW()) * 100 + EXTRACT(HOUR FROM NOW())",
        ),
    ] {
        let text = format!("SELECT EXTRACT({unit} FROM {operand}) AS a FROM d");
        let (sql, rules) = transpiled(&text, mariadb(), output(mysql()));
        assert_eq!(sql, format!("SELECT {expected} AS a FROM d"), "{text}");
        assert_eq!(rules, ["function.extract"], "{text}");
    }
    let (sql, _) = transpiled(
        "SELECT EXTRACT(DAY_SECOND FROM dt) * 2 AS a FROM d",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "SELECT (EXTRACT(DAY FROM dt) * 1000000 + EXTRACT(HOUR_SECOND FROM dt)) * 2 AS a FROM d"
    );
}

#[test]
fn extract_refuses_the_parts_read_otherwise() {
    for (text, input, target) in [
        // MySQL keeps or drops the day of a column.
        (
            "SELECT EXTRACT(DAY_HOUR FROM dt) FROM d",
            mysql(),
            mariadb(),
        ),
        // A string or a number.
        ("SELECT EXTRACT(MINUTE FROM s) FROM g", mysql(), mariadb()),
        ("SELECT EXTRACT(DAY FROM i) FROM d", mariadb(), mysql()),
        (
            "SELECT EXTRACT(HOUR FROM '2024-05-06') FROM g",
            mysql(),
            mariadb(),
        ),
        (
            "SELECT EXTRACT(SECOND FROM '2024-05-06 07:08:09.1234567')",
            mysql(),
            mariadb(),
        ),
        // Another type, a type unknown.
        ("SELECT EXTRACT(YEAR FROM y) FROM g", mysql(), mariadb()),
        (
            "SELECT EXTRACT(YEAR FROM IFNULL(s, dd)) FROM g",
            mariadb(),
            mysql(),
        ),
        // The week of the zero date.
        ("SELECT EXTRACT(WEEK FROM tm) FROM g", mariadb(), mysql()),
        // An operand written twice that may give two values.
        (
            "SELECT EXTRACT(HOUR_MINUTE FROM (SELECT MAX(tm) FROM g)) FROM g",
            mysql(),
            mariadb(),
        ),
        (
            "SELECT EXTRACT(DAY_HOUR FROM SYSDATE())",
            mariadb(),
            mysql(),
        ),
    ] {
        assert_eq!(
            errors(text, input, output(target)),
            ["function.extract"],
            "{text}"
        );
    }
    // Between two versions of MySQL, the day of a subquery.
    let text = "SELECT EXTRACT(DAY_HOUR FROM (SELECT TIMESTAMP '2024-05-06 07:08:09'))";
    assert_eq!(
        errors(text, mysql_at(8, 0, 11), output(mysql_at(8, 4, 0))),
        ["function.extract"]
    );
    assert!(errors(text, mysql_at(8, 4, 0), output(mysql_at(8, 4, 0))).is_empty());
}

#[test]
fn a_binary_argument_is_read_as_the_input_reads_it() {
    let mysql_8_0_18 = mysql_at(8, 0, 18);
    let mysql_8_4 = mysql_at(8, 4, 0);
    // Towards the character set of the string: the string read as binary.
    for (call, expected) in [
        ("REPLACE(s, vb, '-')", "REPLACE(CAST(s AS BINARY), vb, '-')"),
        (
            "SUBSTRING_INDEX(s, vb, 2)",
            "SUBSTRING_INDEX(CAST(s AS BINARY), vb, 2)",
        ),
        ("LOCATE(vb, s)", "LOCATE(vb, CAST(s AS BINARY))"),
        (
            "INSTR(s, CAST('B' AS BINARY))",
            "INSTR(CAST(s AS BINARY), CAST('B' AS BINARY))",
        ),
    ] {
        let text = format!("SELECT {call} AS a FROM g");
        let (sql, rules) = transpiled(&text, mariadb(), output(mysql_8_4));
        assert_eq!(sql, format!("SELECT {expected} AS a FROM g"), "{text}");
        assert_eq!(rules, ["function.binary-argument"], "{text}");
    }
    // Read alike: character strings, a binary string, the same reading.
    for (call, input, target) in [
        ("REPLACE(s, 'a', '-')", mariadb(), mysql_8_4),
        ("REPLACE(vb, s, '-')", mariadb(), mysql_8_4),
        ("LOCATE(vb, s)", mariadb(), mysql_8_0_18),
        ("REPLACE(s, vb, '-')", mysql_8_0_18, mariadb()),
        ("LOCATE(s, vb)", mysql_8_4, mariadb()),
        ("REPLACE(LOWER(s), 'a', '-')", mysql_8_4, mariadb()),
    ] {
        let text = format!("SELECT {call} AS a FROM g");
        let (sql, rules) = transpiled(&text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{text}: {rules:?}");
    }
    // The character set of the string unknown, an argument of unknown type.
    for (call, input, target) in [
        ("REPLACE(s, vb, '-')", mysql_8_4, mariadb()),
        ("LOCATE(vb, s)", mysql_8_4, mariadb()),
        ("REPLACE(s, IFNULL(s, vb), '-')", mariadb(), mysql_8_4),
        ("INSTR(s, LOWER(s))", mysql_8_4, mariadb()),
        // The start counted in characters, then the bytes.
        ("LOCATE(vb, s, 2)", mariadb(), mysql_8_4),
    ] {
        let text = format!("SELECT {call} AS a FROM g");
        assert_eq!(
            errors(&text, input, output(target)),
            ["function.binary-argument"],
            "{text}"
        );
    }
}

#[test]
fn trim_and_position_keep_their_form() {
    let text = concat!(
        "SELECT TRIM(s) AS a, TRIM(BOTH FROM s) AS b, TRIM('x' FROM s) AS c, ",
        "TRIM(LEADING 'x' FROM s) AS e, TRIM(TRAILING s FROM 'xax') AS h, ",
        "POSITION('b' IN s) AS j, POSITION(de + 1 IN s) AS k, POSITION(vb IN vb) AS l FROM g"
    );
    for (input, output) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mysql(), mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // The keywords are written in capitals, a comparison before `IN`
    // between parentheses.
    let (sql, _) = transpiled(
        "SELECT trim(both 'x' from s) AS a, position(('a' = s) in s) AS b FROM g",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "SELECT TRIM(BOTH 'x' FROM s) AS a, POSITION(('a' = s) IN s) AS b FROM g"
    );
}

#[test]
fn trim_and_position_read_a_binary_argument_as_the_input() {
    let mysql_8_4 = mysql_at(8, 4, 0);
    let (sql, rules) = transpiled(
        "SELECT TRIM(BOTH vb FROM s) AS a, POSITION(vb IN s) AS b, TRIM(vb FROM vb) AS c FROM g",
        mariadb(),
        output(mysql_8_4),
    );
    assert_eq!(
        sql,
        concat!(
            "SELECT TRIM(BOTH vb FROM CAST(s AS BINARY)) AS a, ",
            "POSITION(vb IN CAST(s AS BINARY)) AS b, TRIM(vb FROM vb) AS c FROM g"
        )
    );
    assert!(
        rules.iter().all(|rule| *rule == "function.binary-argument") && !rules.is_empty(),
        "{rules:?}"
    );
    // Towards MariaDB, the character set of the string is unknown.
    for text in [
        "SELECT TRIM(vb FROM s) FROM g",
        "SELECT POSITION(vb IN s) FROM g",
        "SELECT TRIM(LOWER(s) FROM s) FROM g",
    ] {
        assert_eq!(
            errors(text, mysql_8_4, output(mariadb())),
            ["function.binary-argument"],
            "{text}"
        );
    }
    // Between two versions of MySQL, as written.
    let text = "SELECT TRIM(vb FROM s) AS a FROM g";
    let (sql, rules) = transpiled(text, mysql_at(8, 0, 18), output(mysql_8_4));
    assert_eq!(sql, text);
    assert!(rules.is_empty(), "{rules:?}");
}

#[test]
fn a_comparison_compares_the_tests_as_written() {
    // Each output reads the tests tighter than the comparisons.
    for (text, expected) in [
        (
            "SELECT 2 = 2 BETWEEN 1 AND 2 AS a FROM g",
            "SELECT 2 = (2 BETWEEN 1 AND 2) AS a FROM g",
        ),
        (
            "SELECT s = s LIKE 'a' AS a FROM g",
            "SELECT s = (s LIKE 'a') AS a FROM g",
        ),
        (
            "SELECT 1 BETWEEN 0 AND 2 IN (1) AS a FROM g",
            "SELECT 1 BETWEEN 0 AND (2 IN (1)) AS a FROM g",
        ),
        (
            "SELECT 2 = NULL IS NULL AS a FROM g",
            "SELECT (2 = NULL) IS NULL AS a FROM g",
        ),
    ] {
        for (input, output) in [
            (mysql(), mysql()),
            (mariadb(), mysql()),
            (mysql(), mariadb()),
        ] {
            let (sql, rules) = transpiled(text, input, self::output(output));
            assert_eq!(sql, expected, "{text}");
            assert!(rules.is_empty(), "{rules:?}");
        }
    }
    // The tests MariaDB chains, and its patterns, between parentheses
    // towards MySQL.
    let (sql, _) = transpiled(
        "SELECT s IN ('a') IN (1) AS a, s LIKE s LIKE 1 AS b, s LIKE 1 + 1 ESCAPE '!' AS c FROM g",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "SELECT (s IN ('a')) IN (1) AS a, (s LIKE s) LIKE 1 AS b, s LIKE (1 + 1) ESCAPE '!' AS c FROM g"
    );
}

#[test]
fn is_true_is_written_between_parentheses_where_mysql_needs_it() {
    let text = concat!(
        "SELECT de IS TRUE AS a, de IS NOT FALSE AS b, s = s IS UNKNOWN AS c, ",
        "NOT de IS TRUE AS e, de IS NULL IS FALSE AS h, (de IS TRUE) + 1 AS j FROM g ",
        "WHERE de IS TRUE AND s IS NOT UNKNOWN"
    );
    for (input, output) in [
        (mysql(), mariadb()),
        (mariadb(), mysql()),
        (mysql(), mysql()),
    ] {
        let (sql, rules) = transpiled(text, input, self::output(output));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
    // MariaDB reads a comparison or `IS` after it, MySQL between
    // parentheses.
    let (sql, _) = transpiled(
        "SELECT de IS TRUE = 1 AS a, de IS TRUE IS NULL AS b, (NOT de) IS TRUE AS c FROM g",
        mariadb(),
        output(mysql()),
    );
    assert_eq!(
        sql,
        "SELECT (de IS TRUE) = 1 AS a, (de IS TRUE) IS NULL AS b, (NOT de) IS TRUE AS c FROM g"
    );
}

#[test]
fn a_json_read_as_a_boolean_is_read_as_the_input_reads_it() {
    let mysql_8_0_16 = mysql_at(8, 0, 16);
    let mysql_8_4 = mysql_at(8, 4, 0);
    // Towards MySQL from MariaDB, read as its text.
    for (text, written) in [
        (
            "SELECT NOT js AS a FROM g",
            "SELECT NOT CAST(js AS CHAR) AS a FROM g",
        ),
        (
            "SELECT s FROM g WHERE js AND s IS NOT NULL",
            "SELECT s FROM g WHERE CAST(js AS CHAR) AND s IS NOT NULL",
        ),
        (
            "SELECT js IS TRUE AS a FROM g",
            "SELECT CAST(js AS CHAR) IS TRUE AS a FROM g",
        ),
        (
            "SELECT CASE WHEN js THEN 1 END AS a FROM g",
            "SELECT CASE WHEN CAST(js AS CHAR) THEN 1 END AS a FROM g",
        ),
        (
            "SELECT IF(js, 1, 0) AS a FROM g",
            "SELECT IF(CAST(js AS CHAR), 1, 0) AS a FROM g",
        ),
    ] {
        for target in [mysql_8_0_16, mysql_8_4] {
            let (sql, rules) = transpiled(text, mariadb(), output(target));
            assert_eq!(sql, written);
            assert_eq!(rules, ["boolean.json"], "{text}");
        }
    }
    // Towards MariaDB, and across MySQL 8.0.17 for the clauses and the
    // operators: an error.
    for text in [
        "SELECT NOT js FROM g",
        "SELECT js AND 1 FROM g",
        "SELECT js IS TRUE FROM g",
        "SELECT s FROM g WHERE js",
    ] {
        for (input, target) in [
            (mysql_8_4, mariadb()),
            (mysql_8_0_16, mariadb()),
            (mysql_8_4, mysql_8_0_16),
            (mysql_8_0_16, mysql_8_4),
        ] {
            assert_eq!(
                errors(text, input, output(target)),
                ["boolean.json"],
                "{text}"
            );
        }
    }
    for text in [
        "SELECT CASE WHEN js THEN 1 END FROM g",
        "SELECT IF(js, 1, 0) FROM g",
    ] {
        assert_eq!(errors(text, mysql_8_4, output(mariadb())), ["boolean.json"]);
    }
    // Read alike: the same reading, a value of `JSON`, a test of NULL, a
    // condition of `IF` and `CASE WHEN` between MySQL versions.
    for (text, input, target) in [
        ("SELECT NOT js AS a FROM g", mysql_8_4, mysql_at(8, 0, 17)),
        (
            "SELECT js IS NULL AS a, js AS b FROM g",
            mysql_8_4,
            mariadb(),
        ),
        (
            "SELECT js IS NULL AS a, js AS b FROM g",
            mariadb(),
            mysql_8_4,
        ),
        (
            "SELECT CASE WHEN js THEN 1 END AS a FROM g",
            mysql_8_4,
            mysql_8_0_16,
        ),
        ("SELECT IF(js, 1, 0) AS a FROM g", mysql_8_0_16, mysql_8_4),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
}

#[test]
fn a_fraction_read_as_a_boolean_is_read_as_the_input_reads_it() {
    let mysql_8_0_16 = mysql_at(8, 0, 16);
    let mysql_8_4 = mysql_at(8, 4, 0);
    let mariadb_11_8_9 = InputDialect {
        dialect: Dialect::MariaDb,
        version: Version::new(11, 8, 9),
    };
    for (text, written, input, target) in [
        // Towards the reading as an integer, compared with 0.
        (
            "SELECT s FROM g WHERE s",
            "SELECT s FROM g WHERE s <> 0",
            mysql_8_4,
            mysql_8_0_16,
        ),
        (
            "SELECT s FROM g GROUP BY s HAVING s",
            "SELECT s FROM g GROUP BY s HAVING s <> 0",
            mariadb(),
            mysql_8_0_16,
        ),
        (
            "SELECT g.s FROM g JOIN t ON de",
            "SELECT g.s FROM g JOIN t ON de <> 0",
            mysql_8_4,
            mysql_8_0_16,
        ),
        (
            "SELECT s XOR 1 AS a FROM g",
            "SELECT s <> 0 XOR 1 AS a FROM g",
            mysql_8_4,
            mariadb(),
        ),
        (
            "SELECT f XOR i AS a FROM d",
            "SELECT f <> 0 XOR i AS a FROM d",
            mariadb_11_8_9,
            mysql_8_0_16,
        ),
        // The left operand of `NOT (a XOR b)` is read alike.
        (
            "SELECT NOT (s XOR de) AS a FROM g",
            "SELECT NOT (s XOR de <> 0) AS a FROM g",
            mysql_8_4,
            mysql_8_0_16,
        ),
        // Towards the comparison, converted to an integer.
        (
            "SELECT s FROM g WHERE s",
            "SELECT s FROM g WHERE CAST(s AS SIGNED)",
            mysql_8_0_16,
            mysql_8_4,
        ),
        (
            "SELECT s FROM g WHERE vb",
            "SELECT s FROM g WHERE CAST(vb AS SIGNED)",
            mysql_8_0_16,
            mariadb(),
        ),
        (
            "SELECT de XOR 1 AS a FROM g",
            "SELECT CAST(de AS SIGNED) XOR 1 AS a FROM g",
            mariadb(),
            mysql_8_4,
        ),
        (
            "SELECT dt3 XOR 1 AS a FROM g",
            "SELECT CAST(dt3 AS SIGNED) XOR 1 AS a FROM g",
            mariadb(),
            mariadb_11_8_9,
        ),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, written);
        assert_eq!(rules, ["boolean.fraction"], "{text}");
    }
    // A time, or a value of unknown type, of another dialect read as an
    // integer.
    for (text, input, target) in [
        ("SELECT s FROM g WHERE dt3", mysql_8_0_16, mariadb()),
        ("SELECT dt3 XOR 1 FROM g", mariadb(), mysql_8_0_16),
        ("SELECT dt3 XOR 1 FROM g", mariadb(), mysql_8_4),
        ("SELECT s FROM g WHERE LENGTH(s)", mysql_8_0_16, mariadb()),
    ] {
        assert_eq!(
            errors(text, input, output(target)),
            ["boolean.fraction"],
            "{text}"
        );
    }
    // Read alike.
    for (text, input, target) in [
        ("SELECT s FROM g WHERE s", mysql_8_4, mariadb()),
        ("SELECT s XOR 1 AS a FROM g", mysql_8_0_16, mariadb()),
        ("SELECT NOT (s XOR 1) AS a FROM g", mysql_8_4, mysql_8_0_16),
        ("SELECT s FROM g WHERE dd AND tm", mysql_8_0_16, mysql_8_4),
        (
            "SELECT n FROM d WHERE i XOR n",
            mysql_8_0_16,
            mariadb_11_8_9,
        ),
        (
            "SELECT s FROM g WHERE NOT de OR s IS TRUE",
            mysql_8_0_16,
            mysql_8_4,
        ),
    ] {
        let (sql, rules) = transpiled(text, input, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
}

#[test]
fn a_json_compared_is_read_as_the_input_reads_it() {
    let mysql_8_4 = mysql_at(8, 4, 0);
    // Towards MySQL from MariaDB, read as its text.
    for (text, written, rules) in [
        (
            "SELECT s FROM g WHERE js = 'b'",
            "SELECT s FROM g WHERE CONCAT(js) = 'b'",
            &["json.text"][..],
        ),
        (
            "SELECT s FROM g WHERE js IN ('b', 1) AND js + 0 > 1",
            "SELECT s FROM g WHERE CONCAT(js) IN ('b', 1) AND CONCAT(js) + 0 > 1",
            &["json.text", "json.text"],
        ),
        (
            "SELECT js FROM g ORDER BY js",
            "SELECT js FROM g ORDER BY CONCAT(js)",
            &["json.text"],
        ),
        (
            "SELECT js AS j FROM g ORDER BY j, 1",
            "SELECT js AS j FROM g ORDER BY CONCAT(js), CONCAT(js)",
            &["json.text"],
        ),
        (
            "SELECT DISTINCT js FROM g",
            "SELECT DISTINCT CONCAT(js) AS `js` FROM g",
            &["json.text", "column.implicit-name"],
        ),
        (
            "SELECT js, COUNT(*) AS n FROM g GROUP BY js",
            "SELECT CONCAT(js) AS `js`, COUNT(*) AS n FROM g GROUP BY CONCAT(js)",
            &["column.implicit-name", "json.text"],
        ),
        (
            "SELECT s FROM g GROUP BY s, js HAVING js < '2'",
            "SELECT s FROM g GROUP BY s, CONCAT(js), js HAVING CONCAT(js) < '2'",
            &["json.text"],
        ),
        (
            "SELECT MAX(js) AS m, COUNT(DISTINCT js) AS n FROM g",
            "SELECT MAX(CONCAT(js)) AS m, COUNT(DISTINCT CONCAT(js)) AS n FROM g",
            &["json.text", "json.text"],
        ),
        (
            "SELECT js FROM g UNION SELECT js FROM g",
            "SELECT CONCAT(js) AS `js` FROM g UNION SELECT CONCAT(js) FROM g",
            &["json.text", "column.implicit-name", "json.text"],
        ),
        (
            "SELECT s FROM g WHERE 'b' IN (SELECT js FROM g)",
            "SELECT s FROM g WHERE 'b' IN (SELECT CONCAT(js) FROM g)",
            &["json.text"],
        ),
        (
            "SELECT s FROM g WHERE (SELECT js FROM g LIMIT 1) = 'b'",
            "SELECT s FROM g WHERE CONCAT((SELECT js FROM g LIMIT 1)) = 'b'",
            &["json.text"],
        ),
        (
            "SELECT CAST(js AS SIGNED) AS a, ROW_NUMBER() OVER (PARTITION BY js) AS r FROM g",
            "SELECT CAST(CONCAT(js) AS SIGNED) AS a, ROW_NUMBER() OVER (PARTITION BY CONCAT(js)) AS r FROM g",
            &["json.text", "json.text"],
        ),
        (
            "SELECT q.js FROM (SELECT js FROM g) AS q WHERE q.js = 'b'",
            "SELECT q.js FROM (SELECT js FROM g) AS q WHERE CONCAT(q.js) = 'b'",
            &["json.text"],
        ),
    ] {
        let (sql, steps) = transpiled(text, mariadb(), output(mysql_8_4));
        assert_eq!(sql, written);
        assert_eq!(steps, rules, "{text}");
    }
    // Read alike: its text or its value.
    for text in [
        "SELECT js, js IS NULL AS a, js LIKE '%b%' AS b, CONCAT(js, s) AS c, CAST(js AS CHAR) AS d FROM g",
        "SELECT COUNT(js) AS n, GROUP_CONCAT(js) AS c FROM g",
        "SELECT COUNT(js) OVER () AS n FROM g",
        "SELECT js FROM g UNION ALL SELECT js FROM g",
    ] {
        for (input, target) in [(mariadb(), mysql_8_4), (mysql_8_4, mariadb())] {
            let (sql, rules) = transpiled(text, input, output(target));
            assert_eq!(sql, text);
            assert!(rules.is_empty(), "{rules:?}");
        }
    }
    assert_eq!(
        errors(
            "SELECT JSON_TYPE(js) AS t FROM g",
            mariadb(),
            output(mysql_8_4)
        ),
        ["function.unstudied"]
    );
    // Towards MariaDB from MySQL, and a join `USING` a `JSON`: an error.
    for text in [
        "SELECT s FROM g WHERE js = 'b'",
        "SELECT js FROM g ORDER BY js",
        "SELECT js + 0 AS a FROM g",
    ] {
        assert_eq!(
            errors(text, mysql_8_4, output(mariadb())),
            ["json.text"],
            "{text}"
        );
    }
    assert_eq!(
        errors("TABLE g UNION TABLE g", mysql_8_4, output(mariadb())),
        ["json.text", "json.text"]
    );
    for text in [
        "SELECT g.s FROM g JOIN g AS h USING (js)",
        "SELECT s FROM g GROUP BY s, js WITH ROLLUP HAVING js < '2'",
    ] {
        assert_eq!(
            errors(text, mariadb(), output(mysql_8_4)),
            ["json.text"],
            "{text}"
        );
    }
}

#[test]
fn in_of_a_json_list_is_read_as_the_input_reads_it() {
    let mysql_8_0_16 = mysql_at(8, 0, 16);
    let mysql_8_4 = mysql_at(8, 4, 0);
    for (text, written) in [
        (
            "SELECT s FROM g WHERE js IN ('b', 1)",
            "SELECT s FROM g WHERE js = 'b' OR js = 1",
        ),
        (
            "SELECT js NOT IN ('b', 1) AS a FROM g",
            "SELECT js <> 'b' AND js <> 1 AS a FROM g",
        ),
        (
            "SELECT s FROM g WHERE js IN ('b', 1) AND s = 'x'",
            "SELECT s FROM g WHERE (js = 'b' OR js = 1) AND s = 'x'",
        ),
    ] {
        let (sql, rules) = transpiled(text, mysql_8_4, output(mysql_8_0_16));
        assert_eq!(sql, written);
        assert_eq!(rules, ["json.in"], "{text}");
    }
    for (text, input, target) in [
        (
            "SELECT s FROM g WHERE js IN ('b', 1)",
            mysql_8_0_16,
            mysql_8_4,
        ),
        (
            "SELECT s FROM g WHERE 'b' IN (js, 'x')",
            mysql_8_4,
            mysql_8_0_16,
        ),
        (
            "SELECT s FROM g WHERE (SELECT js FROM g LIMIT 1) IN ('b', 1)",
            mysql_8_4,
            mysql_8_0_16,
        ),
    ] {
        assert_eq!(errors(text, input, output(target)), ["json.in"], "{text}");
    }
    for (text, target) in [
        ("SELECT s FROM g WHERE js IN ('b')", mysql_8_0_16),
        ("SELECT s FROM g WHERE js IN ('b', 1)", mysql_at(8, 0, 17)),
    ] {
        let (sql, rules) = transpiled(text, mysql_8_4, output(target));
        assert_eq!(sql, text);
        assert!(rules.is_empty(), "{rules:?}");
    }
}
