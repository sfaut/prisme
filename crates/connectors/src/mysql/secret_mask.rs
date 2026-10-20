//! Masking of the secrets of a MySQL or MariaDB statement, before it is
//! traced (`sql_log.rs`): the history of the console and the log keep the
//! statement, without its passwords.
//!
//! The text is read lexically: strings, quoted identifiers and comments are
//! recognised, and a string is replaced by `'***'` when the words before it
//! make it a secret:
//!
//! - `IDENTIFIED BY 'pw'`, `IDENTIFIED BY PASSWORD 'hash'`;
//! - `IDENTIFIED WITH plugin BY | AS 'pw'`, `IDENTIFIED VIA plugin USING | AS
//!   'pw'`, and the next plugins of MariaDB (`OR plugin USING 'pw'`);
//! - `PASSWORD('pw')`;
//! - a word ending with `password` followed by the string, with or without
//!   `=`: `MASTER_PASSWORD = 'pw'`, `SOURCE_PASSWORD = 'pw'`,
//!   `OPTIONS (PASSWORD 'pw')`, and also a comparison with a column named
//!   `password` (`WHERE password = 'pw'`);
//! - `SET PASSWORD [FOR user] = 'pw'`, and `REPLACE 'old'` in a statement
//!   that sets a password.
//!
//! The text stays as written otherwise. A statement without any of the
//! words `password`, `identified` and `replace` is returned as is, without
//! being read.

use std::borrow::Cow;

/// Text written in place of a secret, between its quotes.
const MASK: &str = "***";

/// Words of which a statement must hold one to carry a secret.
const HINTS: [&str; 3] = ["password", "identified", "replace"];

/// Text of `sql`, its secrets replaced by `'***'`.
pub fn masked(sql: &str) -> Cow<'_, str> {
    let lower = sql.to_ascii_lowercase();
    if !HINTS.iter().any(|hint| lower.contains(hint)) {
        return Cow::Borrowed(sql);
    }
    let mut masker = Masker::default();
    let bytes = sql.as_bytes();
    let mut text = String::with_capacity(sql.len());
    let mut at = 0;
    while at < bytes.len() {
        let end = token_end(bytes, at);
        let token = &sql[at..end];
        match bytes[at] {
            quote @ (b'\'' | b'"') => {
                if masker.is_secret() {
                    let quote = char::from(quote);
                    text.push(quote);
                    text.push_str(MASK);
                    text.push(quote);
                } else {
                    text.push_str(token);
                }
                masker.push("'".to_string());
            }
            byte if byte.is_ascii_whitespace() || is_comment(bytes, at) => text.push_str(token),
            b'`' => {
                text.push_str(token);
                masker.push(token.trim_matches('`').to_ascii_lowercase());
            }
            _ => {
                text.push_str(token);
                masker.push(token.to_ascii_lowercase());
            }
        }
        at = end;
    }
    Cow::Owned(text)
}

/// Last tokens read, and what the statement already said.
#[derive(Default)]
struct Masker {
    /// Last significant tokens, lowercase, the most recent last; a string
    /// is written `'`.
    recent: Vec<String>,
    /// Number of significant tokens of the statement.
    count: usize,
    /// Has the statement said `IDENTIFIED`?
    identified: bool,
    /// Does the statement start with `SET PASSWORD`?
    set_password: bool,
}

impl Masker {
    /// Records a significant token; `;` starts a new statement.
    fn push(&mut self, token: String) {
        if token == ";" {
            *self = Self::default();
            return;
        }
        self.identified |= token == "identified";
        if self.count == 1 && token == "password" && self.recent[0] == "set" {
            self.set_password = true;
        }
        self.count += 1;
        if self.recent.len() == 3 {
            self.recent.remove(0);
        }
        self.recent.push(token);
    }

    /// Token `back` places before the next one (0: the last one), empty
    /// if there is none.
    fn before(&self, back: usize) -> &str {
        self.recent
            .len()
            .checked_sub(back + 1)
            .map_or("", |index| self.recent[index].as_str())
    }

    /// Is a string read now a secret?
    fn is_secret(&self) -> bool {
        let (last, previous, third) = (self.before(0), self.before(1), self.before(2));
        let password_word = |word: &str| word.ends_with("password");
        (self.identified
            && (matches!((previous, last), ("identified", "by"))
                || matches!((third, previous, last), ("identified", "by", "password"))
                || (third == "with" && matches!(last, "by" | "as"))
                || (matches!(third, "via" | "or") && matches!(last, "using" | "as"))))
            || (last == "(" && previous == "password")
            || password_word(last)
            || (last == "=" && password_word(previous))
            || (self.set_password && last == "=")
            || ((self.identified || self.set_password) && last == "replace")
    }
}

/// End of the token that starts at `at`: a string, a quoted identifier, a
/// comment, a run of blanks, a word, or a single character.
fn token_end(bytes: &[u8], at: usize) -> usize {
    match bytes[at] {
        quote @ (b'\'' | b'"' | b'`') => quoted_end(bytes, at, quote),
        b'-' | b'#' | b'/' if is_comment(bytes, at) => comment_end(bytes, at),
        byte if byte.is_ascii_whitespace() => run_end(bytes, at, |b| b.is_ascii_whitespace()),
        byte if is_word(byte) => run_end(bytes, at, is_word),
        // An ASCII character: the bytes of the others belong to words.
        _ => at + 1,
    }
}

/// End of a string or quoted identifier opened at `at` by `quote`: a doubled
/// quote, or, in a string, a quote after a backslash, does not close it.
fn quoted_end(bytes: &[u8], at: usize, quote: u8) -> usize {
    let mut index = at + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if quote != b'`' => index += 2,
            byte if byte == quote => {
                if bytes.get(index + 1) == Some(&quote) {
                    index += 2;
                } else {
                    return index + 1;
                }
            }
            _ => index += 1,
        }
    }
    bytes.len()
}

/// Does a comment start at `at`: `-- `, `#` or `/*`?
fn is_comment(bytes: &[u8], at: usize) -> bool {
    match bytes[at] {
        b'#' => true,
        b'-' => {
            bytes.get(at + 1) == Some(&b'-')
                && bytes
                    .get(at + 2)
                    .is_none_or(|byte| byte.is_ascii_whitespace())
        }
        b'/' => bytes.get(at + 1) == Some(&b'*'),
        _ => false,
    }
}

/// End of the comment that starts at `at`.
fn comment_end(bytes: &[u8], at: usize) -> usize {
    if bytes[at] == b'/' {
        bytes[at + 2..]
            .windows(2)
            .position(|pair| pair == b"*/")
            .map_or(bytes.len(), |position| at + 2 + position + 2)
    } else {
        bytes[at..]
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(bytes.len(), |position| at + position)
    }
}

/// End of the run of bytes that `belongs` accepts, from `at`.
fn run_end(bytes: &[u8], at: usize, belongs: impl Fn(u8) -> bool) -> usize {
    bytes[at..]
        .iter()
        .position(|&byte| !belongs(byte))
        .map_or(bytes.len(), |position| at + position)
}

/// Can the byte belong to a word? The bytes of a multi-byte UTF-8
/// character (≥ 0x80) do: a name can hold accented letters.
fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Masked text of `sql`.
    fn mask(sql: &str) -> String {
        masked(sql).into_owned()
    }

    #[test]
    fn masks_the_passwords_of_the_accounts() {
        assert_eq!(
            mask("CREATE USER 'bob'@'%' IDENTIFIED BY 'S3cr\\'et'"),
            "CREATE USER 'bob'@'%' IDENTIFIED BY '***'"
        );
        assert_eq!(
            mask("alter user bob identified by \"pw\" replace 'old'"),
            "alter user bob identified by \"***\" replace '***'"
        );
        assert_eq!(
            mask("CREATE USER bob IDENTIFIED BY PASSWORD '*A4B6'"),
            "CREATE USER bob IDENTIFIED BY PASSWORD '***'"
        );
        assert_eq!(
            mask("CREATE USER bob IDENTIFIED WITH caching_sha2_password BY 'pw'"),
            "CREATE USER bob IDENTIFIED WITH caching_sha2_password BY '***'"
        );
        assert_eq!(
            mask("CREATE USER bob IDENTIFIED VIA ed25519 USING PASSWORD('pw') OR unix_socket"),
            "CREATE USER bob IDENTIFIED VIA ed25519 USING PASSWORD('***') OR unix_socket"
        );
        assert_eq!(
            mask(
                "CREATE USER bob IDENTIFIED VIA mysql_native_password USING '*A4' OR ed25519 AS 'x'"
            ),
            "CREATE USER bob IDENTIFIED VIA mysql_native_password USING '***' OR ed25519 AS '***'"
        );
        assert_eq!(
            mask("GRANT ALL ON *.* TO bob IDENTIFIED BY 'pw'"),
            "GRANT ALL ON *.* TO bob IDENTIFIED BY '***'"
        );
    }

    #[test]
    fn masks_set_password_and_the_replication_passwords() {
        assert_eq!(
            mask("SET PASSWORD FOR 'bob'@'%' = 'pw'"),
            "SET PASSWORD FOR 'bob'@'%' = '***'"
        );
        assert_eq!(
            mask("SET PASSWORD = PASSWORD('pw')"),
            "SET PASSWORD = PASSWORD('***')"
        );
        assert_eq!(
            mask("CHANGE MASTER TO MASTER_USER='r', MASTER_PASSWORD='pw'"),
            "CHANGE MASTER TO MASTER_USER='r', MASTER_PASSWORD='***'"
        );
        assert_eq!(
            mask("CREATE SERVER s FOREIGN DATA WRAPPER mysql OPTIONS (USER 'r', PASSWORD 'pw')"),
            "CREATE SERVER s FOREIGN DATA WRAPPER mysql OPTIONS (USER 'r', PASSWORD '***')"
        );
        assert_eq!(
            mask("SELECT id FROM `users` WHERE `password` = 'pw'"),
            "SELECT id FROM `users` WHERE `password` = '***'"
        );
    }

    #[test]
    fn keeps_the_other_strings_and_the_comments() {
        let kept = [
            "SELECT 'password', REPLACE(name, 'a', 'b') FROM t ORDER BY 'x'",
            "SELECT a OR b AS 'identified' FROM t",
            "SELECT 'IDENTIFIED BY' AS x -- IDENTIFIED BY 'not parsed'",
            "SELECT 1; SELECT 'é' AS replace_me",
            "/* identified by 'c' */ SELECT 'x'",
        ];
        for sql in kept {
            assert_eq!(mask(sql), sql);
        }
        assert!(matches!(masked("SELECT * FROM video"), Cow::Borrowed(_)));
    }

    #[test]
    fn starts_again_at_each_statement() {
        assert_eq!(
            mask("CREATE USER a IDENTIFIED BY 'p'; SELECT a OR b AS 'c'"),
            "CREATE USER a IDENTIFIED BY '***'; SELECT a OR b AS 'c'"
        );
    }
}
