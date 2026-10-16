//! Whitespace normalisation, first rule of the beautifier.
//!
//! A text copied from a word processor, a web page or a messaging app often
//! contains "exotic" whitespace: no-break space, thin space, ideographic
//! space... Invisible on screen, they get in the way of tokenising and of the
//! layout. They become normal spaces here, Unicode line separators become
//! line breaks, and a tab becomes four spaces.
//!
//! Zero-width invisible characters (zero-width space, byte order mark...) are
//! removed: they are not whitespace in the Unicode sense, but they arrive the
//! same way and make a query fail without showing anything.
//!
//! Strings (`'...'`, `"..."`) and identifiers between backticks stay intact:
//! a whitespace there is part of the value, changing it would change the
//! query. Comments are normalised like the rest: they have no effect on the
//! query.

/// A tab becomes this number of spaces.
const TAB_WIDTH: usize = 4;

/// Place in the text where the character read is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Ordinary SQL code.
    Code,
    /// String or identifier delimited by this character (`'`, `"` or `` ` ``).
    Quoted(char),
    /// Comment up to the end of the line (`-- ` or `#`).
    LineComment,
    /// Comment between `/*` and `*/`.
    BlockComment,
}

/// Replaces exotic whitespace and tabs outside strings and identifiers
/// between backticks. See the module header.
pub(crate) fn normalize_blanks(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut context = Context::Code;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        let next = chars.peek().copied();
        match context {
            Context::Quoted(quote) => {
                result.push(c);
                if c == '\\' && quote != '`' {
                    // Escaped character (`\'`): copied as is, it closes nothing.
                    if let Some(escaped) = chars.next() {
                        result.push(escaped);
                    }
                } else if c == quote {
                    if next == Some(quote) {
                        // Doubled delimiter (`''`): a delimiter inside the value.
                        result.push(chars.next().unwrap_or(quote));
                    } else {
                        context = Context::Code;
                    }
                }
                continue;
            }
            Context::Code => match c {
                '\'' | '"' | '`' => context = Context::Quoted(c),
                '#' => context = Context::LineComment,
                // MySQL only opens a `--` comment when followed by whitespace.
                '-' if next == Some('-') => {
                    let mut ahead = chars.clone();
                    ahead.next();
                    if ahead.peek().is_none_or(|after| after.is_whitespace()) {
                        context = Context::LineComment;
                    }
                }
                '/' if next == Some('*') => context = Context::BlockComment,
                _ => {}
            },
            Context::LineComment => {
                if c == '\n' {
                    context = Context::Code;
                }
            }
            Context::BlockComment => {
                if c == '*' && next == Some('/') {
                    result.push(c);
                    result.push(chars.next().unwrap_or('/'));
                    context = Context::Code;
                    continue;
                }
            }
        }
        push_normalized(&mut result, c);
    }
    result
}

/// Adds `c` to `result`, normalised if it is whitespace.
fn push_normalized(result: &mut String, c: char) {
    match c {
        '\t' => result.extend(std::iter::repeat_n(' ', TAB_WIDTH)),
        // Unicode line and paragraph separators, "next line" (NEL).
        '\u{2028}' | '\u{2029}' | '\u{0085}' => result.push('\n'),
        // Any other non-ASCII whitespace: no-break, thin, em spaces...
        c if c.is_whitespace() && !c.is_ascii() => result.push(' '),
        // Zero-width invisibles: zero-width space, non-joiner, joiner, word
        // joiner, byte order mark (BOM).
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}' => {}
        c => result.push(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_exotic_blanks_and_tabs() {
        assert_eq!(
            normalize_blanks("SELECT\u{00A0}a,\u{202F}b\tFROM\u{3000}t\u{2028}WHERE x"),
            "SELECT a, b    FROM t\nWHERE x"
        );
    }

    #[test]
    fn removes_zero_width_characters_outside_strings() {
        assert_eq!(
            normalize_blanks("\u{FEFF}SELECT\u{200B} a FROM t\u{2060} WHERE b = 'x\u{200B}y'"),
            "SELECT a FROM t WHERE b = 'x\u{200B}y'"
        );
    }

    #[test]
    fn keeps_strings_and_quoted_identifiers_intact() {
        let sql = "SELECT 'a\u{00A0}b', \"c\td\", `e\u{2009}f` FROM t";
        assert_eq!(normalize_blanks(sql), sql);
    }

    #[test]
    fn follows_escapes_and_doubled_quotes() {
        // The string goes on after \' and '': the whitespace that follows is still in it.
        let sql = "SELECT 'it\\'s\u{00A0}', 'l''a\u{00A0}' ,\u{00A0}x";
        assert_eq!(
            normalize_blanks(sql),
            "SELECT 'it\\'s\u{00A0}', 'l''a\u{00A0}' , x"
        );
    }

    #[test]
    fn normalizes_comments_without_misreading_quotes() {
        // The apostrophe of a comment does not open a string.
        assert_eq!(
            normalize_blanks("-- it's\u{00A0}ok\nSELECT\u{00A0}1 /* l'a\tb */"),
            "-- it's ok\nSELECT 1 /* l'a    b */"
        );
        assert_eq!(normalize_blanks("# x'\nSELECT\u{00A0}1"), "# x'\nSELECT 1");
        // Without whitespace after --, it is a double subtraction.
        assert_eq!(
            normalize_blanks("SELECT 1--1,\u{00A0}'a'"),
            "SELECT 1--1, 'a'"
        );
    }
}
