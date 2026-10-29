//! CSV for Excel in French, as the export of a full result writes it.
//!
//! Same format as the download of the interface (`web/src/components/csv.ts`,
//! which details its rules): UTF-8 with a BOM, `;` as separator, CRLF line
//! endings, the column names on the first line, RFC 4180 quoting. NULL is an
//! empty field, an empty string `""`; the numbers of the `decimal` and
//! `float` families take the decimal comma. The reference files of
//! `tests/fixtures/export/csv-excel-fr/` keep both writers identical.

use prisme_connectors::mysql::{Cell, TypeFamily};

/// Byte order mark, by which Excel recognises UTF-8.
const BOM: char = '\u{FEFF}';

/// Separator of the fields.
const SEPARATOR: char = ';';

/// End of a line, RFC 4180's.
const LINE_END: &str = "\r\n";

/// Writes the start of the file: the BOM, then the line of the column names.
pub fn write_start<'a>(out: &mut String, names: impl IntoIterator<Item = &'a str>) {
    out.push(BOM);
    for (position, name) in names.into_iter().enumerate() {
        if position > 0 {
            out.push(SEPARATOR);
        }
        write_field(out, name);
    }
    out.push_str(LINE_END);
}

/// Writes the line of a row.
///
/// `order` gives the index in `row` of each written column, `families` the
/// family of its type.
pub fn write_row(out: &mut String, row: &[Cell], order: &[usize], families: &[TypeFamily]) {
    for (position, (&index, &family)) in order.iter().zip(families).enumerate() {
        if position > 0 {
            out.push(SEPARATOR);
        }
        write_cell(out, row.get(index).and_then(Option::as_deref), family);
    }
    out.push_str(LINE_END);
}

/// Writes the field of a cell, `None` for NULL.
fn write_cell(out: &mut String, cell: Option<&str>, family: TypeFamily) {
    match cell {
        None => {}
        Some("") => out.push_str("\"\""),
        Some(value) if matches!(family, TypeFamily::Decimal | TypeFamily::Float) => {
            write_field(out, &value.replacen('.', ",", 1));
        }
        Some(value) => write_field(out, value),
    }
}

/// Writes a field, between quotes when it holds `;`, `"`, CR or LF.
fn write_field(out: &mut String, value: &str) {
    if value.contains([SEPARATOR, '"', '\r', '\n']) {
        out.push('"');
        out.push_str(&value.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(value);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde::Deserialize;

    use super::*;

    /// Input of the reference files: names, families and rows of a result.
    #[derive(Deserialize)]
    struct Reference {
        columns: Vec<String>,
        families: Vec<TypeFamily>,
        rows: Vec<Vec<Cell>>,
    }

    /// Folder of the reference files, shared with the interface's tests.
    fn references() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/export/csv-excel-fr")
    }

    #[test]
    fn writes_the_reference_file_byte_for_byte() {
        let input = std::fs::read_to_string(references().join("input.json")).unwrap();
        let reference: Reference = serde_json::from_str(&input).unwrap();
        let order: Vec<usize> = (0..reference.columns.len()).collect();

        let mut out = String::new();
        write_start(&mut out, reference.columns.iter().map(String::as_str));
        for row in &reference.rows {
            write_row(&mut out, row, &order, &reference.families);
        }
        let expected = std::fs::read(references().join("expected.csv")).unwrap();
        assert_eq!(out.as_bytes(), expected.as_slice());
    }

    #[test]
    fn writes_the_columns_in_the_given_order() {
        let row = [Some("1".to_string()), None, Some("2.5".to_string())];
        let mut out = String::new();
        write_row(
            &mut out,
            &row,
            &[2, 0, 1],
            &[TypeFamily::Decimal, TypeFamily::Integer, TypeFamily::Text],
        );
        assert_eq!(out, "2,5;1;\r\n");
    }
}
