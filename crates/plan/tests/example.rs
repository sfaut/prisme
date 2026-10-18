//! The queries of step 4 of `docs/DEV-DIALECTS.md`, built by hand: the plan
//! must represent them before a parser builds them.
//!
//! `WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds`
//! is written in MySQL; MariaDB needs `VALUES (123), (234), (345)`. Both
//! queries give the same plan, apart from the form of the `VALUES`.

use prisme_plan::{
    AliasForm, ColumnRef, Comparison, Cte, CteRef, Direction, Expr, Ident, Limit, LimitForm,
    NamedExpr, Nulls, Plan, Project, ProjectItem, Sort, SortKey, Span, Statement, Values,
    ValuesForm, Wildcard, With,
};
use prisme_types::{DataType, IntegerSize, IntegerType, Type, Value};

/// Query of the example, in MySQL.
const MYSQL: &str = "WITH ds (c) AS (VALUES ROW (123), ROW (234), ROW (345)) SELECT * FROM ds";

/// Type the dialect gives to the integer literals of the example.
fn bigint() -> DataType {
    DataType::Integer(IntegerType::signed(IntegerSize::Big))
}

/// `VALUES` of the example, in the given form, at its position in `MYSQL`.
fn values(form: ValuesForm) -> Values {
    let rows = [123, 234, 345]
        .into_iter()
        .map(|value| vec![Expr::literal(Value::Integer(value), bigint())])
        .collect();
    let mut values = Values::new(rows, vec![Ident::new("column_0")], vec![bigint()], form).unwrap();
    let start = MYSQL.find("VALUES").unwrap();
    values.span = Some(Span::new(start, MYSQL.find(") SELECT").unwrap()));
    values
}

/// `WITH ds (c) AS (<values>)` around the query built by `body` on `ds`.
fn with_ds(form: ValuesForm, body: impl FnOnce(Plan) -> Plan) -> Plan {
    let cte = Cte::new(
        Ident::new("ds"),
        vec![Ident::new("c")],
        Plan::Values(values(form)),
    )
    .unwrap();
    let ds = Plan::CteRef(CteRef::new(&cte, None).unwrap());
    Plan::With(With {
        recursive: false,
        ctes: vec![cte],
        body: Box::new(body(ds)),
        span: None,
    })
}

/// `SELECT * FROM <input>`.
fn select_star(input: Plan) -> Plan {
    let wildcard = ProjectItem::Wildcard(Wildcard {
        qualifier: None,
        columns: vec![0],
        names: Vec::new(),
    });
    Plan::Project(Project::new(input, vec![wildcard]).unwrap())
}

/// Column `ds.c`, as read from the common table expression.
fn column_c() -> Expr {
    let column = ColumnRef {
        qualifier: None,
        name: Ident::new("c"),
    };
    Expr::column(column, Type::not_null(bigint()))
}

#[test]
fn example_has_one_column_c_from_ds() {
    let plan = with_ds(ValuesForm::Row, select_star);
    let schema = plan.schema();
    assert_eq!(schema.len(), 1);
    let column = &schema.columns[0];
    assert_eq!(column.qualifier, Some(Ident::new("ds")));
    assert_eq!(column.name, Ident::new("c"));
    assert_eq!(column.ty, Type::not_null(bigint()));
}

#[test]
fn mysql_and_mariadb_forms_differ_only_by_the_form_of_values() {
    let mysql = with_ds(ValuesForm::Row, select_star);
    let mariadb = with_ds(ValuesForm::Bare, select_star);
    assert_ne!(mysql, mariadb);
    let Plan::With(mut with) = mariadb else {
        unreachable!("the example starts with WITH");
    };
    let Plan::Values(values) = &mut with.ctes[0].plan else {
        unreachable!("the common table expression is a VALUES");
    };
    values.form = ValuesForm::Row;
    assert_eq!(Plan::With(with), mysql);
}

#[test]
fn values_keeps_its_position_for_the_report() {
    let plan = with_ds(ValuesForm::Row, select_star);
    let Plan::With(with) = plan else {
        unreachable!("the example starts with WITH");
    };
    let span = with.ctes[0].plan.span().unwrap();
    assert_eq!(
        &MYSQL[span.start..span.end],
        "VALUES ROW (123), ROW (234), ROW (345)"
    );
}

#[test]
fn filter_sort_and_limit_of_the_minimal_grammar() {
    // WITH ds (c) AS (...) SELECT c FROM ds WHERE c > 200 ORDER BY c DESC LIMIT 2
    let query = with_ds(ValuesForm::Row, |ds| {
        let predicate = Expr::compare(
            Comparison::Gt,
            column_c(),
            Expr::literal(Value::Integer(200), bigint()),
        );
        let filter = Plan::Filter(prisme_plan::Filter {
            input: Box::new(ds),
            predicate,
            form: prisme_plan::FilterForm::Where,
            span: None,
        });
        let item = ProjectItem::Expr(NamedExpr {
            expr: column_c(),
            name: Ident::new("c"),
            alias: AliasForm::Implicit,
        });
        let project = Plan::Project(Project::new(filter, vec![item]).unwrap());
        let mut key = SortKey::implicit(column_c(), Direction::Descending);
        key.form.direction = true;
        let sort = Plan::Sort(Sort {
            input: Box::new(project),
            keys: vec![key],
            span: None,
        });
        Plan::Limit(Limit {
            input: Box::new(sort),
            count: Some(Expr::literal(Value::Integer(2), bigint())),
            offset: None,
            form: LimitForm::Offset,
            span: None,
        })
    });
    let statement = Statement::Query(query);
    let Statement::Query(plan) = &statement else {
        unreachable!("built as a query");
    };
    assert_eq!(plan.schema().columns[0].ty, Type::not_null(bigint()));
    // `DESC` without `NULLS`: NULL as the smallest value puts the NULLs last.
    let Plan::With(with) = plan else {
        unreachable!("the query starts with WITH");
    };
    let Plan::Limit(limit) = with.body.as_ref() else {
        unreachable!("the body ends with LIMIT");
    };
    let Plan::Sort(sort) = limit.input.as_ref() else {
        unreachable!("LIMIT applies to the sort");
    };
    assert_eq!(sort.keys[0].nulls, Nulls::Last);
    assert!(!sort.keys[0].form.nulls);
}
