//! Measurement of SQL expressions on each server: the type and nullability of
//! the column they give, their value, their first warning or their error.
//! The servers that differ are marked: this is how the behaviours of the
//! built-in functions are compared before writing a translation rule.
//!
//! Each expression is evaluated twice in a stored procedure that catches the
//! errors: `SELECT <expression> INTO @value FROM sample` gives the warning
//! or the error as a query gives them, and `CREATE TABLE ... AS SELECT`,
//! with the warnings kept as warnings, gives the type. The value is the one
//! a query of the client reads, in a session of its own where the statements
//! of the file run again: the column of the `CREATE TABLE` bounds a value to
//! its type (MySQL 8.0.11 types `CAST(-1 AS UNSIGNED)` as an `INT UNSIGNED`,
//! 4294967295) and writes a decimal at its scale, a user variable converts
//! some values. A byte string keeps the value of the column, in hexadecimal.
//! `sample` holds one row:
//! `d DATE`, `dt DATETIME(3)`, `ts TIMESTAMP(6)`, `t TIME`, `s VARCHAR(30)`,
//! `i INT`, `n DECIMAL(10,2)`, `f DOUBLE`, all `NOT NULL`, then `dn DATE`,
//! `sn VARCHAR(30)` and `j JSON`, nullable, `dn` and `sn` being NULL.

use std::collections::HashMap;
use std::fmt::Write;

use anyhow::{Context, bail};
use indoc::indoc;

use crate::server::Server;

/// Table of the sample row, procedure and table of the results.
const PREPARATION: &str = indoc! {r#"
    DROP TABLE IF EXISTS sample;
    CREATE TABLE sample (
      d DATE NOT NULL, dt DATETIME(3) NOT NULL, ts TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
      t TIME NOT NULL, s VARCHAR(30) NOT NULL, i INT NOT NULL, n DECIMAL(10,2) NOT NULL,
      f DOUBLE NOT NULL, dn DATE NULL, sn VARCHAR(30) NULL, j JSON NULL
    );
    INSERT INTO sample VALUES ('2024-05-06', '2024-05-06 07:08:09.123', '2024-05-06 07:08:09.123456',
      '10:20:30', 'Hello World', 42, 12.50, 2.5, NULL, NULL, '{"a": [1, 2]}');
    DROP TABLE IF EXISTS probe_results;
    CREATE TABLE probe_results (
      seq INT AUTO_INCREMENT PRIMARY KEY, case_id VARCHAR(100) NOT NULL, column_type TEXT NULL,
      nullable VARCHAR(3) NULL, value LONGTEXT NULL, warning TEXT NULL, failure TEXT NULL,
      type_failure TEXT NULL
    );
"#};

/// Procedure of the measurement, created after the statements of the user:
/// it runs with their SQL mode.
const PROCEDURE: &str = indoc! {r#"
    DROP PROCEDURE IF EXISTS probe_expression;
    DELIMITER //
    CREATE PROCEDURE probe_expression(IN case_id VARCHAR(100), IN expression LONGTEXT)
    BEGIN
      DECLARE failure TEXT DEFAULT NULL;
      DECLARE failure_code INT DEFAULT 0;
      DECLARE warning TEXT DEFAULT NULL;
      DECLARE value_failure TEXT DEFAULT NULL;
      DECLARE value_warning TEXT DEFAULT NULL;
      DECLARE column_type TEXT DEFAULT NULL;
      DECLARE nullable VARCHAR(3) DEFAULT NULL;
      DECLARE value_text LONGTEXT DEFAULT NULL;
      DECLARE saved_mode TEXT DEFAULT @@SESSION.sql_mode;
      DECLARE CONTINUE HANDLER FOR SQLEXCEPTION
        BEGIN
          GET DIAGNOSTICS CONDITION 1 failure_code = MYSQL_ERRNO, failure = MESSAGE_TEXT;
          SET failure = CONCAT(failure_code, ' ', failure);
        END;
      DECLARE CONTINUE HANDLER FOR SQLWARNING
        BEGIN
          GET DIAGNOSTICS CONDITION 1 warning = MESSAGE_TEXT;
        END;
      -- Value and warning, as a query gives them.
      SET @probe_value = NULL;
      SET @probe_sql = CONCAT('SELECT ', expression, ' INTO @probe_value FROM sample');
      PREPARE probe_statement FROM @probe_sql;
      IF failure IS NULL THEN
        SET warning = NULL;
        EXECUTE probe_statement;
        SET value_warning = warning;
        DEALLOCATE PREPARE probe_statement;
      END IF;
      SET value_failure = failure;
      -- Type and nullability, the warnings kept as warnings.
      SET failure = NULL;
      SET @probe_mode = TRIM(BOTH ',' FROM REPLACE(REPLACE(CONCAT(',', saved_mode, ','),
        ',STRICT_TRANS_TABLES,', ','), ',STRICT_ALL_TABLES,', ','));
      SET SESSION sql_mode = @probe_mode;
      DROP TABLE IF EXISTS probe_type;
      SET @probe_sql = CONCAT('CREATE TABLE probe_type AS SELECT ', expression, ' AS v FROM sample');
      PREPARE probe_statement FROM @probe_sql;
      IF failure IS NULL THEN
        EXECUTE probe_statement;
        DEALLOCATE PREPARE probe_statement;
      END IF;
      SET SESSION sql_mode = saved_mode;
      -- Value, read in the column of its type; from the query otherwise.
      IF failure IS NULL THEN
        SELECT c.COLUMN_TYPE, c.IS_NULLABLE INTO column_type, nullable
          FROM information_schema.COLUMNS AS c
          WHERE c.TABLE_SCHEMA = DATABASE() AND c.TABLE_NAME = 'probe_type';
        IF column_type REGEXP '^(binary|varbinary|tinyblob|blob|mediumblob|longblob|bit|geometry|point|linestring|polygon|multi)' THEN
          SELECT CONCAT('=0x', HEX(v)) INTO value_text FROM probe_type;
        ELSE
          SELECT CONCAT('=', CAST(v AS CHAR)) INTO value_text FROM probe_type;
        END IF;
      ELSE
        SET value_text = CONCAT('=', CAST(@probe_value AS CHAR));
      END IF;
      INSERT INTO probe_results (case_id, column_type, nullable, value, warning, failure, type_failure)
        VALUES (case_id, column_type, nullable, IF(value_failure IS NULL, value_text, NULL),
          value_warning, value_failure, failure);
    END//
    DELIMITER ;
"#};

/// Expression to measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// Identifier, in the output.
    pub id: String,
    /// SQL expression.
    pub expression: String,
    /// Its value is the same at each run: compared, unlike that of `NOW()`.
    pub deterministic: bool,
}

/// Content of a file of cases.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cases {
    /// Statements run before the measurements (`SET time_zone = ...`).
    pub setup: Vec<String>,
    /// Expressions.
    pub cases: Vec<Case>,
}

impl Cases {
    /// Reads a file of cases: one case per line, `id expression`, an id
    /// starting with `~` for an expression whose value changes at each run
    /// (only its type is compared); a line starting with `!` is a statement
    /// run before the measurements; `#` starts a comment line.
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let mut cases = Self::default();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(statement) = line.strip_prefix('!') {
                cases.setup.push(statement.trim().to_string());
                continue;
            }
            let Some((id, expression)) = line.split_once(char::is_whitespace) else {
                bail!("line {}: an identifier, then the expression", number + 1);
            };
            cases
                .push(id, expression.trim())
                .with_context(|| format!("line {}", number + 1))?;
        }
        Ok(cases)
    }

    /// Adds the case `id`, `~` first when its value changes at each run.
    pub fn push(&mut self, id: &str, expression: &str) -> anyhow::Result<()> {
        let (id, deterministic) = match id.strip_prefix('~') {
            Some(id) => (id, false),
            None => (id, true),
        };
        let valid = !id.is_empty()
            && id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character));
        if !valid {
            bail!("identifier {id:?}: letters, digits, `_`, `.` and `-` only");
        }
        if self.cases.iter().any(|case| case.id == id) {
            bail!("identifier {id:?} given twice");
        }
        self.cases.push(Case {
            id: id.to_string(),
            expression: expression.to_string(),
            deterministic,
        });
        Ok(())
    }
}

/// Result of an expression on a server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Measure {
    /// Type of the column, `bigint(17)`.
    pub column_type: Option<String>,
    /// Nullability of the column.
    pub nullable: Option<bool>,
    /// Value, `None` for NULL.
    pub value: Option<String>,
    /// First warning of the query.
    pub warning: Option<String>,
    /// Error of the query: code, then message.
    pub failure: Option<String>,
    /// Error of the `CREATE TABLE` that gives the type.
    pub type_failure: Option<String>,
}

/// Measures `cases` on `server`: a result per identifier.
pub fn measure(server: &Server, cases: &Cases) -> anyhow::Result<HashMap<String, Measure>> {
    let mut script = PREPARATION.to_string();
    for statement in &cases.setup {
        let _ = writeln!(script, "{};", statement.trim_end_matches(';'));
    }
    script.push_str(PROCEDURE);
    for case in &cases.cases {
        let _ = writeln!(
            script,
            "CALL probe_expression('{}', {});",
            case.id,
            text_literal(&case.expression)
        );
    }
    script.push_str(
        "SELECT case_id, column_type, nullable, value, warning, failure, type_failure FROM probe_results ORDER BY seq;\n",
    );
    let output = server.run_checked(&script, &["--batch", "--skip-column-names"])?;
    let mut measures = HashMap::new();
    for line in output.lines() {
        let fields: Vec<Option<String>> = line.split('\t').map(field).collect();
        let [
            Some(id),
            column_type,
            nullable,
            value,
            warning,
            failure,
            type_failure,
        ] = fields.as_slice()
        else {
            continue;
        };
        let measure = Measure {
            column_type: column_type.clone(),
            nullable: nullable.as_deref().map(|nullable| nullable == "YES"),
            value: value
                .as_deref()
                .map(|value| value.strip_prefix('=').unwrap_or(value).to_string()),
            warning: warning.clone(),
            failure: failure.clone(),
            type_failure: type_failure.clone(),
        };
        measures.insert(id.clone(), measure);
    }
    for (id, value) in read_values(server, cases)? {
        if let Some(measure) = measures.get_mut(&id)
            && measure.failure.is_none()
            && !measure.column_type.as_deref().is_some_and(binary)
        {
            measure.value = value;
        }
    }
    Ok(measures)
}

/// Values of `cases` as a query of the client reads them, in a session where
/// the statements of the file run again; a query that fails gives none.
fn read_values(server: &Server, cases: &Cases) -> anyhow::Result<HashMap<String, Option<String>>> {
    let mut script = String::new();
    for statement in &cases.setup {
        let _ = writeln!(script, "{};", statement.trim_end_matches(';'));
    }
    for case in &cases.cases {
        let _ = writeln!(
            script,
            "SELECT '{}', {} FROM sample;",
            case.id, case.expression
        );
    }
    let output = server.run(&script, &["--batch", "--skip-column-names", "--force"])?;
    let values = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(id, value)| (id.to_string(), field(value)))
        .collect();
    Ok(values)
}

/// Tells whether a column of type `column_type` holds bytes, written in
/// hexadecimal.
fn binary(column_type: &str) -> bool {
    [
        "binary",
        "varbinary",
        "tinyblob",
        "blob",
        "mediumblob",
        "longblob",
        "bit",
        "geometry",
        "point",
        "linestring",
        "polygon",
        "multi",
    ]
    .iter()
    .any(|prefix| column_type.starts_with(prefix))
}

/// Literal of `text`, read the same way whatever the SQL mode.
fn text_literal(text: &str) -> String {
    let hex: String = text.bytes().map(|byte| format!("{byte:02X}")).collect();
    format!("CONVERT(X'{hex}' USING utf8mb4)")
}

/// Field of a line of `--batch`: `NULL`, or the text with its escapes read.
fn field(raw: &str) -> Option<String> {
    if raw == "NULL" {
        return None;
    }
    let mut text = String::with_capacity(raw.len());
    let mut characters = raw.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some('0') => text.push('\0'),
            Some(other) => text.push(other),
            None => text.push('\\'),
        }
    }
    Some(text)
}

/// Text of `measure`: type, nullability and value (when `deterministic`),
/// warning, or error.
pub fn cell(measure: Option<&Measure>, deterministic: bool) -> String {
    let Some(measure) = measure else {
        return "?".to_string();
    };
    if let Some(failure) = &measure.failure {
        return format!("ERR {failure}");
    }
    let mut text = match (&measure.column_type, measure.nullable) {
        (Some(column_type), Some(true)) => format!("{column_type} NULL"),
        (Some(column_type), _) => format!("{column_type} NOT NULL"),
        (None, _) => match &measure.type_failure {
            Some(failure) => format!("(type unknown: {failure})"),
            None => "(type unknown)".to_string(),
        },
    };
    if deterministic {
        let value = measure.value.as_deref().unwrap_or("NULL");
        let _ = write!(text, " = {value}");
    }
    if let Some(warning) = &measure.warning {
        let _ = write!(text, " [warning: {warning}]");
    }
    text
}

/// `cell` without the display widths of the integers (`int(11)`, an `int`
/// since MySQL 8.0.19): the cells compared.
pub fn normalized(cell: &str) -> String {
    let mut text = String::with_capacity(cell.len());
    let mut rest = cell;
    while let Some(start) = rest.find('(') {
        let (before, after) = rest.split_at(start);
        text.push_str(before);
        let integer = ["tinyint", "smallint", "mediumint", "int", "bigint"]
            .iter()
            .any(|name| before.ends_with(name));
        let width = after[1..]
            .find(')')
            .filter(|&end| end > 0 && after[1..=end].chars().all(|c| c.is_ascii_digit()));
        match width {
            Some(end) if integer => rest = &after[end + 2..],
            _ => {
                text.push('(');
                rest = &after[1..];
            }
        }
    }
    text.push_str(rest);
    text
}

/// Writes the results of `cases` on `servers`, as text or Markdown, only the
/// cases where the servers differ when `differ`, the cells cut at `width`
/// characters (0: whole).
pub fn render(
    servers: &[Server],
    cases: &Cases,
    measures: &[HashMap<String, Measure>],
    markdown: bool,
    differ: bool,
    width: usize,
) -> String {
    let labels: Vec<String> = servers.iter().map(Server::label).collect();
    let mut out = String::new();
    if markdown {
        let _ = writeln!(out, "| id | expression | {} | same |", labels.join(" | "));
        let _ = writeln!(out, "|---|---|{}---|", "---|".repeat(labels.len()));
    } else {
        for (index, label) in labels.iter().enumerate() {
            let _ = writeln!(out, "[{}] {label}", index + 1);
        }
    }
    let id_width = cases
        .cases
        .iter()
        .map(|case| case.id.len())
        .max()
        .unwrap_or(0);
    for case in &cases.cases {
        let cells: Vec<String> = measures
            .iter()
            .map(|by_id| cell(by_id.get(&case.id), case.deterministic))
            .collect();
        let first = normalized(&cells[0]);
        let same = cells.iter().all(|cell| normalized(cell) == first);
        if differ && same {
            continue;
        }
        let cells: Vec<String> = cells.iter().map(|cell| cut(cell, width)).collect();
        if markdown {
            let escape = |text: &str| text.replace('|', "\\|");
            let _ = writeln!(
                out,
                "| {} | `{}` | {} | {} |",
                case.id,
                escape(&case.expression),
                cells
                    .iter()
                    .map(|cell| escape(cell))
                    .collect::<Vec<_>>()
                    .join(" | "),
                if same { "yes" } else { "**no**" }
            );
        } else {
            let mark = if same { ' ' } else { '*' };
            let _ = writeln!(out, "{:id_width$} {mark} {}", case.id, cells.join(" || "));
        }
    }
    out
}

/// Layout of the results of `render`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Layout {
    /// A line per case, a column per server.
    Columns,
    /// A line per case, then a line per server.
    Rows,
}

/// Results of `cases` on `servers`, each case followed by a line per server,
/// its label then its result: the layout of many servers. `*` marks the cases
/// where the servers differ; `differ` keeps only those; `markdown` writes a
/// table of a row per case and server.
pub fn render_rows(
    servers: &[Server],
    cases: &Cases,
    measures: &[HashMap<String, Measure>],
    markdown: bool,
    differ: bool,
    width: usize,
) -> String {
    let labels: Vec<String> = servers.iter().map(Server::label).collect();
    let label_width = labels.iter().map(String::len).max().unwrap_or(0);
    let mut out = String::new();
    if markdown {
        let _ = writeln!(out, "| id | expression | server | result |");
        let _ = writeln!(out, "|---|---|---|---|");
    }
    for case in &cases.cases {
        let cells: Vec<String> = measures
            .iter()
            .map(|by_id| cell(by_id.get(&case.id), case.deterministic))
            .collect();
        let first = cells.first().map(|cell| normalized(cell));
        let same = cells.iter().all(|cell| Some(normalized(cell)) == first);
        if differ && same {
            continue;
        }
        let mark = if same { ' ' } else { '*' };
        if !markdown {
            let _ = writeln!(out, "{mark} {} {}", case.id, case.expression);
        }
        for (label, cell) in labels.iter().zip(&cells) {
            let cell = cut(cell, width);
            if markdown {
                let escape = |text: &str| text.replace('|', "\\|");
                let _ = writeln!(
                    out,
                    "| {} | `{}` | {label} | {} |",
                    case.id,
                    escape(&case.expression),
                    escape(&cell)
                );
            } else {
                let _ = writeln!(out, "    {label:label_width$}  {cell}");
            }
        }
    }
    out
}

/// `text` cut at `width` characters, `…` marking the cut; whole for 0.
fn cut(text: &str, width: usize) -> String {
    if width == 0 || text.chars().count() <= width {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_cases_gives_its_setup_and_its_cases() {
        let text = "# Ranges\n! SET time_zone = '+00:00'\n\nut_1960 UNIX_TIMESTAMP('1960-01-01')\n~now  NOW(3)\n";
        let cases = Cases::parse(text).unwrap();
        assert_eq!(cases.setup, ["SET time_zone = '+00:00'"]);
        assert_eq!(
            cases.cases,
            [
                Case {
                    id: "ut_1960".to_string(),
                    expression: "UNIX_TIMESTAMP('1960-01-01')".to_string(),
                    deterministic: true,
                },
                Case {
                    id: "now".to_string(),
                    expression: "NOW(3)".to_string(),
                    deterministic: false,
                },
            ]
        );
        assert!(Cases::parse("alone").is_err());
        assert!(Cases::parse("a'b 1").is_err());
        assert!(Cases::parse("a 1\na 2").is_err());
    }

    #[test]
    fn the_fields_of_the_batch_output_are_read() {
        assert_eq!(field("NULL"), None);
        assert_eq!(field("=a\\tb\\nc\\\\d").as_deref(), Some("=a\tb\nc\\d"));
    }

    #[test]
    fn the_expressions_are_sent_as_hexadecimal_literals() {
        assert_eq!(text_literal("'é"), "CONVERT(X'27C3A9' USING utf8mb4)");
    }

    #[test]
    fn the_cells_give_type_value_warning_or_error() {
        let measure = Measure {
            column_type: Some("bigint(17)".to_string()),
            nullable: Some(true),
            value: None,
            warning: Some("Truncated incorrect unixtime value".to_string()),
            failure: None,
            type_failure: None,
        };
        assert_eq!(
            cell(Some(&measure), true),
            "bigint(17) NULL = NULL [warning: Truncated incorrect unixtime value]"
        );
        let failure = Measure {
            failure: Some("1305 FUNCTION probe.f does not exist".to_string()),
            ..Measure::default()
        };
        assert_eq!(
            cell(Some(&failure), true),
            "ERR 1305 FUNCTION probe.f does not exist"
        );
        assert_eq!(cell(None, true), "?");
    }

    #[test]
    fn the_display_widths_of_the_integers_are_ignored() {
        assert_eq!(normalized("int(11) NOT NULL = 0"), "int NOT NULL = 0");
        assert_eq!(
            normalized("bigint(21) unsigned NULL"),
            "bigint unsigned NULL"
        );
        assert_eq!(
            normalized("varchar(36) NULL = f(1)"),
            "varchar(36) NULL = f(1)"
        );
        assert_eq!(normalized("decimal(10,2) NULL"), "decimal(10,2) NULL");
    }

    #[test]
    fn long_cells_are_cut() {
        assert_eq!(cut("abcdef", 4), "abc…");
        assert_eq!(cut("abcdef", 0), "abcdef");
        assert_eq!(cut("abc", 4), "abc");
    }
}
