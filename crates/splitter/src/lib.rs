//! Query splitter: splits a SQL text into queries, and finds the one under
//! the cursor (Ctrl+Enter), those a selection touches (Ctrl+Shift+Enter) or
//! the selected parts of them ("Exécuter précisément").
//!
//! Splitting follows the lexical rules of MySQL and MariaDB, without parsing
//! the SQL: a `;` separates two queries, except inside a string (`'…'`,
//! `"…"`), a quoted identifier (`` `…` ``) or a comment (`-- …`, `# …`,
//! `/* … */`). The `DELIMITER` command of the `mysql` client is recognised: it
//! changes the separator (for example `//` around a `CREATE PROCEDURE`) and is
//! never sent to the server.
//!
//! `split` gives the queries to run; `chunks` gives the whole text in pieces
//! (`DELIMITER` commands, queries with their comments and their separator),
//! for whoever must rewrite it without losing anything (the beautifier);
//! `text_end` tells how a query can be appended to the text (the navigator).
//!
//! Positions (`start`, `end`) are counted in UTF-16 units, like those of the
//! interface's editor (CodeMirror): a cursor compares to them directly.
//!
//! The scan goes byte by byte: all the characters that matter (quotes,
//! separators, comment markers) are ASCII, and the bytes of a multi-byte UTF-8
//! character are always at least 0x80, so they cannot be mistaken for them.
//! The scan may therefore stand inside a character: the separator is looked
//! for among the bytes, never by slicing the text there. A UTF-8 separator
//! can only match from the first byte of a character.

/// Default query separator.
const DEFAULT_DELIMITER: &str = ";";

/// Query cut out of the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    /// Text of the query, without its separator nor the comments and spaces before it.
    pub sql: String,
    /// Start of the query in the text, in UTF-16 units: first significant character.
    pub start: usize,
    /// End of the query in the text, in UTF-16 units, separator included.
    pub end: usize,
}

/// Piece of the text, in order: put end to end, the pieces give the text
/// back, except for whitespace around `DELIMITER` commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// `DELIMITER` command of the `mysql` client.
    Command {
        /// Line of the command, without the whitespace around it.
        line: String,
        /// New separator.
        delimiter: String,
    },
    /// Text of a query, with the spaces and comments before it.
    Sql {
        /// Text, without its separator; sometimes made only of whitespace
        /// and comments.
        text: String,
        /// Separator that ends it (`;`, `//`...), empty at the end of the text.
        delimiter: String,
    },
}

/// End of a text, for whoever appends a query to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEnd {
    /// Separator in force at the end of the text: `;`, or that of the last
    /// `DELIMITER` command.
    pub delimiter: String,
    /// Where the separator of the last query is missing, in UTF-16 units:
    /// right after its last significant character, before the comments and
    /// spaces that follow it. `None` if the text ends with no open query.
    pub open_end: Option<usize>,
}

/// Result of the scan of a text.
struct Scan {
    /// Commands and pieces, in order.
    events: Vec<Event>,
    /// Separator in force at the end of the text.
    delimiter: String,
    /// Byte that follows the last significant character of the text, 0 if there is none.
    token_end: usize,
}

/// What the scan of the text comes across.
enum Event {
    /// `DELIMITER` command, from byte `start` to the end of its line `end`.
    Command {
        start: usize,
        end: usize,
        delimiter: String,
    },
    /// Query (or whitespace and comments only) ended by `delimiter`.
    Statement {
        /// Start of the piece: end of the previous separator or command.
        chunk_start: usize,
        /// First significant byte, absent if there are only whitespace and comments.
        first: Option<usize>,
        /// Start of the separator, or end of the text.
        body_end: usize,
        /// End of the separator.
        end: usize,
        delimiter: String,
    },
}

/// Splits `text` into queries. Empty pieces (spaces, comments only) are ignored.
pub fn split(text: &str) -> Vec<Statement> {
    scan(text)
        .events
        .into_iter()
        .filter_map(|event| match event {
            Event::Statement {
                first: Some(first),
                body_end,
                end,
                ..
            } => Some(statement(text, first, body_end, end)),
            _ => None,
        })
        .collect()
}

/// Cuts `text` into pieces, without losing anything: `DELIMITER` commands,
/// queries with their comments, whitespace and comments at the end of the text.
pub fn chunks(text: &str) -> Vec<Chunk> {
    scan(text)
        .events
        .into_iter()
        .map(|event| match event {
            Event::Command {
                start,
                end,
                delimiter,
            } => Chunk::Command {
                line: text[start..end].trim().to_string(),
                delimiter,
            },
            Event::Statement {
                chunk_start,
                body_end,
                delimiter,
                ..
            } => Chunk::Sql {
                text: text[chunk_start..body_end].to_string(),
                delimiter,
            },
        })
        .collect()
}

/// Tells how a query can be appended to `text`: with which separator, and
/// where the last query lacks its own, so that the two do not make one.
pub fn text_end(text: &str) -> TextEnd {
    let scan = scan(text);
    let open = matches!(
        scan.events.last(),
        Some(Event::Statement { first: Some(_), delimiter, .. }) if delimiter.is_empty()
    );
    TextEnd {
        open_end: open.then(|| utf16_offset(text, scan.token_end)),
        delimiter: scan.delimiter,
    }
}

/// Scans `text` and collects, in order, the `DELIMITER` commands and the
/// pieces that end with a separator or with the end of the text.
fn scan(text: &str) -> Scan {
    let bytes = text.as_bytes();
    let mut events = Vec::new();
    let mut delimiter = DEFAULT_DELIMITER.to_string();
    // Start of the current piece, and its first significant byte if there is one.
    let mut chunk_start = 0;
    let mut start: Option<usize> = None;
    let mut token_end = 0;
    let mut i = 0;

    while i < bytes.len() {
        if start.is_none()
            && at_line_start(bytes, i)
            && let Some((new_delimiter, line_end)) = delimiter_command(text, i)
        {
            // Comments before the command: a piece of their own.
            if !text[chunk_start..i].trim().is_empty() {
                events.push(Event::Statement {
                    chunk_start,
                    first: None,
                    body_end: i,
                    end: i,
                    delimiter: String::new(),
                });
            }
            events.push(Event::Command {
                start: i,
                end: line_end,
                delimiter: new_delimiter.clone(),
            });
            delimiter = new_delimiter;
            i = line_end;
            chunk_start = i;
            continue;
        }
        if bytes[i..].starts_with(delimiter.as_bytes()) {
            events.push(Event::Statement {
                chunk_start,
                first: start.take(),
                body_end: i,
                end: i + delimiter.len(),
                delimiter: delimiter.clone(),
            });
            i += delimiter.len();
            chunk_start = i;
            continue;
        }
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'-' if is_dash_comment(bytes, i) => i = line_end(bytes, i),
            b'#' => i = line_end(bytes, i),
            b'/' if bytes.get(i + 1) == Some(&b'*') => i = block_comment_end(bytes, i),
            quote @ (b'\'' | b'"' | b'`') => {
                start.get_or_insert(i);
                i = quoted_end(bytes, i, quote);
                token_end = i;
            }
            _ => {
                start.get_or_insert(i);
                i += 1;
                token_end = i;
            }
        }
    }
    if start.is_some() || !text[chunk_start..].trim().is_empty() {
        events.push(Event::Statement {
            chunk_start,
            first: start,
            body_end: bytes.len(),
            end: bytes.len(),
            delimiter: String::new(),
        });
    }
    Scan {
        events,
        delimiter,
        token_end,
    }
}

/// Index of the query under the cursor `cursor` (in UTF-16 units).
///
/// It is the last query that starts before the cursor: a cursor placed
/// between two queries (after the separator of one, before the start of the
/// next) designates the previous one. Before the first query, it is the
/// first one. `None` if there is no query.
pub fn statement_at(statements: &[Statement], cursor: usize) -> Option<usize> {
    if statements.is_empty() {
        return None;
    }
    Some(
        statements
            .iter()
            .rposition(|statement| statement.start <= cursor)
            .unwrap_or(0),
    )
}

/// Indexes of the queries that the passage from `start` to `end` (UTF-16
/// units, `end` excluded) touches: those that share at least one character
/// with it, from their first significant character to their separator. The
/// comments and spaces before a query are not part of it. An empty passage
/// touches none.
pub fn statements_in(statements: &[Statement], start: usize, end: usize) -> Vec<usize> {
    if start >= end {
        return Vec::new();
    }
    statements
        .iter()
        .enumerate()
        .filter(|(_, statement)| statement.start < end && start < statement.end)
        .map(|(index, _)| index)
        .collect()
}

/// Parts of the queries that the passage from `start` to `end` (UTF-16
/// units, `end` excluded) touches, as `statements_in` finds them: of each
/// one, only the text inside the passage, without the whitespace around it
/// nor the separator. A part left with whitespace only is left out.
pub fn pieces_in(statements: &[Statement], start: usize, end: usize) -> Vec<Statement> {
    statements_in(statements, start, end)
        .into_iter()
        .filter_map(|index| piece(&statements[index], start, end))
        .collect()
}

/// Part of `statement` inside the passage from `start` to `end` (UTF-16
/// units of the whole text), trimmed; `None` if only whitespace remains.
fn piece(statement: &Statement, start: usize, end: usize) -> Option<Statement> {
    let text = &statement.sql;
    let from = byte_at(text, start.saturating_sub(statement.start));
    let to = byte_at(text, end.saturating_sub(statement.start));
    let inside = &text[from..to];
    let first = from + inside.len() - inside.trim_start().len();
    let sql = inside.trim();
    (!sql.is_empty()).then(|| Statement {
        sql: sql.to_string(),
        start: statement.start + utf16_offset(text, first),
        end: statement.start + utf16_offset(text, first + sql.len()),
    })
}

/// Byte of `text` that starts `unit` UTF-16 units after its start; a unit
/// inside a character goes to the end of that character, a unit beyond the
/// text to its end.
fn byte_at(text: &str, unit: usize) -> usize {
    let mut units = 0;
    for (byte, character) in text.char_indices() {
        if units >= unit {
            return byte;
        }
        units += character.len_utf16();
    }
    text.len()
}

/// Builds a query from its byte positions.
fn statement(text: &str, first: usize, body_end: usize, end: usize) -> Statement {
    Statement {
        sql: text[first..body_end].trim_end().to_string(),
        start: utf16_offset(text, first),
        end: utf16_offset(text, end),
    }
}

/// Position in UTF-16 units of byte `byte` of the text.
fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// Tells whether byte `i` is only preceded, on its line, by spaces.
fn at_line_start(bytes: &[u8], i: usize) -> bool {
    bytes[..i]
        .iter()
        .rev()
        .take_while(|&&byte| byte != b'\n')
        .all(|&byte| byte == b' ' || byte == b'\t' || byte == b'\r')
}

/// Recognises a `DELIMITER x` command starting at byte `i` (spaces
/// included). Returns the new separator and the end of the line.
fn delimiter_command(text: &str, i: usize) -> Option<(String, usize)> {
    let end = line_end(text.as_bytes(), i);
    let line = text[i..end].trim();
    let (keyword, rest) = line.split_once(char::is_whitespace)?;
    if !keyword.eq_ignore_ascii_case("delimiter") {
        return None;
    }
    let new_delimiter = rest.split_whitespace().next()?;
    Some((new_delimiter.to_string(), end))
}

/// Tells whether `--` at byte `i` opens a comment: it must be followed by a
/// space, a control character or the end of the text (MySQL rule).
fn is_dash_comment(bytes: &[u8], i: usize) -> bool {
    bytes.get(i + 1) == Some(&b'-') && bytes.get(i + 2).is_none_or(|&byte| byte <= b' ')
}

/// Position of the line feed that ends the line of byte `i`, or end of the text.
fn line_end(bytes: &[u8], i: usize) -> usize {
    bytes[i..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(bytes.len(), |offset| i + offset)
}

/// Position following the end of the `/* … */` comment opened at byte `i`.
fn block_comment_end(bytes: &[u8], i: usize) -> usize {
    bytes[i + 2..]
        .windows(2)
        .position(|pair| pair == b"*/")
        .map_or(bytes.len(), |offset| i + 2 + offset + 2)
}

/// Position following the end of the string or identifier opened by `quote` at byte `i`.
///
/// A doubled quote stays in the string. In `'…'` and `"…"`, the backslash
/// escapes the next character; not in `` `…` ``.
fn quoted_end(bytes: &[u8], i: usize, quote: u8) -> usize {
    let mut j = i + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' if quote != b'`' => j += 2,
            byte if byte == quote => {
                if bytes.get(j + 1) == Some(&quote) {
                    j += 2;
                } else {
                    return j + 1;
                }
            }
            _ => j += 1,
        }
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqls(text: &str) -> Vec<String> {
        split(text)
            .into_iter()
            .map(|statement| statement.sql)
            .collect()
    }

    #[test]
    fn splits_on_semicolons() {
        assert_eq!(
            sqls("select * from xlist.video;\nselect * from xlist.person;\n"),
            ["select * from xlist.video", "select * from xlist.person"]
        );
        assert_eq!(sqls("SELECT 1"), ["SELECT 1"]);
        assert!(sqls("  ;; -- none\n").is_empty());
    }

    #[test]
    fn ignores_semicolons_in_strings_identifiers_and_comments() {
        assert_eq!(
            sqls("SELECT 'a;b', \"c;d\", `e;f`; SELECT 'it''s; ok', 'x\\';y'"),
            [
                "SELECT 'a;b', \"c;d\", `e;f`",
                "SELECT 'it''s; ok', 'x\\';y'"
            ]
        );
        assert_eq!(
            sqls("SELECT 1 -- end; not here\n; # none; either\nSELECT /* ; */ 2"),
            ["SELECT 1 -- end; not here", "SELECT /* ; */ 2"]
        );
    }

    #[test]
    fn double_dash_needs_a_space_to_open_a_comment() {
        assert_eq!(sqls("SELECT 5--2; SELECT 3"), ["SELECT 5--2", "SELECT 3"]);
    }

    #[test]
    fn statements_start_after_leading_comments() {
        let statements = split("-- title\n/* note */ SELECT 1;");
        assert_eq!(statements[0].sql, "SELECT 1");
        assert_eq!(statements[0].start, 20);
        assert_eq!(statements[0].end, 29);
    }

    #[test]
    fn follows_the_delimiter_command() {
        let text = "DELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END //\ndelimiter ;\nCALL p();";
        assert_eq!(
            sqls(text),
            [
                "CREATE PROCEDURE p() BEGIN SELECT 1; SELECT 2; END",
                "CALL p()"
            ]
        );
    }

    #[test]
    fn cuts_the_whole_text_into_chunks() {
        let text = "-- purge\nDELIMITER //\nCREATE PROCEDURE p() BEGIN SELECT 1; END //\n delimiter ;\nCALL p(); # end\n";
        assert_eq!(
            chunks(text),
            [
                Chunk::Sql {
                    text: "-- purge\n".to_string(),
                    delimiter: String::new(),
                },
                Chunk::Command {
                    line: "DELIMITER //".to_string(),
                    delimiter: "//".to_string(),
                },
                Chunk::Sql {
                    text: "\nCREATE PROCEDURE p() BEGIN SELECT 1; END ".to_string(),
                    delimiter: "//".to_string(),
                },
                // Whitespace alone before a command does not make a piece.
                Chunk::Command {
                    line: "delimiter ;".to_string(),
                    delimiter: ";".to_string(),
                },
                Chunk::Sql {
                    text: "\nCALL p()".to_string(),
                    delimiter: ";".to_string(),
                },
                Chunk::Sql {
                    text: " # end\n".to_string(),
                    delimiter: String::new(),
                },
            ]
        );
    }

    #[test]
    fn counts_positions_in_utf16_units() {
        // "é" counts for 1 UTF-16 unit (2 bytes), "😀" for 2 units (4 bytes).
        let statements = split("SELECT 'é😀';\nSELECT 2;");
        assert_eq!(statements[0].end, 13);
        assert_eq!(statements[1].start, 14);
    }

    #[test]
    fn splits_around_multibyte_characters_outside_strings() {
        // Ticket #12: the scan stood inside "é" when looking for the separator.
        assert_eq!(
            sqls("SELECT prénom FROM élève; SELECT 'à' AS ça"),
            ["SELECT prénom FROM élève", "SELECT 'à' AS ça"]
        );
        assert_eq!(
            sqls("DELIMITER €\nSELECT ç€SELECT 2"),
            ["SELECT ç", "SELECT 2"]
        );
    }

    #[test]
    fn finds_the_statement_under_the_cursor() {
        let text = "select * from xlist.video;\n\nselect * from xlist.person;\n";
        let statements = split(text);
        let second = text.find("select * from xlist.person").unwrap();

        assert_eq!(statement_at(&statements, 0), Some(0));
        assert_eq!(statement_at(&statements, 10), Some(0));
        // Empty line between the two: the previous one.
        assert_eq!(statement_at(&statements, 27), Some(0));
        assert_eq!(statement_at(&statements, second), Some(1));
        assert_eq!(statement_at(&statements, text.len()), Some(1));
        assert_eq!(statement_at(&[], 0), None);
    }

    #[test]
    fn finds_the_statements_a_passage_touches() {
        // Queries at 12..21, 23..32 and 34..43.
        let text = "-- comment\n\nSELECT 1;\n\nSELECT 2;\n\nSELECT 3;\n";
        let statements = split(text);
        assert_eq!(statements_in(&statements, 0, text.len()), [0, 1, 2]);
        // A single character of the query, or its separator, is enough.
        assert_eq!(statements_in(&statements, 30, 31), [1]);
        assert_eq!(statements_in(&statements, 20, 24), [0, 1]);
        // The comment before a query and the blanks between two do not count.
        assert!(statements_in(&statements, 0, 12).is_empty());
        assert!(statements_in(&statements, 21, 23).is_empty());
        // An empty passage, a cursor, touches nothing.
        assert!(statements_in(&statements, 25, 25).is_empty());
    }

    #[test]
    fn cuts_out_the_selected_parts_of_the_statements() {
        // Queries at 0..9 and 10..53.
        let text = "SELECT 1;
SELECT id, name FROM user WHERE id > 1;";
        let statements = split(text);
        let pieces = |start, end| {
            pieces_in(&statements, start, end)
                .into_iter()
                .map(|piece| (piece.sql, piece.start, piece.end))
                .collect::<Vec<_>>()
        };
        let user = text.find("user").unwrap();
        // From the second SELECT to "user": its WHERE clause is left out.
        assert_eq!(
            pieces(10, user + 4),
            [("SELECT id, name FROM user".to_string(), 10, user + 4)]
        );
        // From the first SELECT to "user": two queries, the second one cut.
        assert_eq!(
            pieces(0, user + 4),
            [
                ("SELECT 1".to_string(), 0, 8),
                ("SELECT id, name FROM user".to_string(), 10, user + 4)
            ]
        );
        // The blanks around a part go; a separator alone is no part.
        let from = text.find(" FROM").unwrap();
        assert_eq!(
            pieces(from, user + 5),
            [("FROM user".to_string(), from + 1, user + 4)]
        );
        assert!(pieces(8, 10).is_empty());
        // Positions in UTF-16 units, past a character outside the BMP.
        let statements = split("SELECT '😀', 2 FROM t;");
        let piece = &pieces_in(&statements, 0, 14)[0];
        assert_eq!((piece.sql.as_str(), piece.end), ("SELECT '😀', 2", 14));
        // A separator set by DELIMITER is kept inside the parts.
        let statements = split(
            "DELIMITER //
CREATE PROCEDURE p() BEGIN SELECT 1; END//",
        );
        assert_eq!(
            pieces_in(&statements, 0, 100)[0].sql,
            "CREATE PROCEDURE p() BEGIN SELECT 1; END"
        );
    }

    #[test]
    fn tells_how_to_append_a_query() {
        let end = |text: &str| {
            let end = text_end(text);
            (end.delimiter, end.open_end)
        };
        assert_eq!(end("SELECT 1;\n"), (";".to_string(), None));
        assert_eq!(end(""), (";".to_string(), None));
        assert_eq!(end("-- notes only\n"), (";".to_string(), None));
        // Last query without its separator: it goes before the comment that follows.
        assert_eq!(
            end("SELECT 1;\nSELECT 'é' -- note\n"),
            (";".to_string(), Some(20))
        );
        assert_eq!(end("SELECT `a;b`"), (";".to_string(), Some(12)));
        // Separator of the last DELIMITER command.
        assert_eq!(end("DELIMITER //\nCALL p()//\n"), ("//".to_string(), None));
        assert_eq!(end("DELIMITER //\nCALL p()"), ("//".to_string(), Some(21)));
    }

    #[test]
    fn cursor_before_the_first_statement_designates_it() {
        let statements = split("\n\n-- comment\nSELECT 1;");
        assert_eq!(statement_at(&statements, 0), Some(0));
    }
}
