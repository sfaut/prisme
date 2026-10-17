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

/// `t (a INT NOT NULL, b INT NULL)` and `u (a INT NOT NULL, c INT NULL)`.
fn catalog() -> Tables {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let mut tables = Tables::default();
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
        comments: &[],
    };
    let mut emitter = Emitter {
        out: String::new(),
        output: &ansi,
        source: &source,
        next_comment: 0,
        quiet: 0,
        blocks: Vec::new(),
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
        ("SELECT NOW()", output(mariadb()), "function.now"),
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
