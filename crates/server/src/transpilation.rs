//! Transpilation of the queries written in a dialect other than the native
//! SQL of the data source (`docs/DEV-DIALECTS.md`, step 4).
//!
//! The user chooses the dialect in the profile ("Dialecte préféré"), which an
//! editor tab can override: `source`, the native SQL of the server, whose
//! queries run as written, or a dialect with its version, such as
//! `mysql 8.0.19`. A query written in a chosen dialect is read, built into a
//! plan from the columns of its tables, read on the connection that runs it
//! (its temporary tables included), then written in the dialect of the server.
//! The report goes to the interface with the result; its positions are in
//! UTF-16 units from the start of the query, like the editor's.
//!
//! Routes:
//!
//! | Method and path         | Role                                                      |
//! |-------------------------|-----------------------------------------------------------|
//! | `GET /api/dialects`     | Dialects offered, with their versions.                    |
//! | `POST /api/transpile`   | Transpiles the queries of a text without running them.    |

use std::fmt;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use prisme_connectors::mysql::{self, Connection, MysqlError, ServerInfo, catalog};
use prisme_dialects::Dialect;
use prisme_dialects::build::Tables;
use prisme_dialects::emit::OutputDialect;
use prisme_dialects::lexer::SqlMode;
use prisme_dialects::report::{StepKind, Transpilation};
use prisme_dialects::transpile::{InputDialect, TranspileError, prepare};
use prisme_dialects::versions::{Version, registry};
use prisme_plan::{Ident, Span};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::analyze::{dialect_of, flavor_of};
use crate::api::AppState;
use crate::api_error::ApiError;
use crate::auth::CurrentUser;
use crate::connections;
use crate::query_session::chosen_statements;

/// Stored value of the native SQL of the data source.
const SOURCE: &str = "source";

/// Dialects offered, by identifier.
const DIALECTS: [(&str, Dialect); 2] = [("mysql", Dialect::MySql), ("mariadb", Dialect::MariaDb)];

/// Dialect the user writes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialectChoice {
    /// Native SQL of the data source: the queries run as written.
    Source,
    /// Dialect with its version: the queries are transpiled.
    Chosen(InputDialect),
}

impl DialectChoice {
    /// Reads a choice: `source`, or the identifier of a dialect and a version
    /// `X.Y.Z` from the first version of the registry, such as `mysql 8.0.19`.
    ///
    /// # Errors
    ///
    /// The reason, for an unknown dialect or an invalid version.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text == SOURCE {
            return Ok(Self::Source);
        }
        let (id, version) = text.split_once(' ').unwrap_or((text, ""));
        let dialect = DIALECTS
            .iter()
            .find(|(known, _)| *known == id)
            .map(|&(_, dialect)| dialect)
            .ok_or_else(|| {
                format!(
                    "unknown dialect `{text}`: `source`, or a dialect with its version, such as `mysql 8.0.19`"
                )
            })?;
        let parsed = Version::parse(version)
            .filter(|parsed| parsed.to_string() == version)
            .ok_or_else(|| {
                format!(
                    "{}: version `{version}` invalid, X.Y.Z expected",
                    dialect.label()
                )
            })?;
        let floor = registry(dialect).floor();
        if parsed < floor {
            return Err(format!(
                "{} {parsed}: versions from {floor} only",
                dialect.label()
            ));
        }
        Ok(Self::Chosen(InputDialect {
            dialect,
            version: parsed,
        }))
    }
}

impl fmt::Display for DialectChoice {
    /// Stored form, read again by `parse`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source => f.write_str(SOURCE),
            Self::Chosen(input) => write!(f, "{} {}", id(input.dialect), input.version),
        }
    }
}

/// Identifier of a dialect, in the stored choices.
fn id(dialect: Dialect) -> &'static str {
    DIALECTS
        .iter()
        .find(|(_, known)| *known == dialect)
        .map_or("", |&(id, _)| id)
}

/// Dialect offered to the user, with its versions.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct DialectOption {
    /// Identifier, first word of a stored choice (`mysql`).
    pub id: String,
    /// Name shown (`MySQL`).
    pub label: String,
    /// Versions offered, those where the dialect changed, from the oldest.
    pub versions: Vec<String>,
}

/// Dialects offered to the user, with their versions.
pub fn options() -> Vec<DialectOption> {
    DIALECTS
        .iter()
        .map(|&(id, dialect)| DialectOption {
            id: id.to_string(),
            label: dialect.label().to_string(),
            versions: registry(dialect)
                .offered_versions()
                .iter()
                .map(Version::to_string)
                .collect(),
        })
        .collect()
}

/// Dialect of a server: its version, and its SQL mode, which reads the
/// queries and writes their transpilation.
pub fn output_of(server: &ServerInfo) -> OutputDialect {
    let dialect = dialect_of(server.flavor);
    OutputDialect {
        dialect,
        version: Version::parse(&server.version).unwrap_or_else(|| registry(dialect).floor()),
        mode: SqlMode::parse(&server.sql_mode),
    }
}

/// Input and output dialects of the queries to transpile.
#[derive(Debug, Clone, Copy)]
pub struct Translation {
    /// Dialect the queries are written in.
    pub input: InputDialect,
    /// Dialect of the server that runs them.
    pub output: OutputDialect,
}

/// Report of a transpilation, as the interface receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TranspilationReport {
    /// Dialect the query is written in (`mysql 8.0.19`).
    pub input: String,
    /// Dialect of the server, with its version (`mariadb 11.3.2`).
    pub output: String,
    /// Query in the dialect of the server; empty when it cannot be
    /// transpiled.
    pub sql: String,
    /// Differences between the query and its transpilation.
    pub steps: Vec<ReportStep>,
    /// Why the query cannot be transpiled; empty when it can.
    pub errors: Vec<ReportError>,
}

/// Difference between the query and its transpilation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReportStep {
    /// Identifier of the rule (`values.form`).
    pub rule: String,
    /// Kind of difference.
    pub kind: ReportStepKind,
    /// What changed and why.
    pub message: String,
    /// Passage of the query concerned; absent for the whole query.
    pub span: Option<TextSpan>,
}

/// Kind of a difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ReportStepKind {
    /// Another form of the same meaning.
    Rewrite,
    /// A construct written with other means of the server's dialect.
    Emulation,
    /// The server gives less than the request (a type it gives no expression
    /// of), or than the same request written in its dialect.
    LossOfPrecision,
    /// A construct without effect on the result, left out.
    Omission,
}

/// Reason why the query cannot be transpiled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ReportError {
    /// Identifier of the rule of translation; absent for a query the input
    /// dialect does not read (syntax, construct not supported yet, unknown
    /// name).
    pub rule: Option<String>,
    /// Why.
    pub message: String,
    /// Passage of the query concerned; absent for the whole query.
    pub span: Option<TextSpan>,
}

/// Passage of a query, in UTF-16 units from its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TextSpan {
    /// First unit.
    pub start: usize,
    /// Unit after the last one.
    pub end: usize,
}

impl TranspilationReport {
    /// Empty report of a query transpiled from `translation.input` to
    /// `translation.output`.
    fn new(translation: Translation) -> Self {
        let output = translation.output;
        Self {
            input: DialectChoice::Chosen(translation.input).to_string(),
            output: format!("{} {}", id(output.dialect), output.version),
            sql: String::new(),
            steps: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Records the transpilation of `text`.
    fn fill(&mut self, text: &str, transpilation: Transpilation) {
        self.sql = transpilation.sql;
        self.steps = transpilation
            .steps
            .into_iter()
            .map(|step| ReportStep {
                rule: step.rule.to_string(),
                kind: match step.kind {
                    StepKind::Rewrite => ReportStepKind::Rewrite,
                    StepKind::Emulation => ReportStepKind::Emulation,
                    StepKind::LossOfPrecision => ReportStepKind::LossOfPrecision,
                    StepKind::Omission => ReportStepKind::Omission,
                },
                message: step.message,
                span: step.span.map(|span| utf16(text, span)),
            })
            .collect();
        self.errors = transpilation
            .errors
            .into_iter()
            .map(|error| ReportError {
                rule: Some(error.rule.to_string()),
                message: error.message,
                span: error.span.map(|span| utf16(text, span)),
            })
            .collect();
    }

    /// Records the failure of the reading of `text`, or of its plan.
    fn fail(&mut self, text: &str, error: TranspileError) {
        self.errors = vec![ReportError {
            rule: None,
            span: Some(utf16(text, error.span)),
            message: error.message,
        }];
    }

    /// Message of the failure of the query, when it cannot be transpiled.
    pub fn failure(&self) -> Option<String> {
        let first = self.errors.first()?;
        let mut message = format!(
            "query not transpiled from {} to {}: {}",
            self.input, self.output, first.message
        );
        let others = self.errors.len() - 1;
        if others > 0 {
            message.push_str(&format!(" (and {others} more, see the report)"));
        }
        Some(message)
    }
}

/// Passage `span` of `text` (bytes), in UTF-16 units.
fn utf16(text: &str, span: Span) -> TextSpan {
    let units = |part: Option<&str>| part.map_or(0, |part| part.encode_utf16().count());
    let start = units(text.get(..span.start));
    TextSpan {
        start,
        end: start + units(text.get(span.start..span.end)),
    }
}

/// Transpiles `text`, reading the columns of its tables on `connection`, on
/// its current database, and the types of its user variables.
///
/// The reading and the writing of the query run in blocking tasks, off the
/// threads of `tokio`, like the analysis of the editor's text.
///
/// # Errors
///
/// `MysqlError` when the columns of a table cannot be read; a query that
/// cannot be transpiled gives a report with its errors.
pub async fn transpile(
    connection: &mut Connection,
    text: &str,
    translation: Translation,
) -> Result<TranspilationReport, MysqlError> {
    let mut report = TranspilationReport::new(translation);
    let owned = text.to_string();
    let prepared = blocking(move || prepare(&owned, translation.input, translation.output)).await;
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            report.fail(text, error);
            return Ok(report);
        }
    };
    let flavor = flavor_of(translation.output.dialect);
    let mut tables = Tables::default();
    for table in prepared.tables() {
        let database = table
            .database
            .as_ref()
            .map(|database| database.value.as_str());
        if let Some(columns) =
            catalog::table_columns(connection, flavor, database, &table.name.value).await?
        {
            let columns = columns
                .into_iter()
                .map(|(name, ty)| (Ident::new(name), ty))
                .collect();
            tables.insert(database, &table.name.value, columns);
        }
    }
    let names: Vec<String> = prepared
        .user_variables()
        .into_iter()
        .map(|name| name.value)
        .collect();
    for (name, ty) in catalog::user_variable_types(connection, &names).await? {
        tables.insert_user_variable(&name, ty);
    }
    match blocking(move || prepared.transpile(&tables)).await {
        Ok(transpilation) => report.fill(text, transpilation),
        Err(error) => report.fail(text, error),
    }
    Ok(report)
}

/// Queries to transpile without running them, for the preview.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TranspileRequest {
    /// Saved connection whose server would run the queries.
    pub connection_id: i64,
    /// Current database of the session; absent for that of the connection.
    #[serde(default)]
    #[ts(optional)]
    pub database: Option<String>,
    /// Whole text of the editor.
    pub sql: String,
    /// Position of the cursor in UTF-16 units: only the query under the
    /// cursor. Absent: all the queries.
    #[serde(default)]
    #[ts(optional)]
    pub cursor: Option<usize>,
    /// Dialect the text is written in (`mysql 8.0.19`).
    pub dialect: String,
}

/// Transpilation of a query of the text.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct TranspiledQuery {
    /// Start of the query in the text, in UTF-16 units.
    pub start: usize,
    /// End of the query in the text, in UTF-16 units.
    pub end: usize,
    /// Report of its transpilation.
    pub report: TranspilationReport,
}

/// Routes of the dialects and of the preview of the transpilation.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/dialects", get(list_dialects))
        .route("/api/transpile", post(preview))
}

/// `GET /api/dialects`: dialects offered, with their versions.
async fn list_dialects() -> Json<Vec<DialectOption>> {
    Json(options())
}

/// `POST /api/transpile`: transpiles the queries of a text without running
/// them. The columns of their tables are read on a new connection to the
/// server, placed on the database of the session: the temporary tables and
/// the user variables of the session are not seen there.
async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<TranspileRequest>,
) -> Result<Json<Vec<TranspiledQuery>>, ApiError> {
    let input = match DialectChoice::parse(&request.dialect).map_err(ApiError::BadRequest)? {
        DialectChoice::Chosen(input) => input,
        DialectChoice::Source => {
            return Err(ApiError::BadRequest(
                "the native SQL of the data source runs as written: nothing to transpile"
                    .to_string(),
            ));
        }
    };
    let (user_id, id) = (user.id, request.connection_id);
    let target = state
        .with_db(move |db, key| connections::load_target(db, key, user_id, id))
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut connection = mysql::connect(&connections::with_read_only(target, true)).await?;
    let queries = transpile_all(&mut connection, input, &request).await;
    let _ = connection.disconnect().await;
    Ok(Json(queries?))
}

/// Transpiles the queries of `request` chosen by its cursor, on `connection`.
async fn transpile_all(
    connection: &mut Connection,
    input: InputDialect,
    request: &TranspileRequest,
) -> Result<Vec<TranspiledQuery>, MysqlError> {
    let server = mysql::server_info(connection).await?;
    if let Some(database) = &request.database {
        mysql::use_database(connection, database).await?;
    }
    let translation = Translation {
        input,
        output: output_of(&server),
    };
    let statements = prisme_splitter::split(&request.sql);
    let mut queries = Vec::new();
    for statement in chosen_statements(&statements, request.cursor) {
        queries.push(TranspiledQuery {
            start: statement.start,
            end: statement.end,
            report: transpile(connection, &statement.sql, translation).await?,
        });
    }
    Ok(queries)
}

/// Runs `task` in a blocking task; a panic of the task goes on in the caller.
async fn blocking<T: Send + 'static>(task: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(task)
        .await
        .unwrap_or_else(|error| std::panic::resume_unwind(error.into_panic()))
}

#[cfg(test)]
mod tests {
    use prisme_connectors::mysql::Flavor;

    use super::*;

    #[test]
    fn reads_and_writes_the_choices() {
        for text in ["source", "mysql 8.0.19", "mariadb 10.6.3"] {
            assert_eq!(DialectChoice::parse(text).unwrap().to_string(), text);
        }
        assert_eq!(
            DialectChoice::parse("mysql 8.0.19"),
            Ok(DialectChoice::Chosen(InputDialect {
                dialect: Dialect::MySql,
                version: Version::new(8, 0, 19)
            }))
        );
        for text in [
            "",
            "mysql",
            "mysql 8",
            "mysql 8.0.19 ",
            "oracle 19.0.0",
            "SOURCE",
        ] {
            assert!(DialectChoice::parse(text).is_err(), "{text:?}");
        }
        assert_eq!(
            DialectChoice::parse("mysql 5.7.44"),
            Err("MySQL 5.7.44: versions from 8.0.11 only".to_string())
        );
    }

    #[test]
    fn offers_the_versions_of_the_registry() {
        let options = options();
        assert_eq!(options[0].id, "mysql");
        assert_eq!(options[0].versions[0], "8.0.11");
        assert!(options[0].versions.contains(&"8.0.19".to_string()));
        assert_eq!(options[1].label, "MariaDB");
        for option in &options {
            for version in &option.versions {
                let choice = format!("{} {version}", option.id);
                assert!(DialectChoice::parse(&choice).is_ok(), "{choice}");
            }
        }
    }

    #[test]
    fn the_output_follows_the_server() {
        let server = ServerInfo {
            flavor: Flavor::MariaDb,
            version: "11.3.2-MariaDB-1:11.3.2+maria~ubu2204".to_string(),
            ansi_quotes: true,
            sql_mode: "ANSI_QUOTES,STRICT_TRANS_TABLES".to_string(),
        };
        let output = output_of(&server);
        assert_eq!(output.dialect, Dialect::MariaDb);
        assert_eq!(output.version, Version::new(11, 3, 2));
        assert!(output.mode.ansi_quotes);
    }

    #[test]
    fn positions_are_in_utf16_units() {
        // "é" is 2 bytes and 1 UTF-16 unit, "𝄞" 4 bytes and 2 units: `x` is
        // at byte 17, unit 14.
        let text = "SELECT 'é𝄞', x";
        let start = text.find('x').unwrap();
        assert_eq!(
            utf16(text, Span::new(start, start + 1)),
            TextSpan { start: 14, end: 15 }
        );
    }

    #[test]
    fn a_failure_names_both_dialects() {
        let mut report = TranspilationReport::new(Translation {
            input: InputDialect {
                dialect: Dialect::MySql,
                version: Version::new(8, 0, 19),
            },
            output: OutputDialect {
                dialect: Dialect::MariaDb,
                version: Version::new(11, 3, 2),
                mode: SqlMode::default(),
            },
        });
        assert_eq!(report.failure(), None);
        report.fail(
            "SELECT z",
            TranspileError {
                message: "unknown column `z`".to_string(),
                span: Span::new(7, 8),
            },
        );
        assert_eq!(
            report.failure().unwrap(),
            "query not transpiled from mysql 8.0.19 to mariadb 11.3.2: unknown column `z`"
        );
        assert_eq!(report.errors[0].span, Some(TextSpan { start: 7, end: 8 }));
    }

    #[tokio::test]
    #[ignore = "requires Docker Desktop running"]
    async fn previews_the_queries_of_a_text_on_the_server_of_a_connection() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use secrecy::SecretString;
        use serde_json::{Value, json};
        use testcontainers_modules::mariadb::Mariadb;
        use testcontainers_modules::testcontainers::runners::AsyncRunner;
        use tower::ServiceExt;

        use crate::connections::NewConnection;
        use crate::master_key::MasterKey;
        use crate::store;

        let server = Mariadb::default().start().await.unwrap();
        let host = server.get_host().await.unwrap().to_string();
        let port = server.get_host_port_ipv4(3306).await.unwrap();
        let db = store::open_in_memory();
        let key = MasterKey::generate();
        let user_id = store::ensure_user(&db, "test@example.com").unwrap();
        let new = NewConnection {
            name: "test".to_string(),
            host,
            port,
            username: "root".to_string(),
            password: SecretString::from(""),
            database_name: "test".to_string(),
            init_sql: String::new(),
            session_timeout_minutes: 5,
            show_system_schemas: true,
        };
        let id = connections::insert(&db, &key, user_id, &new).unwrap();
        let app = crate::api::router(AppState::new(db, key, user_id));

        let text = "SELECT 1 AS one;\nVALUES ROW(2);\nSELECT * FROM missing";
        let body = json!({ "connection_id": id, "sql": text, "dialect": "mysql 8.0.19" });
        let request = Request::builder()
            .method("POST")
            .uri("/api/transpile")
            .header("content-type", "application/json")
            .header("host", "localhost:8484")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let queries: Value = serde_json::from_slice(&bytes).unwrap();
        let reports: Vec<&Value> = queries
            .as_array()
            .unwrap()
            .iter()
            .map(|query| &query["report"])
            .collect();
        assert_eq!(reports.len(), 3);
        assert_eq!(reports[0]["sql"], "SELECT 1 AS one");
        assert!(reports[0]["steps"].as_array().unwrap().is_empty());
        // MariaDB names the column of a table value constructor after its first row.
        assert_eq!(reports[1]["sql"], "SELECT 2 AS `column_0`");
        assert_eq!(reports[1]["steps"][0]["rule"], "values.union");
        assert_eq!(
            reports[2]["errors"][0]["message"],
            "unknown table `missing`"
        );
        assert_eq!(queries[1]["start"], 17);
    }
}
