use prisme_plan::{ExprKind, SortReference};
use prisme_types::{DataType, IntegerSize, IntegerType};

use super::*;
use crate::lexer::SqlMode;
use crate::sql::{ParseOptions, parse};
use crate::versions::Version;

/// `t (a INT NOT NULL, b VARCHAR(10) NULL)` and `u (a INT NOT NULL)`.
fn catalog() -> Tables {
    let int = DataType::Integer(IntegerType::signed(IntegerSize::Int));
    let mut tables = Tables::default();
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
    let options = ParseOptions {
        dialect,
        version,
        mode: SqlMode::default(),
        conditional_version: version,
    };
    let parsed = parse(text, &options).unwrap_or_else(|error| panic!("{text}: {error}"));
    let prisme_plan::Statement::Query(plan) = build(&parsed.statement, text, dialect, &catalog())?
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
