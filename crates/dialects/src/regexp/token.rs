//! Tokens of a pattern, in the syntax that ICU and PCRE2 share
//! (`crates/probe/cases/regexp-features.txt`, `regexp-levers.txt`): a
//! construct that one library reads otherwise, or not at all, is refused
//! with its name. The writer (`write` module) then rewrites the tokens that
//! both libraries read with different meanings.

use std::ops::Range;

use super::Library;

/// Highest count of a quantifier that both libraries take (65535 in PCRE2).
const MAX_COUNT: u32 = 65_535;

/// General categories of Unicode that `\p{...}` names in both libraries;
/// `Cn`, the unassigned code points, depends on the version of Unicode of
/// each library, refused.
const CATEGORIES: [&str; 35] = [
    "L", "Lu", "Ll", "Lt", "Lm", "Lo", "M", "Mn", "Mc", "Me", "N", "Nd", "Nl", "No", "P", "Pc",
    "Pd", "Ps", "Pe", "Pi", "Pf", "Po", "S", "Sm", "Sc", "Sk", "So", "Z", "Zs", "Zl", "Zp", "C",
    "Cc", "Cf", "Co",
];

/// Classes `[:name:]` read alike, or rewritten (`space`, `blank`).
const POSIX: [&str; 4] = ["digit", "cntrl", "space", "blank"];

/// Token of a pattern, with its position in the pattern, in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    /// What the token is.
    pub kind: Kind,
    /// Position of its text in the pattern.
    pub span: Range<usize>,
}

/// Kind of token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Kind {
    /// Literal character: written as is, escaped (`\.`, `\x{41}`, `\t`),
    /// or between `\Q` and `\E` (`quoted`), whose text is not that of the
    /// character alone.
    Char {
        /// The character.
        value: char,
        /// `true` between `\Q` and `\E`.
        quoted: bool,
    },
    /// `.`.
    Dot,
    /// `^`.
    Start,
    /// `$`.
    End,
    /// `\Z`: the end, or before a line terminator at the end.
    EndZ,
    /// `\A`, `\z` or `\G`, read alike.
    Anchor,
    /// `\d`, `\D`, `\s`, `\S`, `\h`, `\H`, `\v`, `\V`, `\R`, `\w` or `\W`,
    /// by its letter.
    Shorthand(char),
    /// `\p{X}`, or `\P{X}` when `negated`, of a general category.
    Property {
        /// `true` for `\P`.
        negated: bool,
        /// Name of the category: `L`, `Nd`...
        name: String,
    },
    /// Class between brackets.
    Set(Set),
    /// Quantifier: `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`, lazy (`?` after
    /// it) or possessive (`+` after it).
    Quantifier {
        /// Count of `{n}`, the only quantifier of a fixed length.
        count: Option<u32>,
    },
    /// Opening parenthesis of a group.
    Open(Group),
    /// Flags set until the end of the group: `(?i)`, `(?-s)`...
    Flags(Flags),
    /// `)`.
    Close,
    /// `|`.
    Alternation,
    /// `\1` to `\9`, or `\k<name>`.
    Backreference,
    /// `(?#...)`.
    Comment,
}

/// Kind of group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Group {
    /// `(...)`.
    Capture,
    /// `(?<name>...)`.
    Named,
    /// `(?:...)`.
    NonCapture,
    /// `(?i:...)`, of flags set within the group.
    Flagged(Flags),
    /// `(?>...)`.
    Atomic,
    /// `(?=...)` or `(?!...)`.
    Lookahead,
    /// `(?<=...)` or `(?<!...)`.
    Lookbehind,
}

/// Flags of a group or of the rest of a group: case insensitivity (`i`),
/// line terminators matched by `.` (`s`), `^` and `$` at each line
/// (`m`); `None` for a flag left as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Flags {
    /// `i`.
    pub insensitive: Option<bool>,
    /// `s`.
    pub dot_all: Option<bool>,
    /// `m`.
    pub multiline: Option<bool>,
}

/// Class between brackets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Set {
    /// `true` for `[^...]`.
    pub negated: bool,
    /// Members, in order.
    pub items: Vec<Item>,
}

/// Member of a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    /// A character.
    Char(char),
    /// A range of characters, its bounds included.
    Range(char, char),
    /// `\d`, `\s`, `\w`... by its letter.
    Shorthand(char),
    /// `\p{X}` or `\P{X}`.
    Property {
        /// `true` for `\P`.
        negated: bool,
        /// Name of the category.
        name: String,
    },
    /// `[:name:]`.
    Posix(String),
}

/// Reads `pattern`, as `library` reads it, into tokens; the message of the
/// construct refused otherwise.
pub(super) fn tokens(pattern: &str, library: Library) -> Result<Vec<Token>, String> {
    let mut lexer = Lexer {
        pattern,
        library,
        position: 0,
        tokens: Vec::new(),
        depth: 0,
    };
    while lexer.position < pattern.len() {
        lexer.token()?;
    }
    if lexer.depth > 0 {
        return Err("a group without its closing parenthesis".to_string());
    }
    Ok(lexer.tokens)
}

/// State of the reading.
struct Lexer<'a> {
    /// The pattern.
    pattern: &'a str,
    /// Library that reads it.
    library: Library,
    /// Position of the next character, in bytes.
    position: usize,
    /// Tokens read.
    tokens: Vec<Token>,
    /// Groups opened and not closed.
    depth: usize,
}

impl Lexer<'_> {
    /// Next character, without reading it.
    fn peek(&self) -> Option<char> {
        self.pattern[self.position..].chars().next()
    }

    /// Character `offset` characters after the next one.
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.pattern[self.position..].chars().nth(offset)
    }

    /// Reads the next character.
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += c.len_utf8();
        Some(c)
    }

    /// Reads `text` when it comes next.
    fn eat(&mut self, text: &str) -> bool {
        if self.pattern[self.position..].starts_with(text) {
            self.position += text.len();
            true
        } else {
            false
        }
    }

    /// Records a token from `start` to the current position.
    fn push(&mut self, kind: Kind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: start..self.position,
        });
    }

    /// Reads the next token.
    fn token(&mut self) -> Result<(), String> {
        let start = self.position;
        let Some(c) = self.bump() else {
            return Ok(());
        };
        match c {
            '\\' => self.escape(start),
            '.' => {
                self.push(Kind::Dot, start);
                Ok(())
            }
            '^' => {
                self.push(Kind::Start, start);
                Ok(())
            }
            '$' => {
                self.push(Kind::End, start);
                Ok(())
            }
            '|' => {
                self.push(Kind::Alternation, start);
                Ok(())
            }
            '(' => self.group(start),
            ')' => {
                if self.depth == 0 {
                    return Err("a closing parenthesis without its group".to_string());
                }
                self.depth -= 1;
                self.push(Kind::Close, start);
                Ok(())
            }
            '[' => self.set(start),
            '*' | '+' | '?' => self.quantifier(start, None),
            '{' => self.brace(start),
            '}' => Err(
                "a `}` outside a quantifier, which ICU refuses, PCRE2 reads as itself".to_string(),
            ),
            _ => {
                self.push(
                    Kind::Char {
                        value: c,
                        quoted: false,
                    },
                    start,
                );
                Ok(())
            }
        }
    }

    /// Escape after `\`, from `start`.
    fn escape(&mut self, start: usize) -> Result<(), String> {
        let Some(c) = self.bump() else {
            return Err("a `\\` at the end of the pattern".to_string());
        };
        let kind = match c {
            'Q' => return self.quoted(),
            'A' | 'z' | 'G' => Kind::Anchor,
            'Z' => Kind::EndZ,
            'd' | 'D' | 's' | 'S' | 'h' | 'H' | 'v' | 'V' | 'R' | 'w' | 'W' => Kind::Shorthand(c),
            'p' | 'P' => {
                let name = self.property_name()?;
                Kind::Property {
                    negated: c == 'P',
                    name,
                }
            }
            'k' => {
                self.named_reference()?;
                Kind::Backreference
            }
            '1'..='9' => {
                if self.peek().is_some_and(|next| next.is_ascii_digit()) {
                    return Err(format!(
                        "`\\{c}` followed by a digit, which ICU and PCRE2 read otherwise (a group of several digits or an octal number)"
                    ));
                }
                Kind::Backreference
            }
            'b' | 'B' => {
                return Err(format!(
                    "`\\{c}`: ICU and PCRE2 tell the word characters otherwise, ICU skips the combining marks: not supported yet by Prisme"
                ));
            }
            'X' => {
                return Err(
                    "`\\X`: ICU and PCRE2 cut the graphemes after their own version of Unicode"
                        .to_string(),
                );
            }
            _ => Kind::Char {
                value: self.escaped_char(c)?,
                quoted: false,
            },
        };
        self.push(kind, start);
        Ok(())
    }

    /// Character of an escape of a character, after `\` and `c`.
    fn escaped_char(&mut self, c: char) -> Result<char, String> {
        match c {
            't' => Ok('\t'),
            'n' => Ok('\n'),
            'r' => Ok('\r'),
            'f' => Ok('\u{c}'),
            'a' => Ok('\u{7}'),
            'e' => Ok('\u{1b}'),
            'c' => match self.bump() {
                Some(letter @ 'A'..='Z') => Ok(char::from(letter as u8 ^ 0x40)),
                _ => Err("`\\c` followed by another character than a capital letter".to_string()),
            },
            'x' => self.hex_char(),
            'u' if self.library == Library::Icu => self.fixed_hex(4),
            'U' if self.library == Library::Icu => self.fixed_hex(8),
            _ if c.is_ascii_alphanumeric() => Err(format!(
                "`\\{c}`, which ICU and PCRE2 read otherwise, or which one of them refuses"
            )),
            _ if c.is_control() => Err("a `\\` before a control character".to_string()),
            // Any other character escaped is itself (measured on the ASCII
            // punctuation, the space and `é`).
            _ => Ok(c),
        }
    }

    /// Character of `\x{h...}` or `\xhh`, after `\x`.
    fn hex_char(&mut self) -> Result<char, String> {
        let digits = if self.eat("{") {
            let start = self.position;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.bump();
            }
            let digits = &self.pattern[start..self.position];
            if !self.eat("}") || digits.is_empty() {
                return Err("`\\x{` without its hexadecimal digits and `}`".to_string());
            }
            digits
        } else {
            let start = self.position;
            for _ in 0..2 {
                if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                    return Err(
                        "`\\x` without two hexadecimal digits, which ICU and PCRE2 read otherwise"
                            .to_string(),
                    );
                }
                self.bump();
            }
            &self.pattern[start..self.position]
        };
        code_point(digits)
    }

    /// Character of exactly `count` hexadecimal digits: `\uhhhh` and
    /// `\Uhhhhhhhh` of ICU.
    fn fixed_hex(&mut self, count: usize) -> Result<char, String> {
        let start = self.position;
        for _ in 0..count {
            if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                return Err(format!(
                    "`\\u` or `\\U` without its {count} hexadecimal digits"
                ));
            }
            self.bump();
        }
        code_point(&self.pattern[start..self.position])
    }

    /// Characters between `\Q` and `\E`, or the end of the pattern.
    fn quoted(&mut self) -> Result<(), String> {
        loop {
            let start = self.position;
            if self.eat("\\E") {
                return Ok(());
            }
            let Some(value) = self.bump() else {
                return Ok(());
            };
            self.push(
                Kind::Char {
                    value,
                    quoted: true,
                },
                start,
            );
        }
    }

    /// Name of a property, between braces, after `\p` or `\P`.
    fn property_name(&mut self) -> Result<String, String> {
        if !self.eat("{") {
            return Err("`\\p` or `\\P` without braces, which ICU refuses (`\\pL`)".to_string());
        }
        let start = self.position;
        while self.peek().is_some_and(|c| c != '}') {
            self.bump();
        }
        let name = self.pattern[start..self.position].to_string();
        if !self.eat("}") {
            return Err("`\\p{` without its `}`".to_string());
        }
        if !CATEGORIES.contains(&name.as_str()) {
            return Err(format!(
                "`\\p{{{name}}}`: Prisme translates the general categories of Unicode only (`\\p{{L}}`, `\\p{{Nd}}`...), which ICU and PCRE2 name alike"
            ));
        }
        Ok(name)
    }

    /// `<name>` of `\k<name>`, after `\k`.
    fn named_reference(&mut self) -> Result<(), String> {
        if !self.eat("<") {
            return Err("`\\k` without `<name>`, which ICU refuses (`\\k{name}`)".to_string());
        }
        self.group_name()
    }

    /// Name of a group and its `>`.
    fn group_name(&mut self) -> Result<(), String> {
        let start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
            self.bump();
        }
        let name = &self.pattern[start..self.position];
        let valid = name
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && name.len() <= 32;
        if !valid || !self.eat(">") {
            return Err(
                "a name of group other than ASCII letters and digits, a letter first".to_string(),
            );
        }
        Ok(())
    }

    /// Group after `(`, from `start`.
    fn group(&mut self, start: usize) -> Result<(), String> {
        if !self.eat("?") {
            if self.peek() == Some('*') {
                return Err("a verb of PCRE2 (`(*...)`), which ICU refuses".to_string());
            }
            return self.open(Group::Capture, start);
        }
        let group = match self.peek() {
            Some(':') => Group::NonCapture,
            Some('>') => Group::Atomic,
            Some('=' | '!') => Group::Lookahead,
            Some('#') => return self.comment(start),
            Some('<') => match self.peek_at(1) {
                Some('=' | '!') => {
                    self.bump();
                    Group::Lookbehind
                }
                _ => {
                    self.bump();
                    self.group_name()?;
                    return self.open(Group::Named, start);
                }
            },
            _ => return self.flags(start),
        };
        self.bump();
        self.open(group, start)
    }

    /// Records the opening of `group`, from `start`.
    fn open(&mut self, group: Group, start: usize) -> Result<(), String> {
        self.depth += 1;
        self.push(Kind::Open(group), start);
        Ok(())
    }

    /// `(?#...)`, after `(?`.
    fn comment(&mut self, start: usize) -> Result<(), String> {
        while let Some(c) = self.bump() {
            if c == ')' {
                self.push(Kind::Comment, start);
                return Ok(());
            }
        }
        Err("a comment `(?#` without its closing parenthesis".to_string())
    }

    /// `(?flags)` or `(?flags:`, after `(?`.
    fn flags(&mut self, start: usize) -> Result<(), String> {
        let mut flags = Flags::default();
        let mut value = true;
        let mut any = false;
        loop {
            let Some(c) = self.bump() else {
                return Err("`(?` without its closing parenthesis".to_string());
            };
            let flag = match c {
                'i' => &mut flags.insensitive,
                's' => &mut flags.dot_all,
                'm' => &mut flags.multiline,
                '-' if value => {
                    value = false;
                    continue;
                }
                ')' | ':' if any => {
                    if c == ')' {
                        self.push(Kind::Flags(flags), start);
                        return Ok(());
                    }
                    return self.open(Group::Flagged(flags), start);
                }
                'x' => {
                    return Err(
                        "the flag `x`: ICU and PCRE2 read the spaces of the classes otherwise"
                            .to_string(),
                    );
                }
                _ => {
                    return Err(format!(
                        "the group `(?{c}`, which ICU and PCRE2 read otherwise, or which one of them refuses"
                    ));
                }
            };
            *flag = Some(value);
            any = true;
        }
    }

    /// Quantifier, from `start`, of `count` when fixed; the lazy `?` or the
    /// possessive `+` after it.
    fn quantifier(&mut self, start: usize, count: Option<u32>) -> Result<(), String> {
        let quantifiable = self.tokens.last().is_some_and(|token| match token.kind {
            Kind::Char { .. }
            | Kind::Dot
            | Kind::Shorthand(_)
            | Kind::Property { .. }
            | Kind::Set(_)
            | Kind::Backreference => true,
            Kind::Close => !self.closes_assertion(),
            _ => false,
        });
        if !quantifiable {
            return Err(
                "a quantifier after an assertion, an anchor, a quantifier or nothing, which ICU and PCRE2 read otherwise"
                    .to_string(),
            );
        }
        if !self.eat("?") {
            self.eat("+");
        }
        self.push(Kind::Quantifier { count }, start);
        Ok(())
    }

    /// Tells whether the last `)` read closes a lookahead or a lookbehind.
    fn closes_assertion(&self) -> bool {
        let mut depth = 0usize;
        for token in self.tokens.iter().rev() {
            match token.kind {
                Kind::Close => depth += 1,
                Kind::Open(group) => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(group, Group::Lookahead | Group::Lookbehind);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// `{n}`, `{n,}` or `{n,m}`, after `{`, from `start`.
    fn brace(&mut self, start: usize) -> Result<(), String> {
        let refused = || {
            Err(
                "a `{` that opens no quantifier `{n}`, `{n,}` or `{n,m}`: ICU refuses it, PCRE2 reads it after its version"
                    .to_string(),
            )
        };
        let Some(low) = self.count() else {
            return refused();
        };
        let (count, high) = if self.eat(",") {
            (None, self.count())
        } else {
            (Some(low), Some(low))
        };
        if !self.eat("}") {
            return refused();
        }
        if high.is_some_and(|high| high < low) || low > MAX_COUNT || high > Some(MAX_COUNT) {
            return Err(format!(
                "a quantifier of counts in the wrong order or above {MAX_COUNT}"
            ));
        }
        self.quantifier(start, count)
    }

    /// Decimal count of a quantifier.
    fn count(&mut self) -> Option<u32> {
        let start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        self.pattern[start..self.position].parse().ok()
    }

    /// Class after `[`, from `start`.
    fn set(&mut self, start: usize) -> Result<(), String> {
        let negated = self.eat("^");
        let mut items: Vec<Item> = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.bump() else {
                return Err("a class without its `]`".to_string());
            };
            let item = match c {
                ']' if !first => break,
                '[' if self.eat(":") => Item::Posix(self.posix()?),
                '[' => {
                    return Err(
                        "a `[` in a class, which ICU reads as a nested class, PCRE2 as itself"
                            .to_string(),
                    );
                }
                '&' if self.peek() == Some('&') => {
                    return Err("`&&` in a class, the intersection of ICU".to_string());
                }
                '-' if !first && self.peek() != Some(']') => {
                    return Err(
                        "a `-` in a class that is neither first, last, nor between two characters"
                            .to_string(),
                    );
                }
                '\\' => self.set_escape()?,
                _ => Item::Char(c),
            };
            first = false;
            // A range: a character, `-`, then a character other than `]`.
            if let Item::Char(low) = item
                && self.peek() == Some('-')
                && self.peek_at(1).is_some_and(|next| next != ']')
            {
                self.bump();
                let high = match self.bump() {
                    Some('\\') => self.set_escape()?,
                    Some('[') => {
                        return Err("a range of a class up to a `[`".to_string());
                    }
                    Some(high) => Item::Char(high),
                    None => return Err("a class without its `]`".to_string()),
                };
                let Item::Char(high) = high else {
                    return Err("a range of a class up to a class".to_string());
                };
                if high < low {
                    return Err("a range of a class in the wrong order".to_string());
                }
                items.push(Item::Range(low, high));
                if self.peek() == Some('-') && self.peek_at(1) != Some(']') {
                    return Err("a `-` in a class after a range".to_string());
                }
                continue;
            }
            items.push(item);
        }
        self.push(Kind::Set(Set { negated, items }), start);
        Ok(())
    }

    /// Name of `[:name:]`, after `[:`.
    fn posix(&mut self) -> Result<String, String> {
        let start = self.position;
        while self.peek().is_some_and(|c| c.is_ascii_lowercase()) {
            self.bump();
        }
        let name = self.pattern[start..self.position].to_string();
        if !self.eat(":]") {
            return Err("`[:` without its name and `:]`".to_string());
        }
        if !POSIX.contains(&name.as_str()) {
            return Err(format!(
                "the class `[:{name}:]`, which ICU and PCRE2 read otherwise: Prisme translates `[:digit:]`, `[:cntrl:]`, `[:space:]` and `[:blank:]`"
            ));
        }
        Ok(name)
    }

    /// Member of a class after `\`.
    fn set_escape(&mut self) -> Result<Item, String> {
        let Some(c) = self.bump() else {
            return Err("a class without its `]`".to_string());
        };
        match c {
            'd' | 'D' | 's' | 'S' | 'h' | 'H' | 'v' | 'V' | 'w' | 'W' => Ok(Item::Shorthand(c)),
            'p' | 'P' => Ok(Item::Property {
                negated: c == 'P',
                name: self.property_name()?,
            }),
            'b' => Err("`\\b` in a class, which ICU and PCRE2 read otherwise".to_string()),
            'Q' | 'E' | 'R' | 'X' | 'N' => Err(format!("`\\{c}` in a class")),
            _ => Ok(Item::Char(self.escaped_char(c)?)),
        }
    }
}

/// Character of hexadecimal `digits`.
fn code_point(digits: &str) -> Result<char, String> {
    u32::from_str_radix(digits, 16)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| format!("`{digits}`, which is not the number of a character"))
}
