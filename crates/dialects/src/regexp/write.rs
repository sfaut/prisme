//! Writing of a pattern for the other library, token by token: a token read
//! alike is copied, the others are rewritten into a form that the output
//! reads as the input does (`crates/probe/cases/regexp-translation.txt`,
//! `regexp-syntax.txt`, `regexp-classes.sql`, the classes compared over all
//! the code points).
//!
//! - Line terminators: ICU ends a line at CR, LF, CR LF, VT, FF, NEL, LS and
//!   PS, PCRE2 at LF. Towards PCRE2, `(*ANY)` at the start makes `.`, `$`,
//!   `\Z` and `^` of `(?m)` read them as ICU does; the `.` of `(?s)` reads
//!   CR LF as one character in ICU, written `(?>\r\n|.)`. Towards ICU,
//!   `(?d)` at the start keeps LF alone; `\Z`, which `(?d)` leaves as is, is
//!   written `(?=\n?\z)`, the `.` of `(?s)` `[\s\S]`.
//! - Classes: PCRE2 counts U+180E (MONGOLIAN VOWEL SEPARATOR) among the
//!   spaces of `\s`, `\h`, `[:space:]` and `[:blank:]`, ICU not: written
//!   with or without it. `\w` of ICU is written as its class towards PCRE2;
//!   `\w` of PCRE2 follows its version, refused. A class of a complemented
//!   member and others is written by an alternation or a lookahead.
//! - Categories of Unicode: ICU closes `L`, `Lu`, `Ll`, `Lt`, `M` and `Mn`
//!   over the case of their characters, PCRE2 otherwise, refused under case
//!   insensitivity.
//! - Literal characters: by their case folding (`fold` module). A run of
//!   ICU is written towards PCRE2 with an alternative per path of each piece
//!   that a folding of several characters crosses: `ß` as `(?:ss|[ßẞ])`.
//!   With a collation of unknown case, the pattern keeps the run as written,
//!   and each other alternative carries a lookahead that only case
//!   insensitivity matches, `(?=S)ss` or `(?=ß)ẞ`, else a lookaround on an
//!   ASCII letter next to it (`te(?:st|(?<=E)[ﬅﬆ])`): a path without such a
//!   character is refused. Towards ICU, a character that folds into several
//!   ones is written alone in a class (`[ß]`), out of case insensitivity for
//!   a pair of `APART` (`(?-i:ﬅ)`), and `(?:)` cuts a run where a folding of
//!   several characters crosses it: `s(?:)s`.
//!   A backreference under case insensitivity is refused: ICU compares it by
//!   the full folding (`(ß)\1` matches `ßss`).
//! - Lookbehinds: PCRE2 reads one of a fixed length only, a branch after
//!   another; ICU one of a bounded length. One of a variable length, or that
//!   an emulation would make so, is refused towards PCRE2.

use std::borrow::Cow;

use super::fold::{self, Multi, Step};
use super::token::{Flags, Group, Item, Kind, Set, Token};
use super::{Case, Library, Translation};

/// Content of a class, in PCRE2, of the characters of `\w` in ICU: letters,
/// marks, decimal digits, connectors, the joiners, the circled and squared
/// Latin letters.
const ICU_WORD: &str = r"\p{L}\p{Nl}\p{M}\p{Nd}\p{Pc}\x{200C}\x{200D}\x{24B6}-\x{24E9}\x{1F130}-\x{1F149}\x{1F150}-\x{1F169}\x{1F170}-\x{1F189}";

/// Content of a class, in PCRE2, of the characters of `\s` and `[:space:]`
/// in ICU.
const ICU_SPACE: &str = r"\x{9}-\x{D}\x{20}\x{85}\x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}";

/// Content of a class, in PCRE2, of the characters of `\h` and `[:blank:]`
/// in ICU.
const ICU_BLANK: &str = r"\x{9}\x{20}\x{A0}\x{1680}\x{2000}-\x{200A}\x{202F}\x{205F}\x{3000}";

/// General categories that ICU closes over the case of their characters
/// under case insensitivity, PCRE2 otherwise.
const CASED_CATEGORIES: [&str; 6] = ["L", "Lu", "Ll", "Lt", "M", "Mn"];

/// ASCII punctuation special in a pattern or a class of one of the
/// libraries, escaped by a backslash.
const SPECIAL: &str = r"\^$.|?*+()[]{}-&#";

/// Most alternatives of a piece of a run of literal characters.
const MAX_PATHS: usize = 32;

/// Writes `tokens`, read from `pattern` by the other library, for `to`, the
/// case of the comparison given by `case` out of the flags.
pub(super) fn write(
    pattern: &str,
    tokens: &[Token],
    to: Library,
    case: Case,
) -> Result<Translation, String> {
    let mut writer = Writer {
        pattern,
        tokens,
        to,
        case,
        out: String::with_capacity(pattern.len()),
        emulations: Vec::new(),
        state: State::default(),
        groups: Vec::new(),
        lines: false,
    };
    let mut index = 0;
    while index < tokens.len() {
        index = writer.token(index)?;
    }
    Ok(writer.finish())
}

/// Flags in force.
#[derive(Debug, Clone, Copy, Default)]
struct State {
    /// Case insensitivity set by a flag, the collation deciding otherwise.
    insensitive: Option<bool>,
    /// `s`: `.` matches the line terminators.
    dot_all: bool,
    /// `m`: `^` and `$` at each line.
    multiline: bool,
}

impl State {
    /// Sets `flags`.
    fn apply(&mut self, flags: Flags) {
        if let Some(insensitive) = flags.insensitive {
            self.insensitive = Some(insensitive);
        }
        if let Some(dot_all) = flags.dot_all {
            self.dot_all = dot_all;
        }
        if let Some(multiline) = flags.multiline {
            self.multiline = multiline;
        }
    }
}

/// Member of a class written for the output.
enum Member {
    /// As written.
    Same,
    /// Members of a class, written in its place.
    Positive(&'static str),
    /// The complement of the members of a class.
    Complement(&'static str),
}

/// State of the writing.
struct Writer<'a> {
    /// The pattern.
    pattern: &'a str,
    /// Its tokens.
    tokens: &'a [Token],
    /// Library of the output.
    to: Library,
    /// Case of the comparison out of the flags, after the collation.
    case: Case,
    /// Pattern written.
    out: String,
    /// Emulations, once each.
    emulations: Vec<String>,
    /// Flags in force.
    state: State,
    /// Groups opened, with the flags in force before each one.
    groups: Vec<(Group, State)>,
    /// `true` when a token reads the line terminators.
    lines: bool,
}

impl<'a> Writer<'a> {
    /// Writes the token at `index`; the index of the next one.
    fn token(&mut self, index: usize) -> Result<usize, String> {
        let token = &self.tokens[index];
        match &token.kind {
            Kind::Char { .. } => return self.run(index),
            Kind::Dot => self.dot(token)?,
            Kind::Start => {
                self.lines |= self.state.multiline;
                self.copy(token);
            }
            Kind::End => {
                self.lines = true;
                self.copy(token);
            }
            Kind::EndZ => self.end_z(token),
            Kind::Shorthand(letter) => self.shorthand(*letter, token)?,
            Kind::Property { negated, name } => {
                self.property(*negated, name)?;
                self.copy(token);
            }
            Kind::Set(set) => self.set(set, token)?,
            Kind::Open(group) => self.open(*group, index)?,
            Kind::Flags(flags) => {
                self.state.apply(*flags);
                self.copy(token);
            }
            Kind::Close => {
                if let Some((_, state)) = self.groups.pop() {
                    self.state = state;
                }
                self.copy(token);
            }
            Kind::Backreference => {
                if self.case() != Case::Sensitive {
                    return Err(format!(
                        "a backreference {}: ICU compares it by the full folding of the case (`(ß)\\1` matches `ßss`), PCRE2 character by character",
                        self.case_words()
                    ));
                }
                self.copy(token);
            }
            Kind::Quantifier { .. } | Kind::Anchor | Kind::Alternation | Kind::Comment => {
                self.copy(token);
            }
        }
        Ok(index + 1)
    }

    /// Pattern written, after `(*ANY)` or `(?d)` when a token reads the line
    /// terminators.
    fn finish(mut self) -> Translation {
        if self.lines {
            let (prefix, note) = match self.to {
                Library::Pcre2 => (
                    "(*ANY)",
                    "ICU ends a line at CR, LF, CR LF, VT, FF, NEL, LS and PS, PCRE2 at LF: written after `(*ANY)`",
                ),
                Library::Icu => (
                    "(?d)",
                    "PCRE2 ends a line at LF, ICU also at CR, CR LF, VT, FF, NEL, LS and PS: written after `(?d)`",
                ),
            };
            self.out.insert_str(0, prefix);
            self.emulations.insert(0, note.to_string());
        }
        Translation {
            pattern: self.out,
            emulations: self.emulations,
        }
    }

    /// Case of the comparison in force.
    fn case(&self) -> Case {
        match self.state.insensitive {
            Some(true) => Case::Insensitive,
            Some(false) => Case::Sensitive,
            None => self.case,
        }
    }

    /// Case of the comparison in force, for the messages.
    fn case_words(&self) -> &'static str {
        match self.case() {
            Case::Unknown => "with a collation of unknown case, which may ignore it",
            _ => "under case insensitivity",
        }
    }

    /// Tells whether a lookbehind holds the token written.
    fn in_lookbehind(&self) -> bool {
        self.groups
            .iter()
            .any(|(group, _)| *group == Group::Lookbehind)
    }

    /// Writes `text`.
    fn write(&mut self, text: &str) {
        self.out.push_str(text);
    }

    /// Writes `token` as written.
    fn copy(&mut self, token: &Token) {
        self.out.push_str(&self.pattern[token.span.clone()]);
    }

    /// Records the emulation `note`, once.
    fn note(&mut self, note: String) {
        if !self.emulations.contains(&note) {
            self.emulations.push(note);
        }
    }

    /// Text of the literal character `token`: as written, or rewritten when
    /// quoted (`\Q...\E`) or written by `\u` of ICU.
    fn char_text(&self, token: &Token) -> Cow<'a, str> {
        let text = &self.pattern[token.span.clone()];
        match token.kind {
            Kind::Char { value, quoted }
                if quoted || text.starts_with("\\u") || text.starts_with("\\U") =>
            {
                Cow::Owned(literal(value))
            }
            _ => Cow::Borrowed(text),
        }
    }

    /// `.`.
    fn dot(&mut self, token: &Token) -> Result<(), String> {
        if !self.state.dot_all {
            self.lines = true;
            self.copy(token);
            return Ok(());
        }
        match self.to {
            Library::Pcre2 => {
                if self.in_lookbehind() {
                    return Err(
                        "the `.` of `(?s)` in a lookbehind: ICU reads CR LF as one character, of a length that PCRE2 refuses there"
                            .to_string(),
                    );
                }
                self.write(r"(?>\r\n|.)");
                self.note(
                    "ICU reads CR LF as one character with the `.` of `(?s)`: written `(?>\\r\\n|.)`"
                        .to_string(),
                );
            }
            Library::Icu => {
                self.write(r"[\s\S]");
                self.note(
                    "ICU reads CR LF as one character with the `.` of `(?s)`, even after `(?d)`: written `[\\s\\S]`"
                        .to_string(),
                );
            }
        }
        Ok(())
    }

    /// `\Z`.
    fn end_z(&mut self, token: &Token) {
        match self.to {
            Library::Pcre2 => {
                self.lines = true;
                self.copy(token);
            }
            Library::Icu => {
                self.write(r"(?=\n?\z)");
                self.note(
                    "ICU ends `\\Z` before any line terminator, even after `(?d)`: written `(?=\\n?\\z)`"
                        .to_string(),
                );
            }
        }
    }

    /// `\d`, `\s`, `\w`... of `letter`, alone.
    fn shorthand(&mut self, letter: char, token: &Token) -> Result<(), String> {
        let written: Cow<'static, str> = match (self.to, letter) {
            (Library::Pcre2, 's') => r"[^\S\x{180E}]".into(),
            (Library::Pcre2, 'S') => r"[\S\x{180E}]".into(),
            (Library::Pcre2, 'h') => r"[^\H\x{180E}]".into(),
            (Library::Pcre2, 'H') => r"[\H\x{180E}]".into(),
            (Library::Pcre2, 'w') => format!("[{ICU_WORD}]").into(),
            (Library::Pcre2, 'W') => format!("[^{ICU_WORD}]").into(),
            (Library::Icu, 's') => r"[\s\x{180E}]".into(),
            (Library::Icu, 'S') => r"[^\s\x{180E}]".into(),
            (Library::Icu, 'h') => r"[\h\x{180E}]".into(),
            (Library::Icu, 'H') => r"[^\h\x{180E}]".into(),
            (Library::Icu, 'w' | 'W') => return Err(pcre2_word(letter)),
            _ => {
                self.copy(token);
                return Ok(());
            }
        };
        self.class_note(letter);
        self.write(&written);
        Ok(())
    }

    /// Records the emulation of the members of a class of `letter`, `s`,
    /// `h`, their complements, `w` or `W`.
    fn class_note(&mut self, letter: char) {
        let note = match (letter.to_ascii_lowercase(), self.to) {
            ('w', _) => {
                "ICU reads `\\w` as the letters, marks, decimal digits and connectors, the joiners, the circled and squared Latin letters: written as that class"
            }
            (_, Library::Pcre2) => {
                "PCRE2 counts U+180E (MONGOLIAN VOWEL SEPARATOR) among the spaces of `\\s`, `\\h`, `[:space:]` and `[:blank:]`, ICU not: written without it"
            }
            (_, Library::Icu) => {
                "PCRE2 counts U+180E (MONGOLIAN VOWEL SEPARATOR) among the spaces of `\\s`, `\\h`, `[:space:]` and `[:blank:]`, ICU not: written with it"
            }
        };
        self.note(note.to_string());
    }

    /// Refuses `\p{name}`, or `\P{name}` when `negated`, that the case
    /// insensitivity makes ICU and PCRE2 read otherwise.
    fn property(&self, negated: bool, name: &str) -> Result<(), String> {
        if self.case() == Case::Sensitive || !CASED_CATEGORIES.contains(&name) {
            return Ok(());
        }
        Err(format!(
            "`\\{}{{{name}}}` {}: ICU closes the category over the case of its characters, PCRE2 otherwise",
            if negated { 'P' } else { 'p' },
            self.case_words()
        ))
    }

    /// Class between brackets.
    fn set(&mut self, set: &Set, token: &Token) -> Result<(), String> {
        for item in &set.items {
            if let Item::Property { negated, name } = item {
                self.property(*negated, name)?;
            }
        }
        if self.case() != Case::Sensitive
            && let Some((member, other)) = apart_member(set)
        {
            return Err(format!(
                "a class of `{member}` without `{other}` {}: ICU matches them with each other, PCRE2 not",
                self.case_words()
            ));
        }
        let mut members = String::new();
        let mut complement = None;
        let mut letters = Vec::new();
        for item in &set.items {
            match self.member(item)? {
                Member::Same => members.push_str(&item_text(item)),
                Member::Positive(text) => {
                    members.push_str(text);
                    letters.push(item_letter(item));
                }
                Member::Complement(text) => {
                    if complement.replace(text).is_some() {
                        return Err(
                            "a class of two complemented members, which Prisme cannot write for the other library"
                                .to_string(),
                        );
                    }
                    letters.push(item_letter(item));
                }
            }
        }
        if letters.is_empty() {
            self.copy(token);
            return Ok(());
        }
        for letter in letters {
            self.class_note(letter);
        }
        let written = match (complement, set.negated) {
            (None, false) => format!("[{members}]"),
            (None, true) => format!("[^{members}]"),
            (Some(complement), false) if members.is_empty() => format!("[^{complement}]"),
            (Some(complement), false) => format!("(?:[^{complement}]|[{members}])"),
            (Some(complement), true) if members.is_empty() => format!("[{complement}]"),
            (Some(complement), true) => format!("(?:(?![{members}])[{complement}])"),
        };
        self.write(&written);
        Ok(())
    }

    /// `item` of a class written for the output.
    fn member(&self, item: &Item) -> Result<Member, String> {
        let letter = item_letter(item);
        Ok(match (self.to, letter) {
            (Library::Pcre2, 's') => Member::Positive(ICU_SPACE),
            (Library::Pcre2, 'h') => Member::Positive(ICU_BLANK),
            (Library::Pcre2, 'S') => Member::Positive(r"\S\x{180E}"),
            (Library::Pcre2, 'H') => Member::Positive(r"\H\x{180E}"),
            (Library::Pcre2, 'w') => Member::Positive(ICU_WORD),
            (Library::Pcre2, 'W') => Member::Complement(ICU_WORD),
            (Library::Icu, 's') => Member::Positive(r"\s\x{180E}"),
            (Library::Icu, 'h') => Member::Positive(r"\h\x{180E}"),
            (Library::Icu, 'S') => Member::Complement(r"\s\x{180E}"),
            (Library::Icu, 'H') => Member::Complement(r"\h\x{180E}"),
            (Library::Icu, 'w' | 'W') => return Err(pcre2_word(letter)),
            _ => Member::Same,
        })
    }

    /// Opening of `group`, the token at `index`.
    fn open(&mut self, group: Group, index: usize) -> Result<(), String> {
        if group == Group::Lookbehind && self.to == Library::Pcre2 {
            let mut at = index + 1;
            if branches(self.tokens, &mut at).contains(&None) {
                return Err(
                    "a lookbehind of a variable length, which ICU reads up to a bound, PCRE2 refuses"
                        .to_string(),
                );
            }
        }
        self.groups.push((group, self.state));
        if let Group::Flagged(flags) = group {
            self.state.apply(flags);
        }
        self.copy(&self.tokens[index]);
        Ok(())
    }

    /// Indexes of the characters of the run of literal characters at
    /// `index`: the characters that follow each other, comments between
    /// them, up to the one before a quantifier, a run of its own.
    fn run_chars(&self, index: usize) -> Vec<usize> {
        let mut chars = Vec::new();
        for (at, token) in self.tokens.iter().enumerate().skip(index) {
            match token.kind {
                Kind::Char { .. } => {
                    let quantified = matches!(
                        self.tokens.get(at + 1).map(|next| &next.kind),
                        Some(Kind::Quantifier { .. })
                    );
                    if quantified && !chars.is_empty() {
                        break;
                    }
                    chars.push(at);
                    if quantified {
                        break;
                    }
                }
                Kind::Comment => {}
                _ => break,
            }
        }
        chars
    }

    /// Run of literal characters at `index`; the index after it.
    fn run(&mut self, index: usize) -> Result<usize, String> {
        let chars = self.run_chars(index);
        let end = chars.last().map_or(index + 1, |last| last + 1);
        let case = self.case();
        let written = match (case, self.to) {
            (Case::Sensitive, _) => None,
            (_, Library::Pcre2) => self.run_pcre2(&chars, case)?,
            (_, Library::Icu) => self.run_icu(&chars),
        };
        match written {
            Some((written, note)) => {
                self.write(&written);
                self.note(note);
            }
            None => {
                for at in index..end {
                    let token = &self.tokens[at];
                    let text = self.char_text(token);
                    self.out.push_str(&text);
                }
            }
        }
        Ok(end)
    }

    /// Run of the characters at `chars`, from ICU, written for PCRE2 with its
    /// foldings, and its emulation; `None` without a folding of several
    /// characters.
    fn run_pcre2(&self, chars: &[usize], case: Case) -> Result<Option<(String, String)>, String> {
        let values: Vec<char> = chars
            .iter()
            .map(|&at| char_value(&self.tokens[at]))
            .collect();
        let folded = fold::fold(&values);
        if folded.spans.is_empty() {
            return Ok(None);
        }
        let source = self.texts(chars);
        if self.in_lookbehind() {
            return Err(format!(
                "`{source}` in a lookbehind {}: ICU folds it into strings of other lengths, which PCRE2 refuses there",
                self.case_words()
            ));
        }
        let mut pieces = Vec::new();
        for (start, end) in folded.pieces() {
            let first = folded.bounds.iter().position(|&bound| bound == start);
            let last = folded.bounds.iter().position(|&bound| bound == end);
            let (Some(first), Some(last)) = (first, last) else {
                return Err(format!("`{source}`, whose folding Prisme cannot cut"));
            };
            pieces.push((start, end, &chars[first..last]));
        }
        // An ASCII letter of a piece of its own, next to a piece: a guard of
        // its paths with a collation of unknown case.
        let letter = |index: Option<usize>| {
            let (start, end, originals) = *pieces.get(index?)?;
            let c = char_value(&self.tokens[*originals.first()?]);
            (end - start == 1 && c.is_ascii_alphabetic()).then_some(c)
        };
        let mut written = String::new();
        for (index, &(start, end, originals)) in pieces.iter().enumerate() {
            if end - start == 1 {
                written.push_str(&self.texts(originals));
                continue;
            }
            let paths = folded.paths(start, end, MAX_PATHS).ok_or_else(|| {
                format!(
                    "`{source}` {}: too many foldings to write",
                    self.case_words()
                )
            })?;
            let neighbours = (letter(index.checked_sub(1)), letter(Some(index + 1)));
            written.push_str(&self.piece(originals, &paths, case, neighbours)?);
        }
        let note = format!(
            "ICU compares `{source}` by the full folding of the case (`ss` matches `ß`), PCRE2 character by character: written `{written}`"
        );
        Ok(Some((written, note)))
    }

    /// Piece of a run, of the characters at `originals` and their `paths`,
    /// written for PCRE2; `neighbours`, the ASCII letters of the pieces before
    /// and after it, guard a path without a character of another case.
    fn piece(
        &self,
        originals: &[usize],
        paths: &[Vec<Step>],
        case: Case,
        neighbours: (Option<char>, Option<char>),
    ) -> Result<String, String> {
        let original = self.texts(originals);
        let values: Vec<char> = originals
            .iter()
            .map(|&at| char_value(&self.tokens[at]))
            .collect();
        let single = values.iter().all(|&c| fold::multi(c).is_none());
        let mut alternatives = Vec::new();
        if case == Case::Unknown {
            alternatives.push(original.clone());
        }
        for path in paths {
            let chars_only = path.iter().all(|step| matches!(step, Step::Char(_)));
            if case == Case::Insensitive {
                alternatives.push(if chars_only && single {
                    original.clone()
                } else {
                    path_text(path, None)
                });
                continue;
            }
            if written_as(path, &values) {
                continue;
            }
            let text = match (path.iter().position(|&step| guardable(step)), neighbours) {
                (Some(guard), _) => path_text(path, Some(guard)),
                (None, (Some(before), _)) => {
                    format!("(?<={}){}", other_case(before), path_text(path, None))
                }
                (None, (None, Some(after))) => {
                    format!("{}(?={})", path_text(path, None), other_case(after))
                }
                (None, (None, None)) => {
                    let matched: String = path.iter().map(|&step| step_char(step)).collect();
                    return Err(format!(
                        "`{original}` {}: ICU matches `{matched}` under case insensitivity only, which PCRE2 cannot tell from the case of the characters",
                        self.case_words()
                    ));
                }
            };
            alternatives.push(text);
        }
        Ok(format!("(?:{})", alternatives.join("|")))
    }

    /// Run of the characters at `chars`, from PCRE2, written for ICU: a
    /// character that folds into several ones alone, the others cut where a
    /// folding of several characters crosses them; `None` when written as
    /// is.
    fn run_icu(&self, chars: &[usize]) -> Option<(String, String)> {
        let values: Vec<char> = chars
            .iter()
            .map(|&at| char_value(&self.tokens[at]))
            .collect();
        let mut cuts = vec![false; chars.len()];
        let mut start = 0;
        while start < values.len() {
            let end = (start..values.len())
                .find(|&at| fold::multi(values[at]).is_some())
                .unwrap_or(values.len());
            let folded = fold::fold(&values[start..end]);
            for position in 1..end - start {
                if folded
                    .spans
                    .iter()
                    .any(|&(from, to, _)| from < position && position < to)
                {
                    cuts[start + position] = true;
                }
            }
            start = end + 1;
        }
        let multi = values.iter().any(|&c| fold::multi(c).is_some());
        if !multi && !cuts.contains(&true) {
            return None;
        }
        let mut written = String::new();
        for (k, &at) in chars.iter().enumerate() {
            if cuts[k] {
                written.push_str("(?:)");
            }
            let c = values[k];
            if fold::apart(c) {
                written.push_str(&format!("(?-i:{})", literal(c)));
            } else if fold::multi(c).is_some() {
                written.push_str(&format!("[{}]", literal(c)));
            } else {
                written.push_str(&self.char_text(&self.tokens[at]));
            }
        }
        let note = format!(
            "ICU compares a run of literal characters by the full folding of the case (`ss` matches `ß`), PCRE2 character by character: `{}` written `{written}`",
            self.texts(chars)
        );
        Some((written, note))
    }

    /// Texts of the literal characters at `chars`.
    fn texts(&self, chars: &[usize]) -> String {
        chars
            .iter()
            .map(|&at| self.char_text(&self.tokens[at]))
            .collect()
    }
}

/// Lengths in characters of the branches of the group whose content starts
/// at `index` of `tokens`, up to its closing parenthesis; `None` for a
/// variable one. Moves `index` after it.
fn branches(tokens: &[Token], index: &mut usize) -> Vec<Option<usize>> {
    let mut lengths = Vec::new();
    let mut length = Some(0);
    let mut last = Some(0);
    while let Some(token) = tokens.get(*index) {
        *index += 1;
        let atom = match &token.kind {
            Kind::Close => break,
            Kind::Alternation => {
                lengths.push(length);
                length = Some(0);
                last = Some(0);
                continue;
            }
            Kind::Quantifier { count } => {
                length = match (length, last, count) {
                    (Some(length), Some(last), Some(count)) => {
                        Some(length - last + last * *count as usize)
                    }
                    _ => None,
                };
                continue;
            }
            Kind::Open(group) => {
                let inner = branches(tokens, index);
                match group {
                    Group::Lookahead | Group::Lookbehind => Some(0),
                    _ => same_length(&inner),
                }
            }
            Kind::Shorthand('R') | Kind::Backreference => None,
            Kind::Char { .. }
            | Kind::Dot
            | Kind::Shorthand(_)
            | Kind::Property { .. }
            | Kind::Set(_) => Some(1),
            Kind::Start
            | Kind::End
            | Kind::EndZ
            | Kind::Anchor
            | Kind::Flags(_)
            | Kind::Comment => Some(0),
        };
        length = length.zip(atom).map(|(length, atom)| length + atom);
        last = atom;
    }
    lengths.push(length);
    lengths
}

/// Length of branches all of the same length.
fn same_length(lengths: &[Option<usize>]) -> Option<usize> {
    let first = lengths.first().copied().flatten()?;
    lengths
        .iter()
        .all(|&length| length == Some(first))
        .then_some(first)
}

/// Character of a literal character token.
fn char_value(token: &Token) -> char {
    match token.kind {
        Kind::Char { value, .. } => value,
        _ => '\0',
    }
}

/// Letter of a class member that the output writes otherwise: `s`, `h`,
/// their complements, `w` and `W`, `[:space:]` read as `s`, `[:blank:]` as
/// `h`; `\0` for another one.
fn item_letter(item: &Item) -> char {
    match item {
        Item::Shorthand(letter) => *letter,
        Item::Posix(name) if name == "space" => 's',
        Item::Posix(name) if name == "blank" => 'h',
        _ => '\0',
    }
}

/// Text of a member of a class.
fn item_text(item: &Item) -> String {
    match item {
        Item::Char(c) => literal(*c),
        Item::Range(low, high) => format!("{}-{}", literal(*low), literal(*high)),
        Item::Shorthand(letter) => format!("\\{letter}"),
        Item::Property { negated, name } => {
            format!("\\{}{{{name}}}", if *negated { 'P' } else { 'p' })
        }
        Item::Posix(name) => format!("[:{name}:]"),
    }
}

/// Character of a pair of `APART` that `set` holds without the other one,
/// and the other one.
fn apart_member(set: &Set) -> Option<(char, char)> {
    let holds = |c: char| {
        set.items.iter().any(|item| match *item {
            Item::Char(member) => member == c,
            Item::Range(low, high) => (low..=high).contains(&c),
            _ => false,
        })
    };
    fold::APART
        .iter()
        .find_map(|&(a, b)| match (holds(a), holds(b)) {
            (true, false) => Some((a, b)),
            (false, true) => Some((b, a)),
            _ => None,
        })
}

/// Message of the refusal of `\w` or `\W` of PCRE2.
fn pcre2_word(letter: char) -> String {
    format!(
        "`\\{letter}`: PCRE2 reads the word characters after its version, which the version of MariaDB does not tell"
    )
}

/// Literal character, written alike by both libraries in a class and out of
/// one: escaped when special in one of them, as itself when a letter, a
/// digit, a space or another ASCII punctuation, by its code point otherwise.
pub(super) fn literal(c: char) -> String {
    match c {
        '\t' => r"\t".to_string(),
        '\n' => r"\n".to_string(),
        '\r' => r"\r".to_string(),
        _ if SPECIAL.contains(c) => format!("\\{c}"),
        _ if c.is_alphanumeric() || c == ' ' || c.is_ascii_punctuation() => c.to_string(),
        _ => format!("\\x{{{:X}}}", u32::from(c)),
    }
}

/// Tells whether a step can carry a lookahead that only case insensitivity
/// matches.
fn guardable(step: Step) -> bool {
    match step {
        Step::Char(c) => fold::partner(c).is_some(),
        Step::Multi(multi) => multi.paired(),
    }
}

/// Tells whether `path`, read with the case of its characters, matches
/// `values` only.
fn written_as(path: &[Step], values: &[char]) -> bool {
    let mut chars = Vec::new();
    for &step in path {
        match step {
            Step::Char(c) => chars.push(c),
            Step::Multi(Multi { chars: [c], .. }) => chars.push(*c),
            Step::Multi(_) => return false,
        }
    }
    chars == values
}

/// ASCII letter `c` of the other case.
fn other_case(c: char) -> char {
    if c.is_ascii_lowercase() {
        c.to_ascii_uppercase()
    } else {
        c.to_ascii_lowercase()
    }
}

/// First character a step matches, for the messages.
fn step_char(step: Step) -> char {
    match step {
        Step::Char(c) => c,
        Step::Multi(multi) => multi.chars[0],
    }
}

/// Text of a path for PCRE2, the step at `guard` after a lookahead that only
/// case insensitivity matches.
fn path_text(path: &[Step], guard: Option<usize>) -> String {
    let mut text = String::new();
    for (index, &step) in path.iter().enumerate() {
        let guarded = guard == Some(index);
        match step {
            Step::Char(c) if guarded => {
                let partner = fold::partner(c).unwrap_or(c);
                text.push_str(&format!("(?={}){}", literal(partner), literal(c)));
            }
            Step::Char(c) => text.push_str(&literal(c)),
            Step::Multi(multi) if guarded => text.push_str(&format!(
                "(?={}){}",
                literal(multi.chars[0]),
                literal(multi.chars[1])
            )),
            Step::Multi(Multi { chars: [c], .. }) => text.push_str(&literal(*c)),
            Step::Multi(multi) => {
                let members: String = multi.chars.iter().map(|&c| literal(c)).collect();
                text.push_str(&format!("[{members}]"));
            }
        }
    }
    text
}
