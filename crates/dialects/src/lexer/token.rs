//! Tokens of the MySQL family, with their position in the input.

use prisme_plan::Span;

use crate::versions::Version;

/// Token of a query, with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Position in the input, quotes and prefixes included.
    pub span: Span,
}

/// Kind of a token. Words, numbers and variables keep their text in the
/// input, read through `Token::text`; quoted tokens carry their value, quotes
/// and escapes resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// Unquoted word: keyword or identifier, told apart by the parser, as
    /// most keywords of the MySQL family are not reserved.
    Word,
    /// Identifier between backticks, or between double quotes under
    /// `ANSI_QUOTES`.
    QuotedIdent(String),
    /// String between quotes.
    String {
        /// Value, escapes resolved.
        value: String,
        /// Quote used.
        quote: Quote,
    },
    /// National string, `N'...'`.
    NationalString(String),
    /// Hexadecimal string, `X'4A'`: the digits.
    HexString(String),
    /// Bit string, `B'0101'`: the digits.
    BitString(String),
    /// Hexadecimal number, `0x4A`: the digits.
    HexNumber(String),
    /// Bit number, `0b0101`: the digits.
    BitNumber(String),
    /// Integer, `123`.
    Integer,
    /// Exact decimal, `1.5`, `.5`, `1.`.
    Decimal,
    /// Approximate number, with an exponent: `1e5`, `1.5E-3`.
    Float,
    /// User variable, `@name`, `@'name'`.
    UserVariable {
        /// Name, quotes removed.
        name: String,
        /// `true` when the name is written between quotes.
        quoted: bool,
    },
    /// System variable, `@@name`: the scope and the dots that may follow
    /// (`@@session.sql_mode`) are separate tokens.
    SystemVariable,
    /// Parameter of a prepared statement, `?`.
    Placeholder,
    /// Operator or punctuation.
    Punct(Punct),
    /// Comment, kept for the report and the comments of the output.
    Comment(CommentKind),
    /// Start of an executable comment whose condition holds (`/*!80000`):
    /// its content is code, up to `ConditionalEnd`.
    ConditionalStart {
        /// Minimum version written, if any.
        version: Option<Version>,
        /// `true` for `/*M!`, run by MariaDB only.
        mariadb_only: bool,
    },
    /// End of an executable comment whose content is code, `*/`.
    ConditionalEnd,
}

/// Quote of a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quote {
    /// `'...'`.
    Single,
    /// `"..."`, without `ANSI_QUOTES`.
    Double,
}

/// Kind of a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentKind {
    /// `-- ...`, up to the end of the line.
    Dash,
    /// `# ...`, up to the end of the line.
    Hash,
    /// `/* ... */`.
    Block,
    /// Optimizer hint, `/*+ ... */`: a hint after `SELECT`, `INSERT`,
    /// `REPLACE`, `UPDATE` or `DELETE`, a comment elsewhere.
    Hint,
    /// Executable comment whose condition does not hold: a plain comment
    /// for the server.
    Executable,
}

/// Operator or punctuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Punct {
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `.`
    Dot,
    /// `:`
    Colon,
    /// `*`
    Star,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `^`
    Caret,
    /// `~`
    Tilde,
    /// `&`
    Ampersand,
    /// `|`
    Pipe,
    /// `!`
    Bang,
    /// `@`, alone (`'user'@'host'`).
    At,
    /// `=`
    Eq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    LtEq,
    /// `>=`
    GtEq,
    /// `<>`
    LtGt,
    /// `!=`
    BangEq,
    /// `<=>`
    Spaceship,
    /// `<<`
    ShiftLeft,
    /// `>>`
    ShiftRight,
    /// `&&`
    DoubleAmpersand,
    /// `||`
    DoublePipe,
    /// `:=`
    Assign,
    /// `->`
    Arrow,
    /// `->>`
    LongArrow,
}

impl Token {
    /// Text of the token in `input`, the text it was read from.
    pub fn text<'a>(&self, input: &'a str) -> &'a str {
        &input[self.span.start..self.span.end]
    }

    /// Tells whether the token is a comment or the boundary of an executable
    /// comment, which the parser skips.
    pub fn is_trivia(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Comment(_) | TokenKind::ConditionalStart { .. } | TokenKind::ConditionalEnd
        )
    }
}

impl Punct {
    /// Text of the operator.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LeftParen => "(",
            Self::RightParen => ")",
            Self::LeftBrace => "{",
            Self::RightBrace => "}",
            Self::Comma => ",",
            Self::Semicolon => ";",
            Self::Dot => ".",
            Self::Colon => ":",
            Self::Star => "*",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::Caret => "^",
            Self::Tilde => "~",
            Self::Ampersand => "&",
            Self::Pipe => "|",
            Self::Bang => "!",
            Self::At => "@",
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Gt => ">",
            Self::LtEq => "<=",
            Self::GtEq => ">=",
            Self::LtGt => "<>",
            Self::BangEq => "!=",
            Self::Spaceship => "<=>",
            Self::ShiftLeft => "<<",
            Self::ShiftRight => ">>",
            Self::DoubleAmpersand => "&&",
            Self::DoublePipe => "||",
            Self::Assign => ":=",
            Self::Arrow => "->",
            Self::LongArrow => "->>",
        }
    }

    /// Operators, the longest first: the lexer takes the first one that
    /// matches.
    pub(crate) const BY_LENGTH: [Self; 34] = [
        Self::Spaceship,
        Self::LongArrow,
        Self::LtEq,
        Self::GtEq,
        Self::LtGt,
        Self::BangEq,
        Self::ShiftLeft,
        Self::ShiftRight,
        Self::DoubleAmpersand,
        Self::DoublePipe,
        Self::Assign,
        Self::Arrow,
        Self::LeftParen,
        Self::RightParen,
        Self::LeftBrace,
        Self::RightBrace,
        Self::Comma,
        Self::Semicolon,
        Self::Dot,
        Self::Colon,
        Self::Star,
        Self::Plus,
        Self::Minus,
        Self::Slash,
        Self::Percent,
        Self::Caret,
        Self::Tilde,
        Self::Ampersand,
        Self::Pipe,
        Self::Bang,
        Self::At,
        Self::Eq,
        Self::Lt,
        Self::Gt,
    ];
}
