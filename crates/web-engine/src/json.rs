//! JSON answers of the module: the shape of a computed table, a window of
//! it, the distinct values of a column, the errors. Written by hand: the
//! module depends on the engine alone.

use std::fmt::Write as _;

use prisme_pivot::{Engine, Error, Label, Table, Value};
use prisme_store::{BlockError, NumberError};

/// Writes a text as a JSON string.
pub fn string(json: &mut String, text: &str) {
    json.push('"');
    for character in text.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            control if control < ' ' => {
                let _ = write!(json, "\\u{:04x}", u32::from(control));
            }
            other => json.push(other),
        }
    }
    json.push('"');
}

/// Writes a cell: its text as a JSON string, `null` for NULL.
fn cell(json: &mut String, cell: Option<&str>) {
    match cell {
        Some(text) => string(json, text),
        None => json.push_str("null"),
    }
}

/// Writes the items of a list as a JSON array, each by `item`.
fn array<T>(
    json: &mut String,
    items: impl IntoIterator<Item = T>,
    mut item: impl FnMut(&mut String, T),
) {
    json.push('[');
    for (index, value) in items.into_iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        item(json, value);
    }
    json.push(']');
}

/// Writes the distinct values of a column, by code.
pub fn distinct(json: &mut String, values: &[Option<&str>]) {
    array(json, values, |json, &value| cell(json, value));
}

/// Writes the shape of a computed table: its numbers of lines, of columns of
/// values, of levels on each axis, of finest groups and of rows taken into
/// account.
pub fn shape(json: &mut String, table: &Table) {
    let _ = write!(
        json,
        r#"{{"lines":{},"columns":{},"rowLevels":{},"columnLevels":{},"groups":{},"rows":{}}}"#,
        table.line_count(),
        table.column_count(),
        table.row_levels(),
        table.column_levels(),
        table.finest_groups(),
        table.selected_rows(),
    );
}

/// Writes a window of a table: the lines from `lines.start` to `lines.end`,
/// the columns of values from `columns.start` to `columns.end`, both bounded
/// by the table.
///
/// - `lines`: `{"depth", "label", "path"}` of each line, depth 0 for the grand
///   total, `path` holding the value of each of its levels;
/// - `columns`: `{"node", "path", "total", "measure"}` of each column, `path`
///   holding `[node, value]` per level;
/// - `cells`: one array per line, a value as a string, `null` for NULL,
///   `false` for an empty cell.
pub fn window(
    json: &mut String,
    engine: &Engine,
    table: &Table,
    lines: std::ops::Range<usize>,
    columns: std::ops::Range<usize>,
) {
    let lines = lines.start.min(table.line_count())..lines.end.min(table.line_count());
    let columns = columns.start.min(table.column_count())..columns.end.min(table.column_count());
    json.push_str(r#"{"lines":"#);
    array(json, lines.clone(), |json, index| {
        let line = table.line(engine, index);
        let _ = write!(json, r#"{{"depth":{},"label":"#, line.depth);
        match line.label {
            Label::Value(value) => cell(json, value),
            Label::Total => json.push_str("null"),
        }
        json.push_str(r#","path":"#);
        array(json, table.line_path(engine, index), |json, value| {
            cell(json, value)
        });
        json.push('}');
    });
    json.push_str(r#","columns":"#);
    array(json, columns.clone(), |json, index| {
        let header = table.header(engine, index);
        let _ = write!(json, r#"{{"node":{},"path":"#, header.node);
        array(json, &header.path, |json, &(node, value)| {
            let _ = write!(json, "[{node},");
            cell(json, value);
            json.push(']');
        });
        let _ = write!(json, r#","total":{},"measure":"#, header.total);
        match header.measure {
            Some(measure) => {
                let _ = write!(json, "{measure}");
            }
            None => json.push_str("null"),
        }
        json.push('}');
    });
    json.push_str(r#","cells":"#);
    array(json, lines, |json, line| {
        array(json, columns.clone(), |json, column| {
            match table.value(line, column) {
                Value::Empty => json.push_str("false"),
                value => cell(json, value.text(engine.store()).as_deref()),
            }
        });
    });
    json.push('}');
}

/// Writes an error of the engine: `{"error": {"kind", …}}`.
pub fn error(json: &mut String, error: &Error) {
    json.push_str(r#"{"error":"#);
    let _ = match error {
        Error::UnknownColumn(column) => {
            write!(json, r#"{{"kind":"unknown_column","column":{column}}}"#)
        }
        Error::UnknownRow(row) => write!(json, r#"{{"kind":"unknown_row","row":{row}}}"#),
        Error::Ranks(column) => write!(json, r#"{{"kind":"ranks","column":{column}}}"#),
        Error::NotNumeric { measure } => {
            write!(json, r#"{{"kind":"not_numeric","measure":{measure}}}"#)
        }
        Error::Number { column, error } => {
            let (kind, row) = match error {
                NumberError::Invalid { row } => ("invalid_number", row),
                NumberError::TooLarge { row } => ("too_large", row),
            };
            write!(json, r#"{{"kind":"{kind}","column":{column},"row":{row}}}"#)
        }
        Error::Overflow { measure } => write!(json, r#"{{"kind":"overflow","measure":{measure}}}"#),
        Error::InvalidDate { column, row } => {
            write!(
                json,
                r#"{{"kind":"invalid_date","column":{column},"row":{row}}}"#
            )
        }
    };
    json.push('}');
}

/// Writes an error of the module itself: a call out of order, a block of
/// cells refused.
pub fn state_error(json: &mut String, kind: &str, block: Option<&BlockError>) {
    let _ = write!(json, r#"{{"error":{{"kind":"{kind}""#);
    if let Some(block) = block {
        json.push_str(r#","detail":"#);
        string(json, &block.to_string());
    }
    json.push_str("}}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        let mut json = String::new();
        string(&mut json, "a\"b\\c\nd\u{1}é");
        assert_eq!(json, r#""a\"b\\c\nd\u0001é""#);
    }

    #[test]
    fn a_cell_outside_of_the_calendar_gives_its_column_and_its_row() {
        let mut json = String::new();
        error(&mut json, &Error::InvalidDate { column: 2, row: 7 });
        assert_eq!(
            json,
            r#"{"error":{"kind":"invalid_date","column":2,"row":7}}"#
        );
    }
}
