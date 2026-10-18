//! Lexer of the MySQL family: cuts a query into tokens, each with its
//! position in bytes (`docs/DEV-DIALECTS.md`, step 3).
//!
//! The lexer is set by the dialect, the version and the SQL mode of the
//! server (`LexerOptions`):
//!
//! - the SQL mode changes the strings: under `ANSI_QUOTES`, `"..."` is an
//!   identifier; under `NO_BACKSLASH_ESCAPES`, the backslash escapes nothing;
//! - executable comments (`/*! ... */`, `/*!80000 ... */`, `/*M! ... */`)
//!   are code for the server when their condition holds: the dialect runs
//!   them (`/*M!` is MariaDB only) and the version reaches the one written.
//!   Their content is then read as tokens, between `ConditionalStart` and
//!   `ConditionalEnd`; otherwise they are a comment. The caller gives the
//!   version they are resolved against: the version of the server towards
//!   the same dialect, the version of the input dialect otherwise
//!   (`docs/QUESTIONS.md`, point 41).
//!
//! Comments are tokens, for the report and the comments of the output;
//! whitespace is not, the positions giving it back. Words are not told
//! apart from keywords here: most keywords of the MySQL family are not
//! reserved, and the parser knows from the context which one it reads.
//!
//! The scan goes byte by byte: all the characters that delimit a token are
//! ASCII, and the bytes of a multi-byte UTF-8 character are always at least
//! 0x80, so they are never mistaken for them. Every byte from 0x80 belongs
//! to a word, as MySQL allows the characters U+0080 to U+FFFF in unquoted
//! identifiers; a character beyond U+FFFF is refused there.

mod location;
mod mode;
mod token;

#[cfg(test)]
mod tests;

use std::fmt;

use prisme_plan::Span;

pub use location::{LineIndex, Location};
pub use mode::SqlMode;
pub use token::{CommentKind, Punct, Quote, Token, TokenKind};

use crate::Dialect;
use crate::versions::Version;

/// Settings of the lexer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexerOptions {
    /// Dialect of the query.
    pub dialect: Dialect,
    /// Version against which the executable comments are resolved.
    pub version: Version,
    /// SQL mode of the server.
    pub mode: SqlMode,
}

/// Text that is not a token of the dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// What is wrong.
    pub message: String,
    /// Position of the faulty text.
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LexError {}

/// Cuts `text` into tokens.
///
/// # Errors
///
/// `LexError` on a string, a quoted identifier or a comment left open, a
/// malformed hexadecimal or bit literal, or a character that no token of the
/// dialect starts with.
pub fn tokenize(text: &str, options: &LexerOptions) -> Result<Vec<Token>, LexError> {
    Lexer {
        text,
        bytes: text.as_bytes(),
        options,
        pos: 0,
        tokens: Vec::new(),
        conditional: None,
    }
    .run()
}

/// State of the scan.
struct Lexer<'a> {
    /// Text scanned.
    text: &'a str,
    /// Bytes of the text.
    bytes: &'a [u8],
    /// Settings.
    options: &'a LexerOptions,
    /// Current byte.
    pos: usize,
    /// Tokens read so far.
    tokens: Vec<Token>,
    /// Start of the executable comment whose content is being read as code.
    conditional: Option<usize>,
}

impl Lexer<'_> {
    /// Reads every token of the text.
    fn run(mut self) -> Result<Vec<Token>, LexError> {
        while let Some(&byte) = self.bytes.get(self.pos) {
            let start = self.pos;
            let kind = match byte {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C => {
                    self.pos += 1;
                    continue;
                }
                b'#' => self.line_comment(CommentKind::Hash),
                b'-' if self.at_dash_comment() => self.line_comment(CommentKind::Dash),
                b'/' if self.peek(1) == Some(b'*') => self.block_comment()?,
                b'*' if self.conditional.is_some() && self.peek(1) == Some(b'/') => {
                    self.pos += 2;
                    self.conditional = None;
                    TokenKind::ConditionalEnd
                }
                b'\'' => self.string(Quote::Single)?,
                b'"' if self.options.mode.ansi_quotes => self.quoted_ident(b'"')?,
                b'"' => self.string(Quote::Double)?,
                b'`' => self.quoted_ident(b'`')?,
                b'N' | b'n' if self.peek(1) == Some(b'\'') => {
                    self.pos += 1;
                    TokenKind::NationalString(self.quoted(b'\'', self.escapes())?)
                }
                b'X' | b'x' if self.peek(1) == Some(b'\'') => self.hex_string()?,
                b'B' | b'b' if self.peek(1) == Some(b'\'') => self.bit_string()?,
                b'0'..=b'9' => self.number_or_word()?,
                b'.' if self.at_leading_decimal() => self.number_or_word()?,
                b'@' => self.variable()?,
                b'?' => {
                    self.pos += 1;
                    TokenKind::Placeholder
                }
                byte if is_word_byte(byte) => self.word()?,
                _ => self.punct()?,
            };
            self.tokens.push(Token {
                kind,
                span: Span::new(start, self.pos),
            });
        }
        if let Some(start) = self.conditional {
            return Err(self.error("unterminated comment", start, self.pos));
        }
        Ok(self.tokens)
    }

    /// Byte `offset` positions after the current one.
    fn peek(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// Error on `start..end`.
    fn error(&self, message: impl Into<String>, start: usize, end: usize) -> LexError {
        LexError {
            message: message.into(),
            span: Span::new(start, end.min(self.text.len())),
        }
    }

    /// Tells whether the backslash escapes in strings.
    fn escapes(&self) -> bool {
        !self.options.mode.no_backslash_escapes
    }

    /// End of the run of bytes from `from` that satisfy `accept`.
    fn scan_while(&self, from: usize, accept: impl Fn(u8) -> bool) -> usize {
        self.bytes[from.min(self.bytes.len())..]
            .iter()
            .position(|&byte| !accept(byte))
            .map_or(self.bytes.len(), |length| from + length)
    }

    /// Tells whether `--` at the current byte opens a comment: it must be
    /// followed by a space, a control character or the end of the text.
    fn at_dash_comment(&self) -> bool {
        self.peek(1) == Some(b'-') && self.peek(2).is_none_or(|byte| byte <= b' ')
    }

    /// Comment up to the end of the line, line feed excluded.
    fn line_comment(&mut self, kind: CommentKind) -> TokenKind {
        self.pos = self.scan_while(self.pos, |byte| byte != b'\n');
        TokenKind::Comment(kind)
    }

    /// `/* ... */`, `/*+ ... */`, or an executable comment.
    fn block_comment(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        let rest = &self.bytes[start + 2..];
        let executable = if rest.starts_with(b"!") {
            Some((false, start + 3))
        } else if rest.starts_with(b"M!") {
            Some((true, start + 4))
        } else {
            None
        };
        if let Some((mariadb_only, after_bang)) = executable {
            let (version, content) = self.conditional_version(after_bang);
            let runs = (!mariadb_only || self.options.dialect == Dialect::MariaDb)
                && version.is_none_or(|version| self.options.version >= version);
            if runs && self.conditional.is_none() {
                self.pos = content;
                self.conditional = Some(start);
                return Ok(TokenKind::ConditionalStart {
                    version,
                    mariadb_only,
                });
            }
            self.pos = self.comment_end(start)?;
            return Ok(TokenKind::Comment(CommentKind::Executable));
        }
        let kind = if rest.starts_with(b"+") {
            CommentKind::Hint
        } else {
            CommentKind::Block
        };
        self.pos = self.comment_end(start)?;
        Ok(TokenKind::Comment(kind))
    }

    /// Version written after the `!` of an executable comment at byte `at`,
    /// and the start of its content. MySQL reads six digits followed by a
    /// whitespace as a six-digit version (`/*!100000 `), otherwise five
    /// digits; MariaDB reads six digits when there are six. Fewer than five
    /// digits are content.
    fn conditional_version(&self, at: usize) -> (Option<Version>, usize) {
        let digits = self.scan_while(at, |byte| byte.is_ascii_digit()) - at;
        let six = digits >= 6
            && match self.options.dialect {
                Dialect::MySql => self
                    .bytes
                    .get(at + 6)
                    .is_some_and(|byte| byte.is_ascii_whitespace()),
                Dialect::MariaDb => true,
            };
        let length = match digits {
            _ if six => 6,
            5.. => 5,
            _ => return (None, at),
        };
        let number: u32 = self.text[at..at + length].parse().unwrap_or(0);
        let part = |value: u32| u16::try_from(value % 100).unwrap_or(0);
        let version = Version::new(
            u16::try_from(number / 10_000).unwrap_or(0),
            part(number / 100),
            part(number),
        );
        (Some(version), at + length)
    }

    /// End of the block comment opened at `start`, after its `*/`.
    fn comment_end(&self, start: usize) -> Result<usize, LexError> {
        self.text[start + 2..]
            .find("*/")
            .map(|offset| start + 2 + offset + 2)
            .ok_or_else(|| self.error("unterminated comment", start, self.text.len()))
    }

    /// String between quotes.
    fn string(&mut self, quote: Quote) -> Result<TokenKind, LexError> {
        let byte = match quote {
            Quote::Single => b'\'',
            Quote::Double => b'"',
        };
        let value = self.quoted(byte, self.escapes())?;
        Ok(TokenKind::String { value, quote })
    }

    /// Identifier between `quote`, the backslash escaping nothing.
    fn quoted_ident(&mut self, quote: u8) -> Result<TokenKind, LexError> {
        let start = self.pos;
        let value = self
            .quoted(quote, false)
            .map_err(|_| self.error("unterminated quoted identifier", start, self.text.len()))?;
        Ok(TokenKind::QuotedIdent(value))
    }

    /// Value between `quote`, at the current byte: a doubled quote stands
    /// for one, and with `escapes`, a backslash escapes the next character
    /// (`\n`, `\t`, `\0`...; `\%` and `\_` keep their backslash, for `LIKE`).
    fn quoted(&mut self, quote: u8, escapes: bool) -> Result<String, LexError> {
        let start = self.pos;
        let mut value = String::new();
        let mut piece = start + 1;
        let mut i = start + 1;
        while let Some(&byte) = self.bytes.get(i) {
            if byte == quote {
                value.push_str(&self.text[piece..i]);
                if self.bytes.get(i + 1) == Some(&quote) {
                    value.push(char::from(quote));
                    i += 2;
                    piece = i;
                    continue;
                }
                self.pos = i + 1;
                return Ok(value);
            }
            if byte == b'\\' && escapes && i + 1 < self.bytes.len() {
                value.push_str(&self.text[piece..i]);
                let next = self.bytes[i + 1];
                match next {
                    b'0' => value.push('\0'),
                    b'b' => value.push('\u{8}'),
                    b'n' => value.push('\n'),
                    b'r' => value.push('\r'),
                    b't' => value.push('\t'),
                    b'Z' => value.push('\u{1A}'),
                    b'%' | b'_' => {
                        value.push('\\');
                        value.push(char::from(next));
                    }
                    _ if next.is_ascii() => value.push(char::from(next)),
                    // A backslash before a multi-byte character escapes it as is.
                    _ => {
                        piece = i + 1;
                        i += 1;
                        continue;
                    }
                }
                i += 2;
                piece = i;
                continue;
            }
            i += 1;
        }
        Err(self.error("unterminated string", start, self.text.len()))
    }

    /// `X'4A'`: an even number of hexadecimal digits.
    fn hex_string(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        self.pos += 1;
        let digits = self.quoted(b'\'', false)?;
        if digits.len() % 2 != 0 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(self.error("invalid hexadecimal literal", start, self.pos));
        }
        Ok(TokenKind::HexString(digits))
    }

    /// `B'0101'`: binary digits.
    fn bit_string(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        self.pos += 1;
        let digits = self.quoted(b'\'', false)?;
        if !digits.bytes().all(|byte| byte == b'0' || byte == b'1') {
            return Err(self.error("invalid bit literal", start, self.pos));
        }
        Ok(TokenKind::BitString(digits))
    }

    /// Tells whether the `.` at the current byte starts a decimal (`.5`): it
    /// is followed by a digit, and does not qualify the name just before it
    /// (`t.5a` names the column `5a` of `t`).
    fn at_leading_decimal(&self) -> bool {
        let qualifies = self.tokens.last().is_some_and(|token| {
            token.span.end == self.pos
                && matches!(token.kind, TokenKind::Word | TokenKind::QuotedIdent(_))
        });
        !qualifies && self.peek(1).is_some_and(|byte| byte.is_ascii_digit())
    }

    /// Number, or word that starts with digits (`1a`, `0xZ`): MySQL allows
    /// an unquoted identifier to start with a digit, unless it is only
    /// digits.
    fn number_or_word(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        if self.bytes[start] == b'0' {
            let prefixed = match self.peek(1) {
                Some(b'x') => Some((true, self.scan_while(start + 2, |b| b.is_ascii_hexdigit()))),
                Some(b'b') => Some((
                    false,
                    self.scan_while(start + 2, |b| b == b'0' || b == b'1'),
                )),
                _ => None,
            };
            if let Some((hex, end)) = prefixed
                && end > start + 2
                && !self.bytes.get(end).copied().is_some_and(is_word_byte)
            {
                self.pos = end;
                let digits = self.text[start + 2..end].to_string();
                return Ok(if hex {
                    TokenKind::HexNumber(digits)
                } else {
                    TokenKind::BitNumber(digits)
                });
            }
        }
        let mut end = self.scan_while(start, |byte| byte.is_ascii_digit());
        let mut kind = TokenKind::Integer;
        if self.bytes.get(end) == Some(&b'.') {
            end = self.scan_while(end + 1, |byte| byte.is_ascii_digit());
            kind = TokenKind::Decimal;
        }
        if matches!(self.bytes.get(end), Some(b'e' | b'E')) {
            let mut digits = end + 1;
            if matches!(self.bytes.get(digits), Some(b'+' | b'-')) {
                digits += 1;
            }
            let exponent_end = self.scan_while(digits, |byte| byte.is_ascii_digit());
            if exponent_end > digits {
                end = exponent_end;
                kind = TokenKind::Float;
            }
        }
        if kind != TokenKind::Decimal && self.bytes.get(end).copied().is_some_and(is_word_byte) {
            return self.word();
        }
        self.pos = end;
        Ok(kind)
    }

    /// Unquoted word.
    fn word(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        self.pos = self.scan_while(start, is_word_byte);
        self.check_word(start)?;
        Ok(TokenKind::Word)
    }

    /// Refuses, in the word `start..pos`, a character beyond U+FFFF, which
    /// MySQL does not allow in an unquoted identifier.
    fn check_word(&self, start: usize) -> Result<(), LexError> {
        // A 4-byte UTF-8 sequence starts with a byte from 0xF0.
        match self.bytes[start..self.pos]
            .iter()
            .position(|&byte| byte >= 0xF0)
        {
            Some(offset) => {
                let at = start + offset;
                let length = self.text[at..].chars().next().map_or(1, char::len_utf8);
                Err(self.error(
                    "character not allowed in an unquoted identifier",
                    at,
                    at + length,
                ))
            }
            None => Ok(()),
        }
    }

    /// `@name`, `@'name'`, `@@name`, `` @@`name` ``, or `@` alone.
    fn variable(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        if self.peek(1) == Some(b'@') {
            // The name of a system variable takes backticks, no other quote.
            if self.peek(2) == Some(b'`') {
                self.pos += 2;
                let name = self.quoted(b'`', false)?;
                return Ok(TokenKind::SystemVariable { name, quoted: true });
            }
            let end = self.scan_while(start + 2, is_word_byte);
            if end == start + 2 {
                return Err(self.error("system variable name expected", start, start + 2));
            }
            self.pos = end;
            self.check_word(start + 2)?;
            return Ok(TokenKind::SystemVariable {
                name: self.text[start + 2..end].to_string(),
                quoted: false,
            });
        }
        match self.peek(1) {
            Some(quote @ (b'\'' | b'"' | b'`')) => {
                self.pos += 1;
                let escapes = quote != b'`' && self.escapes();
                let name = self.quoted(quote, escapes)?;
                Ok(TokenKind::UserVariable { name, quoted: true })
            }
            // The unquoted name of a user variable also takes dots.
            Some(byte) if is_word_byte(byte) || byte == b'.' => {
                let end = self.scan_while(start + 1, |byte| is_word_byte(byte) || byte == b'.');
                self.pos = end;
                self.check_word(start + 1)?;
                let name = self.text[start + 1..end].to_string();
                Ok(TokenKind::UserVariable {
                    name,
                    quoted: false,
                })
            }
            _ => {
                self.pos += 1;
                Ok(TokenKind::Punct(Punct::At))
            }
        }
    }

    /// Operator or punctuation, the longest that matches.
    fn punct(&mut self) -> Result<TokenKind, LexError> {
        let rest = &self.text[self.pos..];
        let found = Punct::BY_LENGTH
            .into_iter()
            .find(|punct| rest.starts_with(punct.as_str()));
        match found {
            Some(punct) => {
                self.pos += punct.as_str().len();
                Ok(TokenKind::Punct(punct))
            }
            None => {
                let length = rest.chars().next().map_or(1, char::len_utf8);
                let character = &rest[..length];
                Err(self.error(
                    format!("unexpected character {character:?}"),
                    self.pos,
                    self.pos + length,
                ))
            }
        }
    }
}

/// Tells whether a byte belongs to an unquoted word: ASCII letter, digit,
/// `_`, `$`, or byte of a non-ASCII character.
fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}
