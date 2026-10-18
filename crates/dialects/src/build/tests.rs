use prisme_plan::{ExprKind, SortReference};
use prisme_types::{DataType, IntegerSize, IntegerType, TextType};

use super::*;
use crate::lexer::SqlMode;
use crate::sql::{ParseOptions, parse};
use crate::versions::Version;

/// `t (a INT NOT NULL, b VARCHAR(10) NULL)`, `db.u (a INT NOT NULL)`,
/// `a (k INT NULL, x INT NOT NULL)` and `b (k INT NOT NULL, y INT NULL)`.
fn catalog() -> Tables {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let mut tables = Tables::default();
    tables.insert(
        None,
        "a",
        vec![
            (Ident::new("k"), Type::nullable(int.clone())),
            (Ident::new("x"), Type::not_null(int.clone())),
        ],
    );
    tables.insert(
        None,
        "b",
        vec![
            (Ident::new("k"), Type::not_null(int.clone())),
            (Ident::new("y"), Type::nullable(int.clone())),
        ],
    );
    tables.insert(
        None,
        "t",
        vec![
            (Ident::new("a"), Type::not_null(int.clone())),
            (
                Ident::new("b"),
                Type::nullable(types::string_literal("0123456789")),
            ),
        ],
    );
    tables.insert(
        Some("db"),
        "u",
        vec![(Ident::new("a"), Type::not_null(int))],
    );
    tables
}

/// Plan of `text`, written in `dialect`.
fn plan_in(text: &str, dialect: Dialect) -> Result<Plan, BuildError> {
    let version = match dialect {
        Dialect::MySql => Version::new(8, 0, 19),
        Dialect::MariaDb => Version::new(10, 6, 3),
    };
    plan_at(text, dialect, version)
}

/// Plan of `text`, written in `dialect` and its `version`.
fn plan_at(text: &str, dialect: Dialect, version: Version) -> Result<Plan, BuildError> {
    let options = ParseOptions {
        dialect,
        version,
        mode: SqlMode::default(),
        conditional_version: version,
    };
    let parsed = parse(text, &options).unwrap_or_else(|error| panic!("{text}: {error}"));
    let prisme_plan::Statement::Query(plan) =
        build(&parsed.statement, text, dialect, version, &catalog())?
    else {
        panic!("not a query");
    };
    Ok(plan)
}

/// Plan of `text`, written in MySQL.
fn plan(text: &str) -> Plan {
    plan_in(text, Dialect::MySql).unwrap()
}

/// Error of `text`, written in MySQL.
fn error(text: &str) -> String {
    plan_in(text, Dialect::MySql).unwrap_err().message
}

#[test]
fn builds_the_example_of_the_dialects() {
    let plan = plan("WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds");
    let schema = plan.schema();
    assert_eq!(schema.len(), 1);
    let column = &schema.columns[0];
    assert_eq!(column.qualifier, Some(Ident::new("ds")));
    assert_eq!(column.name, Ident::new("c"));
    assert_eq!(column.ty, Type::not_null(types::bigint()));
    let Plan::With(with) = plan else {
        panic!("WITH expected");
    };
    let Plan::Values(values) = &with.ctes[0].plan else {
        panic!("VALUES expected");
    };
    assert_eq!(values.form, ValuesForm::Row);
    assert!(matches!(*with.body, Plan::Project(_)));
}

#[test]
fn names_the_columns_of_values_after_the_dialect() {
    let names = |text, dialect| -> Vec<String> {
        plan_in(text, dialect)
            .unwrap()
            .schema()
            .columns
            .into_iter()
            .map(|column| column.name.value)
            .collect()
    };
    assert_eq!(
        names("VALUES ROW(1, 'x', NULL)", Dialect::MySql),
        ["column_0", "column_1", "column_2"]
    );
    assert_eq!(
        names("VALUES (1, 'x', NULL)", Dialect::MariaDb),
        ["1", "x", "NULL"]
    );
}

#[test]
fn types_the_columns_of_values_from_all_rows() {
    let schema = plan("VALUES ROW(1, NULL), ROW(2.5, 'abc')").schema();
    // BIGINT (19 digits) and DECIMAL(2, 1): 19 digits before the point, 1 after.
    assert_eq!(schema.columns[0].ty, Type::not_null(types::decimal(20, 1)));
    assert_eq!(
        schema.columns[1].ty,
        Type::nullable(types::string_literal("abc"))
    );
    assert_eq!(
        error("VALUES ROW(1), ROW(1, 2)"),
        "the rows of `VALUES` do not have the same number of columns"
    );
}

#[test]
fn resolves_the_columns_of_tables_and_aliases() {
    let schema = plan("SELECT x.a, b AS c, a + 1 FROM t AS x").schema();
    let names: Vec<&str> = schema
        .columns
        .iter()
        .map(|column| column.name.value.as_str())
        .collect();
    assert_eq!(names, ["a", "c", "a + 1"]);
    assert!(schema.columns[1].ty.nullable);
    assert_eq!(schema.columns[2].ty, Type::not_null(types::bigint()));
    assert_eq!(plan("SELECT * FROM db.u").schema().len(), 1);
    assert_eq!(error("SELECT z FROM t"), "unknown column `z`");
    assert_eq!(error("SELECT t.a FROM t AS x"), "unknown column `t.a`");
    assert_eq!(error("SELECT a FROM nowhere"), "unknown table `nowhere`");
}

#[test]
fn converts_between_booleans_and_numbers() {
    let Plan::Project(project) = plan("SELECT a = 1 FROM t WHERE a") else {
        panic!("projection expected");
    };
    let ProjectItem::Expr(named) = &project.items[0] else {
        panic!("expression expected");
    };
    assert!(matches!(named.expr.kind, ExprKind::ToInteger(_)));
    assert_eq!(named.expr.ty.data_type, types::bigint());
    let Plan::Filter(filter) = project.input.as_ref() else {
        panic!("filter expected");
    };
    assert!(matches!(filter.predicate.kind, ExprKind::ToBoolean(_)));
}

#[test]
fn sort_keys_designate_an_alias_a_position_or_an_expression() {
    let Plan::Project(project) = plan("SELECT a AS x, b FROM t ORDER BY x DESC, 2, a + 1") else {
        panic!("projection expected");
    };
    let Plan::Sort(sort) = project.input.as_ref() else {
        panic!("sort below the projection expected");
    };
    let references: Vec<SortReference> = sort.keys.iter().map(|key| key.form.reference).collect();
    assert_eq!(
        references,
        [
            SortReference::Alias,
            SortReference::Position,
            SortReference::Expression
        ]
    );
    // NULL is the smallest value: last in a descending sort.
    assert_eq!(sort.keys[0].nulls, Nulls::Last);
    assert_eq!(sort.keys[1].nulls, Nulls::First);
    assert_eq!(
        error("SELECT a FROM t ORDER BY 3"),
        "unknown column `3` in `ORDER BY`"
    );
}

#[test]
fn lists_the_tables_outside_the_common_table_expressions() {
    let options = ParseOptions {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        mode: SqlMode::default(),
        conditional_version: Version::new(8, 0, 19),
    };
    let text = "WITH c AS (SELECT a FROM t) SELECT * FROM c";
    let parsed = parse(text, &options).unwrap();
    let tables = referenced_tables(&parsed.statement);
    assert_eq!(
        tables,
        [TableName {
            database: None,
            name: Ident::new("t")
        }]
    );
}

#[test]
fn a_join_on_names_merges_its_columns() {
    let columns = |text| -> Vec<(String, bool)> {
        plan(text)
            .schema()
            .columns
            .into_iter()
            .map(|column| (column.name.value, column.ty.nullable))
            .collect()
    };
    let column = |name: &str, nullable| (name.to_string(), nullable);
    assert_eq!(
        columns("SELECT * FROM a JOIN b USING (k)"),
        [column("k", true), column("x", false), column("y", true)]
    );
    assert_eq!(
        columns("SELECT * FROM a RIGHT JOIN b USING (k)"),
        [column("k", false), column("y", true), column("x", true)]
    );
    assert_eq!(
        columns("SELECT a.*, b.* FROM a NATURAL LEFT JOIN b"),
        [
            column("k", true),
            column("x", false),
            column("k", true),
            column("y", true)
        ]
    );
    assert_eq!(
        columns("SELECT k, a.k, b.k FROM a JOIN b USING (k)"),
        [column("k", false), column("k", true), column("k", false)]
    );
    assert_eq!(error("SELECT k FROM a, b"), "column `k` is ambiguous");
    assert_eq!(
        error("SELECT 1 FROM a JOIN b USING (z)"),
        "unknown column `z` in `USING`"
    );
    assert_eq!(
        error("SELECT 1 FROM a JOIN a AS c ON 1 JOIN b USING (k)"),
        "column `k` is ambiguous in `USING`"
    );
}

#[test]
fn the_condition_of_a_join_reads_both_sides() {
    let Plan::Project(project) = plan("SELECT * FROM a LEFT JOIN b ON a.k = b.k AND y > 1") else {
        panic!("projection expected");
    };
    let Plan::Join(join) = project.input.as_ref() else {
        panic!("join expected");
    };
    assert_eq!(join.kind, JoinKind::Left);
    assert!(join.condition.is_some());
    assert_eq!(
        error("SELECT 1 FROM a JOIN (b JOIN t ON x = 1) ON 1"),
        "unknown column `x`"
    );
    let Plan::Project(project) = plan("SELECT * FROM a, b") else {
        panic!("projection expected");
    };
    assert!(matches!(project.input.as_ref(), Plan::Join(join) if join.kind == JoinKind::Cross));
}

#[test]
fn a_derived_table_names_its_columns() {
    let schema = plan("SELECT d.p, q FROM (SELECT k, x FROM a) AS d (p, q)").schema();
    assert_eq!(schema.columns[0].name, Ident::new("p"));
    assert!(error("SELECT * FROM (SELECT 1) AS d (p, q)").contains("columns"));
    assert_eq!(
        plan("SELECT * FROM a, LATERAL (SELECT a.x + 1 AS n) AS d")
            .schema()
            .len(),
        3
    );
    assert_eq!(
        error("SELECT * FROM a, (SELECT a.x) AS d"),
        "unknown column `a.x`"
    );
}

#[test]
fn an_outer_reference_in_a_derived_table_follows_the_version() {
    let text = "SELECT (SELECT n FROM (SELECT a.x AS n) AS d) FROM a";
    assert!(plan_in(text, Dialect::MySql).is_ok());
    let message = plan_in(text, Dialect::MariaDb).unwrap_err().message;
    assert!(message.contains("outer reference"), "{message}");
}

#[test]
fn a_set_operation_types_its_columns_from_both_sides() {
    let schema = plan("SELECT k, 1 FROM a UNION SELECT y, 'abc' FROM b").schema();
    assert_eq!(schema.columns[0].name, Ident::new("k"));
    assert!(schema.columns[0].ty.nullable);
    assert!(matches!(schema.columns[1].ty.data_type, DataType::Text(_)));
    assert_eq!(
        error("SELECT 1 UNION SELECT 1, 2"),
        "the sides of the set operation have 1 and 2 columns"
    );
    let Plan::Limit(limit) = plan("TABLE a UNION ALL TABLE b ORDER BY k LIMIT 1") else {
        panic!("limit expected");
    };
    assert!(matches!(limit.input.as_ref(), Plan::Sort(_)));
}

#[test]
fn subqueries_read_the_columns_of_the_enclosing_queries() {
    let schema =
        plan("SELECT (SELECT y FROM b WHERE b.k = a.k LIMIT 1) AS y FROM a WHERE EXISTS (SELECT 1 FROM b WHERE y = x)")
            .schema();
    assert!(schema.columns[0].ty.nullable);
    assert!(
        plan_in(
            "SELECT 1 FROM a WHERE (k, x) = ANY (TABLE b)",
            Dialect::MySql
        )
        .is_ok()
    );
    assert!(
        plan_in(
            "SELECT 1 FROM a WHERE k NOT IN (SELECT k FROM b)",
            Dialect::MySql
        )
        .is_ok()
    );
    assert_eq!(
        error("SELECT (SELECT k, y FROM b)"),
        "subquery of 2 columns: row subqueries are not supported yet by Prisme"
    );
    assert_eq!(
        error("SELECT (1, 2)"),
        "a row of 2 values where a single value is expected"
    );
    assert_eq!(
        error("SELECT 1 FROM a WHERE (k, x) = (1, 2, 3)"),
        "operands of 2 and 3 values are compared"
    );
    assert_eq!(
        error("SELECT 1 FROM a WHERE (k, x) > ANY (TABLE b)"),
        "a row of 2 values where a single value is expected"
    );
    assert_eq!(
        error("SELECT 1 FROM a WHERE k IN (TABLE b)"),
        "2 columns where 1 are expected"
    );
}

#[test]
fn lists_the_tables_of_joins_derived_tables_and_subqueries() {
    let options = ParseOptions {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        mode: SqlMode::default(),
        conditional_version: Version::new(8, 0, 19),
    };
    let text = "WITH c AS (TABLE a) SELECT * FROM c JOIN (SELECT * FROM b) AS d ON 1 \
                WHERE EXISTS (SELECT 1 FROM t) UNION TABLE db.u";
    let parsed = parse(text, &options).unwrap();
    let names: Vec<String> = referenced_tables(&parsed.statement)
        .into_iter()
        .map(|table| table.name.value)
        .collect();
    assert_eq!(names, ["a", "b", "t", "u"]);
}

/// Type of each column of `text`, written in `dialect`: its data type and
/// whether it is nullable.
fn column_types(text: &str, dialect: Dialect) -> Vec<(DataType, bool)> {
    plan_in(text, dialect)
        .unwrap_or_else(|error| panic!("{text}: {error}"))
        .schema()
        .columns
        .into_iter()
        .map(|column| (column.ty.data_type, column.ty.nullable))
        .collect()
}

/// Aggregation of the plan of `text`, under its projection, `HAVING` and
/// windows.
fn aggregation(text: &str) -> prisme_plan::Aggregate {
    let mut node = plan(text);
    loop {
        node = match node {
            Plan::Aggregate(aggregate) => return aggregate,
            Plan::Project(Project { input, .. })
            | Plan::Filter(Filter { input, .. })
            | Plan::Sort(Sort { input, .. })
            | Plan::Window(prisme_plan::Window { input, .. }) => *input,
            other => panic!("{text}: no aggregation in {other:?}"),
        };
    }
}

#[test]
fn types_the_aggregate_functions_as_measured() {
    let int = |size| DataType::Integer(IntegerType::signed(size));
    let bigint_unsigned = DataType::Integer(IntegerType::unsigned(IntegerSize::Big));
    assert_eq!(
        column_types(
            "SELECT COUNT(*), SUM(x), AVG(x), MIN(k), MAX(x), BIT_OR(x), STD(x) FROM a",
            Dialect::MySql
        ),
        [
            (int(IntegerSize::Big), false),
            (types::decimal(32, 0), true),
            (types::decimal(14, 4), true),
            (int(IntegerSize::Int), true),
            (int(IntegerSize::Int), true),
            (bigint_unsigned, false),
            (types::double(), true),
        ]
    );
    // With keys, a group is never empty: the functions of a column that
    // is not null are not null.
    assert_eq!(
        column_types("SELECT k, SUM(x), MIN(k) FROM a GROUP BY k", Dialect::MySql),
        [
            (int(IntegerSize::Int), true),
            (types::decimal(32, 0), false),
            (int(IntegerSize::Int), true),
        ]
    );
    assert_eq!(
        column_types("SELECT x FROM a GROUP BY x WITH ROLLUP", Dialect::MySql),
        [(int(IntegerSize::Int), true)]
    );
}

#[test]
fn types_the_window_functions_after_the_dialect() {
    let text = "SELECT ROW_NUMBER() OVER (), PERCENT_RANK() OVER (ORDER BY x), \
                LAG(x) OVER (ORDER BY x), SUM(x) OVER () FROM a";
    let bigint = DataType::Integer(IntegerType::signed(IntegerSize::Big));
    let bigint_unsigned = DataType::Integer(IntegerType::unsigned(IntegerSize::Big));
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    assert_eq!(
        column_types(text, Dialect::MySql),
        [
            (bigint_unsigned, false),
            (types::double(), false),
            (bigint.clone(), true),
            (types::decimal(32, 0), true),
        ]
    );
    assert_eq!(
        column_types(text, Dialect::MariaDb),
        [
            (bigint, false),
            (types::double(), true),
            (int, true),
            (types::decimal(32, 0), true),
        ]
    );
}

#[test]
fn a_grouped_block_reads_its_keys_and_its_aggregates() {
    let Plan::Project(project) = plan("SELECT k FROM a WHERE x > 1 GROUP BY k HAVING COUNT(*) > 1")
    else {
        panic!("projection expected");
    };
    let Plan::Filter(having) = project.input.as_ref() else {
        panic!("`HAVING` expected");
    };
    let Plan::Aggregate(aggregate) = having.input.as_ref() else {
        panic!("aggregation expected");
    };
    assert!(matches!(aggregate.input.as_ref(), Plan::Filter(_)));
    assert_eq!(aggregate.keys[0].name, Ident::new("#key1"));
    let ProjectItem::Expr(item) = &project.items[0] else {
        panic!("expression expected");
    };
    assert!(matches!(&item.expr.kind, ExprKind::Column(column) if column.name.value == "#key1"));
    // The same call is read once; a column equal to a key reads the key.
    assert_eq!(
        aggregation("SELECT SUM(x), SUM(a.x) + 1 FROM a")
            .aggregates
            .len(),
        1
    );
    assert!(
        aggregation("SELECT k + 1 FROM a GROUP BY k + 1")
            .aggregates
            .is_empty()
    );
    assert!(
        aggregation("SELECT a.k FROM a GROUP BY k")
            .aggregates
            .is_empty()
    );
    // A column neither grouped nor aggregated reads any value of its group.
    let aggregate = aggregation("SELECT k, x FROM a GROUP BY k");
    assert!(aggregate.aggregates[0].call.implicit);
}

#[test]
fn a_grouping_key_designates_a_position_a_column_or_an_alias() {
    let references = |text| aggregation(text).references;
    assert_eq!(
        references("SELECT x AS z, COUNT(*) FROM a GROUP BY z, 1"),
        [SortReference::Alias, SortReference::Position]
    );
    // The columns of the source come before the aliases: `k` is `a.k`.
    let aggregate = aggregation("SELECT x AS k FROM a GROUP BY k");
    assert_eq!(aggregate.references, [SortReference::Expression]);
    assert!(aggregate.aggregates[0].call.implicit);
    // `HAVING` reads the aliases of the select list.
    let text = "SELECT k AS z, SUM(x) AS s FROM a GROUP BY k HAVING s > 10 AND z > 0";
    assert!(plan_in(text, Dialect::MySql).is_ok());
}

#[test]
fn aggregate_and_window_functions_stay_in_their_clauses() {
    for (text, message) in [
        (
            "SELECT x FROM a WHERE SUM(x) > 1",
            "invalid use of the aggregate function `SUM` in `WHERE`",
        ),
        (
            "SELECT SUM(SUM(x)) FROM a",
            "invalid use of the aggregate function `SUM` in the arguments of an aggregate function",
        ),
        (
            "SELECT SUM(x) FROM a GROUP BY SUM(x)",
            "invalid use of the aggregate function `SUM` in `GROUP BY`",
        ),
        (
            "SELECT SUM(x) AS s FROM a GROUP BY s",
            "grouping on `s`, which reads an aggregate or window function",
        ),
        (
            "SELECT * FROM a GROUP BY k",
            "`*` in a grouped query: not supported yet by Prisme",
        ),
        (
            "SELECT x FROM a HAVING x > 1",
            "`HAVING` without grouping: not supported yet by Prisme",
        ),
        (
            "SELECT ROW_NUMBER() FROM a",
            "`ROW_NUMBER` needs a window (`OVER`)",
        ),
        (
            "SELECT x FROM a WHERE ROW_NUMBER() OVER () > 1",
            "window function `ROW_NUMBER` in `WHERE`",
        ),
        (
            "SELECT k, ROW_NUMBER() OVER () AS r FROM a GROUP BY k HAVING r > 1",
            "window function in `HAVING`",
        ),
        (
            "SELECT `#key1` FROM (SELECT 1 AS `#key1`) AS d",
            "column `#key1`: the names `#key...`, `#aggregate...` and `#window...` are reserved by Prisme",
        ),
    ] {
        assert_eq!(error(text), message, "{text}");
    }
    // A window function reads the aggregation of its block.
    let text = "SELECT SUM(SUM(x)) OVER (ORDER BY k) FROM a GROUP BY k";
    assert!(plan_in(text, Dialect::MySql).is_ok());
    // Window functions of MariaDB.
    for (text, message) in [
        (
            "SELECT MEDIAN(x) OVER () FROM a",
            "`MEDIAN` with `OVER`: not supported yet by Prisme",
        ),
        (
            "SELECT MEDIAN(x) FROM a",
            "`MEDIAN` needs a window (`OVER`)",
        ),
    ] {
        let refused = plan_in(text, Dialect::MariaDb).unwrap_err();
        assert_eq!(refused.message, message, "{text}");
    }
    // Without a window, `LAST_VALUE` of MariaDB is a scalar function.
    assert!(plan_in("SELECT LAST_VALUE(x, k) FROM a", Dialect::MariaDb).is_ok());
}

#[test]
fn a_named_window_follows_the_rules_of_the_dialect() {
    let Plan::Project(project) =
        plan("SELECT SUM(x) OVER w FROM a WINDOW w AS (ROWS UNBOUNDED PRECEDING)")
    else {
        panic!("projection expected");
    };
    let Plan::Window(window) = project.input.as_ref() else {
        panic!("window expected");
    };
    assert_eq!(window.definitions.len(), 1);
    let ExprKind::Window(call) = &window.functions[0].expr.kind else {
        panic!("window function expected");
    };
    assert!(call.frame.is_some());
    for (text, message) in [
        (
            "SELECT SUM(x) OVER (w) FROM a WINDOW w AS (ROWS UNBOUNDED PRECEDING)",
            "window `w` has a frame: no window can complete it",
        ),
        (
            "SELECT SUM(x) OVER (w ORDER BY x) FROM a WINDOW w AS (ORDER BY k)",
            "window `w` has an `ORDER BY`: a window that completes it has none of its own",
        ),
        (
            "SELECT SUM(x) OVER (w PARTITION BY x) FROM a WINDOW w AS (ORDER BY k)",
            "a window that completes window `w` has no `PARTITION BY` of its own",
        ),
        (
            "SELECT 1 FROM a WINDOW w AS (), w AS ()",
            "window `w` is defined twice",
        ),
        (
            "SELECT 1 FROM a WINDOW w AS (w)",
            "the named windows complete each other in a cycle",
        ),
        ("SELECT SUM(x) OVER v FROM a", "unknown window `v`"),
    ] {
        assert_eq!(error(text), message, "{text}");
    }
    // MySQL reads the window a definition completes anywhere in the clause,
    // MariaDB before it.
    let text = "SELECT SUM(x) OVER w1 FROM a WINDOW w1 AS (w2), w2 AS (ORDER BY k)";
    assert!(plan_in(text, Dialect::MySql).is_ok());
    assert_eq!(
        plan_in(text, Dialect::MariaDb).unwrap_err().message,
        "window `w2` is not defined before window `w1`"
    );
}

#[test]
fn types_case_from_its_results_after_the_dialect() {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let types = |text: &str, dialect| column_types(&format!("SELECT {text} FROM t"), dialect);
    for dialect in [Dialect::MySql, Dialect::MariaDb] {
        assert_eq!(
            types(
                "CASE WHEN a > 1 THEN a END, CASE a WHEN 1 THEN a ELSE a END",
                dialect
            ),
            [(int.clone(), true), (int.clone(), false)]
        );
        assert_eq!(
            types("CASE WHEN a > 1 THEN b ELSE a END", dialect),
            [(
                DataType::Text(TextType {
                    length: None,
                    fixed: false,
                    charset: None,
                    collation: None
                }),
                true
            )]
        );
        // A predicate is a number.
        assert_eq!(
            types("CASE WHEN a THEN a > 1 ELSE NULL END", dialect)[0].0,
            DataType::Integer(types::boolean_integer(dialect))
        );
    }
    let integer = |size, unsigned| DataType::Integer(IntegerType { size, unsigned });
    let small = integer(IntegerSize::Small, false);
    let tiny_unsigned = integer(IntegerSize::Tiny, true);
    let big_unsigned = integer(IntegerSize::Big, true);
    for (results, mysql, mariadb) in [
        (
            vec![&small, &small],
            integer(IntegerSize::Int, false),
            small.clone(),
        ),
        (
            vec![&tiny_unsigned, &DataType::Null],
            integer(IntegerSize::Int, true),
            types::decimal(3, 0),
        ),
        (
            vec![&tiny_unsigned, &int],
            integer(IntegerSize::Int, false),
            types::decimal(10, 0),
        ),
        (
            vec![&integer(IntegerSize::Medium, false), &tiny_unsigned],
            integer(IntegerSize::Int, false),
            types::decimal(8, 0),
        ),
        (
            vec![&big_unsigned, &int],
            types::decimal(20, 0),
            types::decimal(20, 0),
        ),
        (
            vec![&big_unsigned, &tiny_unsigned],
            big_unsigned.clone(),
            big_unsigned.clone(),
        ),
    ] {
        assert_eq!(
            types::case(Dialect::MySql, results.clone()),
            mysql,
            "{results:?}"
        );
        assert_eq!(
            types::case(Dialect::MariaDb, results.clone()),
            mariadb,
            "{results:?}"
        );
    }
    // Dates and times give the larger `DATETIME`, a number with them a string.
    let datetime = |fraction| DataType::DateTime { fraction };
    for (results, merged) in [
        (vec![DataType::Date, datetime(3)], datetime(3)),
        (
            vec![DataType::Time { fraction: 2 }, DataType::Date],
            datetime(2),
        ),
        (
            vec![DataType::Timestamp { fraction: 6 }, datetime(0)],
            datetime(6),
        ),
        (
            vec![
                DataType::Timestamp { fraction: 1 },
                DataType::Timestamp { fraction: 6 },
            ],
            DataType::Timestamp { fraction: 6 },
        ),
        (
            vec![DataType::Date, int.clone()],
            DataType::Text(TextType {
                length: None,
                fixed: false,
                charset: None,
                collation: None,
            }),
        ),
        (vec![DataType::Json, DataType::Null], DataType::Json),
        (vec![DataType::Unknown, DataType::Date], DataType::Unknown),
    ] {
        assert_eq!(types::case(Dialect::MySql, &results), merged, "{results:?}");
    }
}

#[test]
fn types_cast_after_its_target() {
    let text = |length, charset: Option<&str>| {
        DataType::Text(TextType {
            length,
            fixed: false,
            charset: charset.map(str::to_string),
            collation: None,
        })
    };
    let binary = |length| {
        DataType::Binary(prisme_types::BinaryType {
            length,
            fixed: false,
        })
    };
    assert_eq!(
        column_types(
            concat!(
                "SELECT CAST(a AS SIGNED), CAST(a AS UNSIGNED), CAST(a AS DECIMAL), ",
                "CAST(a AS DECIMAL(0, 2)), CAST(a AS DOUBLE), CAST(a AS FLOAT(30)), ",
                "CAST(a AS CHAR(3)), CAST(a AS NCHAR), CAST(a AS CHAR BYTE), ",
                "CONVERT(b USING latin1), CAST(a AS BINARY(2)), CAST(a AS DATE), ",
                "CAST(a AS DATETIME(3)), CAST(b AS TIME) FROM t"
            ),
            Dialect::MySql
        ),
        [
            (types::bigint(), false),
            (
                DataType::Integer(IntegerType::unsigned(IntegerSize::Big)),
                false
            ),
            (types::decimal(10, 0), false),
            (types::decimal(10, 2), false),
            (types::double(), false),
            (types::double(), false),
            (text(Some(3), None), false),
            (text(None, Some("utf8mb3")), false),
            (binary(None), false),
            (text(Some(10), Some("latin1")), true),
            (binary(Some(2)), false),
            (DataType::Date, true),
            (DataType::DateTime { fraction: 3 }, true),
            (DataType::Time { fraction: 0 }, true),
        ]
    );
    // MariaDB: `INT` alone, a nullable `DOUBLE`.
    assert_eq!(
        column_types(
            "SELECT CAST(a AS INT), CAST(a AS DOUBLE) FROM t",
            Dialect::MariaDb
        ),
        [(types::bigint(), false), (types::double(), true)]
    );
}

#[test]
fn types_typed_literals_as_the_servers() {
    let text = concat!(
        "SELECT DATE '2024-05-06', TIME '07:08:09.5', TIMESTAMP '2024-05-06 07:08:09.1234567', ",
        "TIMESTAMP '2024-05-06 07:08:09.', {t '070809'}, DATE '2024-05-00', {d 'x'}"
    );
    let expected = |zero_nullable| {
        vec![
            (DataType::Date, false),
            (DataType::Time { fraction: 1 }, false),
            (DataType::DateTime { fraction: 6 }, false),
            (DataType::DateTime { fraction: 0 }, false),
            (DataType::Time { fraction: 0 }, false),
            (DataType::Date, zero_nullable),
            (types::string_literal("x"), false),
        ]
    };
    assert_eq!(column_types(text, Dialect::MySql), expected(false));
    assert_eq!(column_types(text, Dialect::MariaDb), expected(true));
}

#[test]
fn types_intervals_as_the_servers() {
    let text = concat!(
        "SELECT DATE '2024-05-06' + INTERVAL 1 DAY, DATE '2024-05-06' + INTERVAL 1 HOUR, ",
        "DATE '2024-05-06' + INTERVAL 1.25 SECOND, ",
        "TIMESTAMP '2024-05-06 07:08:09.5' + INTERVAL 1 MONTH, ",
        "TIMESTAMP '2024-05-06 07:08:09' - INTERVAL '1.5' SECOND, ",
        "DATE_ADD(DATE '2024-05-06', INTERVAL '1.5' SECOND_MICROSECOND), ",
        "TIMESTAMPADD(SECOND, 1, TIME '07:08:09.5'), TIME '07:08:09' + INTERVAL 1 DAY, ",
        "TIME '07:08:09' + INTERVAL 1 WEEK, '2024-05-06' + INTERVAL 1 DAY, ",
        "ADDDATE('2024-05-06 07:08:09.5', 1)"
    );
    let text_of = |length| {
        DataType::Text(TextType {
            length: Some(length),
            fixed: false,
            charset: None,
            collation: None,
        })
    };
    let expected = |time_week, string, fractional_string| {
        [
            DataType::Date,
            DataType::DateTime { fraction: 0 },
            DataType::DateTime { fraction: 2 },
            DataType::DateTime { fraction: 1 },
            DataType::DateTime { fraction: 6 },
            DataType::DateTime { fraction: 6 },
            DataType::Time { fraction: 1 },
            DataType::Time { fraction: 0 },
            time_week,
            text_of(string),
            text_of(fractional_string),
        ]
        .into_iter()
        .map(|data_type| (data_type, true))
        .collect::<Vec<_>>()
    };
    assert_eq!(
        column_types(text, Dialect::MySql),
        expected(DataType::Time { fraction: 0 }, 29, 29)
    );
    assert_eq!(
        column_types(text, Dialect::MariaDb),
        expected(DataType::DateTime { fraction: 0 }, 19, 21)
    );
    // A `DATETIME`, the time read on the current date, from MySQL 8.0.28.
    let plan = plan_at(
        "SELECT TIME '07:08:09' + INTERVAL 1 DAY, TIME '07:08:09' + INTERVAL 1 HOUR",
        Dialect::MySql,
        Version::new(8, 0, 28),
    )
    .unwrap();
    let types: Vec<DataType> = plan
        .schema()
        .columns
        .into_iter()
        .map(|column| column.ty.data_type)
        .collect();
    assert_eq!(
        types,
        [
            DataType::DateTime { fraction: 0 },
            DataType::Time { fraction: 0 }
        ]
    );
}

#[test]
fn types_timestampdiff_as_a_nullable_bigint() {
    let bigint = DataType::Integer(IntegerType::signed(IntegerSize::Big));
    for dialect in [Dialect::MySql, Dialect::MariaDb] {
        assert_eq!(
            column_types(
                "SELECT TIMESTAMPDIFF(DAY, DATE '2024-05-06', DATE '2024-06-05')",
                dialect
            ),
            [(bigint.clone(), true)]
        );
    }
}

#[test]
fn types_the_current_date_and_time_after_the_version() {
    let text = concat!(
        "SELECT NOW(), CURRENT_TIMESTAMP(3), SYSDATE(6), LOCALTIME, LOCALTIMESTAMP(2), ",
        "UTC_TIMESTAMP, CURDATE(), CURRENT_DATE, UTC_DATE(), CURTIME(1), CURRENT_TIME, UTC_TIME(4)"
    );
    let types = |dialect, version| -> Vec<(DataType, bool)> {
        plan_at(text, dialect, version)
            .unwrap()
            .schema()
            .columns
            .into_iter()
            .map(|column| (column.ty.data_type, column.ty.nullable))
            .collect()
    };
    let expected = |now: fn(u8) -> DataType, localtime: DataType| {
        [
            now(0),
            now(3),
            now(6),
            localtime,
            DataType::DateTime { fraction: 2 },
            DataType::DateTime { fraction: 0 },
            DataType::Date,
            DataType::Date,
            DataType::Date,
            DataType::Time { fraction: 1 },
            DataType::Time { fraction: 0 },
            DataType::Time { fraction: 4 },
        ]
        .into_iter()
        .map(|data_type| (data_type, false))
        .collect::<Vec<_>>()
    };
    let datetime: fn(u8) -> DataType = |fraction| DataType::DateTime { fraction };
    let timestamp: fn(u8) -> DataType = |fraction| DataType::Timestamp { fraction };
    for (dialect, version) in [
        (Dialect::MySql, Version::new(8, 4, 0)),
        (Dialect::MariaDb, Version::new(11, 6, 1)),
    ] {
        assert_eq!(
            types(dialect, version),
            expected(datetime, DataType::DateTime { fraction: 0 })
        );
    }
    assert_eq!(
        types(Dialect::MariaDb, Version::new(11, 6, 2)),
        expected(datetime, DataType::Time { fraction: 0 })
    );
    assert_eq!(
        types(Dialect::MariaDb, Version::new(11, 7, 1)),
        expected(timestamp, DataType::Time { fraction: 0 })
    );
    // An argument the servers refuse leaves the type unknown.
    assert_eq!(
        column_types("SELECT NOW(7), CURDATE(1)", Dialect::MySql),
        [(DataType::Unknown, true), (DataType::Unknown, true)]
    );
}

#[test]
fn types_the_json_of_the_functions_of_mysql() {
    let text = concat!(
        "SELECT JSON_ARRAY(), JSON_OBJECT('k', 1), JSON_EXTRACT('{}', '$.a'), ",
        "JSON_SET('{}', '$.b', 1), JSON_LENGTH('[1]'), JSON_UNQUOTE('\"a\"')"
    );
    let json = (DataType::Json, true);
    let unknown = (DataType::Unknown, true);
    assert_eq!(
        column_types(text, Dialect::MySql),
        [
            json.clone(),
            json.clone(),
            json.clone(),
            json,
            unknown.clone(),
            unknown.clone()
        ]
    );
    // MariaDB gives a string, of a type unknown as its other functions.
    assert_eq!(column_types(text, Dialect::MariaDb), vec![unknown; 6]);
}

#[test]
fn types_regexp_sounds_like_and_member_of_as_predicates() {
    let text = concat!(
        "SELECT a REGEXP 'x', b NOT REGEXP 'x', a SOUNDS LIKE 'x', b SOUNDS LIKE 'x', ",
        "a MEMBER OF ('[1]'), a MEMBER OF (NULL) FROM t WHERE b REGEXP a"
    );
    let integer = |nullable| (types::bigint(), nullable);
    assert_eq!(
        column_types(text, Dialect::MySql),
        [false, true, false, true, false, true].map(integer)
    );
    let Plan::Project(project) = plan(text) else {
        panic!("projection expected");
    };
    let Plan::Filter(filter) = project.input.as_ref() else {
        panic!("filter expected");
    };
    assert!(matches!(filter.predicate.kind, ExprKind::Regexp { .. }));
    assert_eq!(filter.predicate.ty, Type::nullable(DataType::Boolean));
}

#[test]
fn types_extract_as_a_nullable_integer() {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let bigint = DataType::Integer(IntegerType::signed(IntegerSize::Big));
    let text = concat!(
        "SELECT EXTRACT(DAY FROM DATE '2024-05-06'), EXTRACT(DAY_SECOND FROM NULL), ",
        "EXTRACT(MINUTE_MICROSECOND FROM TIME '10:20:30'), EXTRACT(DAY_MICROSECOND FROM '')"
    );
    for dialect in [Dialect::MySql, Dialect::MariaDb] {
        assert_eq!(
            column_types(text, dialect),
            [
                (int.clone(), true),
                (int.clone(), true),
                (bigint.clone(), true),
                (bigint.clone(), true)
            ]
        );
    }
}

#[test]
fn a_recursive_expression_takes_the_types_of_its_anchors() {
    let text = "WITH RECURSIVE r (n, s, k) AS (SELECT 1, 'a', k FROM a UNION ALL \
                SELECT n + 0.5, CONCAT(s, 'b'), k FROM r WHERE n < 3) SELECT * FROM r";
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let anchors = |first| {
        vec![
            (first, true),
            (types::string_literal("a"), true),
            (int.clone(), true),
        ]
    };
    assert_eq!(column_types(text, Dialect::MySql), anchors(types::bigint()));
    assert_eq!(column_types(text, Dialect::MariaDb), anchors(int.clone()));
    // The anchors after a recursive part, as MariaDB writes them.
    let text = "WITH RECURSIVE r (n) AS (SELECT n + 1 FROM r WHERE n < 3 UNION ALL SELECT 1) \
                SELECT n FROM r";
    assert_eq!(column_types(text, Dialect::MariaDb), [(int, true)]);
}

#[test]
fn a_recursive_expression_needs_an_anchor_and_a_union() {
    assert_eq!(
        error("WITH RECURSIVE r (n) AS (SELECT n FROM r) SELECT n FROM r"),
        "the recursive common table expression `r` has no part that does not read it"
    );
    assert_eq!(
        plan_in(
            "WITH RECURSIVE r (n) AS (SELECT 1 EXCEPT SELECT n FROM r) SELECT n FROM r",
            Dialect::MariaDb
        )
        .unwrap_err()
        .message,
        "a recursive common table expression of another set operation than `UNION`: not \
         supported yet by Prisme"
    );
    // `b` is a table for MySQL, the expression after it for MariaDB.
    assert_eq!(
        error("WITH RECURSIVE r (k) AS (SELECT k FROM b), b (k) AS (SELECT 1) SELECT k FROM r"),
        "`b`, the name of a common table expression after `r` in `WITH RECURSIVE`: not \
         supported yet by Prisme"
    );
    // A `WITH` without `RECURSIVE` reads the table.
    assert!(
        plan_in(
            "WITH r (k) AS (SELECT k FROM b), b (k) AS (SELECT 1) SELECT k FROM r",
            Dialect::MySql
        )
        .is_ok()
    );
    let limited = "WITH RECURSIVE r (n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM r WHERE n < 3 \
                   LIMIT 2) SELECT n FROM r";
    assert!(plan_at(limited, Dialect::MySql, Version::new(8, 0, 19)).is_ok());
    assert_eq!(
        plan_at(limited, Dialect::MySql, Version::new(8, 0, 18))
            .unwrap_err()
            .message,
        "`LIMIT` in a recursive common table expression: from MySQL 8.0.19, chosen version 8.0.18"
    );
}

#[test]
fn lists_the_tables_outside_a_recursive_expression() {
    let options = ParseOptions {
        dialect: Dialect::MySql,
        version: Version::new(8, 0, 19),
        mode: SqlMode::default(),
        conditional_version: Version::new(8, 0, 19),
    };
    let text = "WITH RECURSIVE r (k) AS (SELECT k FROM a UNION ALL SELECT r.k FROM r JOIN b \
                ON b.k = r.k) SELECT k FROM r";
    let parsed = parse(text, &options).unwrap();
    let names: Vec<_> = referenced_tables(&parsed.statement)
        .into_iter()
        .map(|table| table.name.value)
        .collect();
    assert_eq!(names, ["a", "b"]);
}

/// Predicate of the `HAVING` of `text`, written in MySQL.
fn having(text: &str) -> prisme_plan::Expr {
    let Plan::Project(project) = plan(text) else {
        panic!("{text}: a select list expected");
    };
    let Plan::Filter(filter) = *project.input else {
        panic!("{text}: `HAVING` expected");
    };
    filter.predicate
}

#[test]
fn reads_the_aliases_inside_the_expressions_of_the_keys() {
    // `b` names a column of `t`, which `HAVING` reads after the alias, a
    // grouping key aside.
    let left = |text: &str| {
        let ExprKind::Compare { left, .. } = having(text).kind else {
            panic!("{text}: a comparison expected");
        };
        left.kind
    };
    assert!(matches!(
        left("SELECT a % 2 AS b, COUNT(*) AS n FROM t GROUP BY a % 2 HAVING b = 1"),
        ExprKind::Alias(alias) if alias.name.value == "b"
    ));
    assert!(!matches!(
        left("SELECT a % 2 AS b, COUNT(*) AS n FROM t GROUP BY a % 2, b HAVING b = '1'"),
        ExprKind::Alias(_)
    ));
    for text in [
        "SELECT a AS x FROM t ORDER BY x + 0",
        "SELECT a AS b FROM t ORDER BY b + 0",
        "SELECT a AS b, b AS b FROM t ORDER BY b + 0",
        "SELECT a AS x, COUNT(*) AS n FROM t GROUP BY x + 0",
        "SELECT a AS x FROM t GROUP BY a ORDER BY MAX(x)",
        "SELECT a % 2 AS x, COUNT(*) AS n FROM t GROUP BY x HAVING ABS(x) = 1 ORDER BY -n",
    ] {
        assert!(plan_in(text, Dialect::MySql).is_ok(), "{text}");
    }
    // Errors 1052 and 1111 of MySQL.
    for text in [
        "SELECT a AS x, b AS x FROM t ORDER BY x",
        "SELECT a AS x, b AS x FROM t ORDER BY x + 0",
        "SELECT a AS x, b AS x FROM t GROUP BY x",
    ] {
        assert_eq!(
            error(text),
            "`x` is ambiguous: several items of the select list have this alias",
            "{text}"
        );
    }
    assert_eq!(
        error("SELECT a % 2 AS x, SUM(a) AS s FROM t GROUP BY x HAVING SUM(s) > 3"),
        "an aggregate function of `s`, the alias of an aggregate or window function"
    );
}
