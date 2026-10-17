//! `CAST(x AS type)`, `CONVERT(x, type)` and `CONVERT(x USING charset)`,
//! with the targets of each dialect in its version
//! (`crates/probe/cases/cast-forms.txt`): `SIGNED [INTEGER]`, `CHAR(n)
//! CHARACTER SET cs`, `DATETIME(n)`... in both, `DOUBLE PRECISION` or `YEAR`
//! in MySQL only, `INT` or `VARCHAR(n)` in MariaDB only.

use prisme_plan::{CastForm, CastTarget, CharTarget, CharWord, Charset, IntegerWord, Span};

use super::{ParseError, Parser};
use crate::Dialect;
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::{CastCall, Expr, ExprKind};
use crate::versions::Construct;

/// Largest precision of a `DECIMAL`.
const MAX_PRECISION: u32 = 65;

/// Largest number of fractional digits of a `DATETIME` or a `TIME`.
const MAX_FRACTION: u32 = 6;

/// Spatial types, targets of `CAST` in MySQL.
const SPATIAL: [&str; 7] = [
    "POINT",
    "LINESTRING",
    "POLYGON",
    "MULTIPOINT",
    "MULTILINESTRING",
    "MULTIPOLYGON",
    "GEOMETRYCOLLECTION",
];

impl Parser<'_> {
    /// `CAST(x [AT TIME ZONE 'zone'] AS type)`, after its name.
    pub(super) fn cast(&mut self, start: usize) -> Result<Expr, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let operand = self.expr()?;
        let mut form = CastForm::Cast;
        if let Some(at) = self.eat_keyword("AT") {
            self.check(Construct::CastAtTimeZone, at)?;
            self.expect_keyword("TIME")?;
            self.expect_keyword("ZONE")?;
            let interval = self.eat_keyword("INTERVAL").is_some();
            let Some(TokenKind::String { value, .. }) = self.peek().map(|token| token.kind.clone())
            else {
                return Err(self.expected("a time zone between quotes"));
            };
            self.advance();
            form = CastForm::AtTimeZone {
                zone: value,
                interval,
            };
        }
        self.expect_keyword("AS")?;
        if form != CastForm::Cast && !self.at_keyword("DATETIME") {
            return Err(self.expected("`DATETIME`"));
        }
        let target = self.cast_target()?;
        if self.options.dialect == Dialect::MySql && self.at_keyword("ARRAY") {
            return Err(ParseError {
                message: "`CAST(... AS ... ARRAY)`: only in the key of a multi-valued index"
                    .to_string(),
                span: self.here(),
            });
        }
        self.expect_punct(Punct::RightParen)?;
        Ok(cast(operand, target, form, self.span_from(start)))
    }

    /// `CONVERT(x, type)` or `CONVERT(x USING charset)`, after its name.
    pub(super) fn convert(&mut self, start: usize) -> Result<Expr, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let operand = self.expr()?;
        let (target, form) = if self.eat_punct(Punct::Comma).is_some() {
            (self.cast_target()?, CastForm::Convert)
        } else if self.eat_keyword("USING").is_some() {
            let charset = self.charset_name(false)?;
            let target = CastTarget::Char(CharTarget {
                word: CharWord::Char,
                length: None,
                charset: Some(charset),
                binary: false,
                collation: None,
            });
            (target, CastForm::ConvertUsing)
        } else {
            return Err(self.expected("`,` or `USING`"));
        };
        self.expect_punct(Punct::RightParen)?;
        Ok(cast(operand, target, form, self.span_from(start)))
    }

    /// Type after `AS` or after the comma of `CONVERT`.
    fn cast_target(&mut self) -> Result<CastTarget, ParseError> {
        let span = self.here();
        let Some(word) = self.word_upper() else {
            return Err(self.expected("a type"));
        };
        let target = match word.as_str() {
            "SIGNED" | "UNSIGNED" => {
                self.advance();
                let integer = self.integer_word();
                if word == "SIGNED" {
                    CastTarget::Signed(integer)
                } else {
                    CastTarget::Unsigned(integer)
                }
            }
            "INTEGER" | "INT" => {
                self.check(Construct::CastInteger, span)?;
                CastTarget::Integer(self.integer_word().expect("an integer word comes next"))
            }
            "DECIMAL" | "DEC" => {
                self.advance();
                self.decimal_target(word == "DEC")?
            }
            "DOUBLE" => {
                self.advance();
                self.check(Construct::CastDouble, span)?;
                if let Some(precision) = self.eat_keyword("PRECISION") {
                    self.check(Construct::CastDoubleSynonyms, precision)?;
                    CastTarget::Double {
                        precision: true,
                        digits: None,
                    }
                } else if self.at_punct(Punct::LeftParen) {
                    self.check(Construct::CastDoubleDigits, self.here())?;
                    self.advance();
                    let digits = self.size(255, "digits")?;
                    self.expect_punct(Punct::Comma)?;
                    let decimals = self.size(digits.min(30), "decimals, at most the digits")?;
                    self.expect_punct(Punct::RightParen)?;
                    CastTarget::Double {
                        precision: false,
                        digits: Some((narrow(digits), narrow(decimals))),
                    }
                } else {
                    CastTarget::Double {
                        precision: false,
                        digits: None,
                    }
                }
            }
            "FLOAT" => {
                self.advance();
                self.check(Construct::CastDouble, span)?;
                if self.at_punct(Punct::LeftParen) {
                    self.check(Construct::CastDoubleSynonyms, self.here())?;
                    CastTarget::Float(Some(narrow(self.parenthesized_size(53, "bits")?)))
                } else {
                    CastTarget::Float(None)
                }
            }
            "REAL" => {
                self.advance();
                self.check(Construct::CastDoubleSynonyms, span)?;
                CastTarget::Real {
                    float: self.options.mode.real_as_float,
                }
            }
            "CHAR" | "CHARACTER" | "NCHAR" | "NATIONAL" | "VARCHAR" => {
                CastTarget::Char(self.char_target(&word, span)?)
            }
            "BINARY" => {
                self.advance();
                CastTarget::Binary(self.length()?)
            }
            "DATE" => {
                self.advance();
                CastTarget::Date
            }
            "DATETIME" | "TIME" => {
                self.advance();
                let fraction = self.fraction()?;
                if word == "TIME" {
                    CastTarget::Time(fraction)
                } else {
                    CastTarget::DateTime(fraction)
                }
            }
            "JSON" => {
                self.check(Construct::CastJson, span)?;
                self.advance();
                CastTarget::Json
            }
            "YEAR" => {
                self.check(Construct::CastYear, span)?;
                self.advance();
                CastTarget::Year
            }
            "INTERVAL" => {
                self.check(Construct::CastInterval, span)?;
                self.advance();
                self.expect_keyword("DAY_SECOND")?;
                CastTarget::Interval(narrow(
                    self.parenthesized_size(MAX_FRACTION, "fractional digits")?,
                ))
            }
            "INET6" | "INET4" | "UUID" => {
                let construct = match word.as_str() {
                    "INET6" => Construct::Inet6Type,
                    "INET4" => Construct::Inet4Type,
                    _ => Construct::UuidType,
                };
                self.check(construct, span)?;
                self.advance();
                CastTarget::Named(word)
            }
            spatial if SPATIAL.contains(&spatial) => {
                self.check(Construct::CastSpatial, span)?;
                self.advance();
                CastTarget::Spatial(word)
            }
            _ => return Err(self.expected("a type")),
        };
        Ok(target)
    }

    /// `[(precision[, scale])]` of a `DECIMAL`, after its name: a precision
    /// of 65 at most, a scale of 30 in MySQL, 38 in MariaDB, at most the
    /// precision.
    fn decimal_target(&mut self, dec: bool) -> Result<CastTarget, ParseError> {
        let (mut precision, mut scale) = (None, None);
        if self.eat_punct(Punct::LeftParen).is_some() {
            let digits = self.size(MAX_PRECISION, "digits")?;
            precision = Some(narrow(digits));
            if self.eat_punct(Punct::Comma).is_some() {
                let largest = match self.options.dialect {
                    Dialect::MySql => 30,
                    Dialect::MariaDb => 38,
                };
                // A precision of 0 is the default, 10.
                let bound = largest.min(if digits == 0 { 10 } else { digits });
                scale = Some(narrow(self.size(bound, "decimals, at most the digits")?));
            }
            self.expect_punct(Punct::RightParen)?;
        }
        Ok(CastTarget::Decimal {
            dec,
            precision,
            scale,
        })
    }

    /// Character string target, from its word: `CHAR[(n)]`, `CHARACTER`,
    /// `NCHAR`, `NATIONAL CHAR` (MySQL), `VARCHAR(n)` (MariaDB), then its
    /// character set, `BINARY`, and `COLLATE` (MariaDB).
    fn char_target(&mut self, word: &str, span: Span) -> Result<CharTarget, ParseError> {
        self.advance();
        let word = match word {
            "CHAR" => CharWord::Char,
            "CHARACTER" => CharWord::Character,
            "NCHAR" => CharWord::NChar,
            "NATIONAL" => {
                self.check(Construct::CastNationalChar, span)?;
                self.expect_keyword("CHAR")?;
                CharWord::NationalChar
            }
            _ => {
                self.check(Construct::CastVarchar, span)?;
                CharWord::VarChar
            }
        };
        let length = if word == CharWord::VarChar {
            Some(self.parenthesized_size(u32::MAX, "characters")?)
        } else {
            self.length()?
        };
        let mut binary = self.eat_keyword("BINARY").is_some();
        let charset = self.charset_clause()?;
        if !binary {
            binary = self.eat_keyword("BINARY").is_some();
        }
        let collation = match self.eat_keyword("COLLATE") {
            Some(collate) => {
                self.check(Construct::CastCollate, collate)?;
                Some(self.quoted_name("a collation")?.0)
            }
            None => None,
        };
        Ok(CharTarget {
            word,
            length,
            charset,
            binary,
            collation,
        })
    }

    /// `CHARACTER SET name`, `CHARSET name`, `ASCII`, `UNICODE` or `BYTE`,
    /// when written.
    fn charset_clause(&mut self) -> Result<Option<Charset>, ParseError> {
        if self.eat_keyword("CHARACTER").is_some() {
            self.expect_keyword("SET")?;
            return self.charset_name(false).map(Some);
        }
        if self.eat_keyword("CHARSET").is_some() {
            return self.charset_name(true).map(Some);
        }
        let charset = [
            ("ASCII", Charset::Ascii),
            ("UNICODE", Charset::Unicode),
            ("BYTE", Charset::Byte),
        ]
        .into_iter()
        .find(|(keyword, _)| self.at_keyword(keyword));
        if let Some((_, charset)) = charset {
            self.advance();
            return Ok(Some(charset));
        }
        Ok(None)
    }

    /// Name of a character set, `CHARSET` written for `short`.
    fn charset_name(&mut self, short: bool) -> Result<Charset, ParseError> {
        let (name, quoted) = self.quoted_name("a character set")?;
        Ok(Charset::Named {
            name,
            short,
            quoted,
        })
    }

    /// Name of a character set or of a collation, `what`: a word, a quoted
    /// identifier, or a string (`true`).
    fn quoted_name(&mut self, what: &str) -> Result<(String, bool), ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.expected(what));
        };
        let name = match &token.kind {
            TokenKind::Word => (token.text(self.text).to_string(), false),
            TokenKind::QuotedIdent(name) => (name.clone(), false),
            TokenKind::String { value, .. } => (value.clone(), true),
            _ => return Err(self.expected(what)),
        };
        self.advance();
        Ok(name)
    }

    /// `INTEGER` or `INT`, consumed when it comes next.
    fn integer_word(&mut self) -> Option<IntegerWord> {
        let word = if self.at_keyword("INTEGER") {
            IntegerWord::Integer
        } else if self.at_keyword("INT") {
            IntegerWord::Int
        } else {
            return None;
        };
        self.advance();
        Some(word)
    }

    /// `(n)` of a string type, when written.
    fn length(&mut self) -> Result<Option<u32>, ParseError> {
        if self.at_punct(Punct::LeftParen) {
            self.parenthesized_size(u32::MAX, "bytes or characters")
                .map(Some)
        } else {
            Ok(None)
        }
    }

    /// `(n)` of a `DATETIME` or a `TIME`, 6 at most, when written.
    fn fraction(&mut self) -> Result<Option<u8>, ParseError> {
        if self.at_punct(Punct::LeftParen) {
            Ok(Some(narrow(
                self.parenthesized_size(MAX_FRACTION, "fractional digits")?,
            )))
        } else {
            Ok(None)
        }
    }

    /// `(n)`, `n` an integer of `largest` at most.
    fn parenthesized_size(&mut self, largest: u32, what: &str) -> Result<u32, ParseError> {
        self.expect_punct(Punct::LeftParen)?;
        let size = self.size(largest, what)?;
        self.expect_punct(Punct::RightParen)?;
        Ok(size)
    }

    /// Integer of `largest` at most, a number of `what`.
    fn size(&mut self, largest: u32, what: &str) -> Result<u32, ParseError> {
        let expected = format!("a number of {what} up to {largest}");
        let size = match self.peek() {
            Some(token) if token.kind == TokenKind::Integer => {
                token.text(self.text).parse::<u32>().ok()
            }
            _ => return Err(self.expected(&expected)),
        };
        match size {
            Some(size) if size <= largest => {
                self.advance();
                Ok(size)
            }
            _ => Err(self.expected(&expected)),
        }
    }

    /// Next token in capitals when it is an unquoted word.
    fn word_upper(&self) -> Option<String> {
        self.peek()
            .filter(|token| token.kind == TokenKind::Word)
            .map(|token| token.text(self.text).to_ascii_uppercase())
    }
}

/// Conversion node.
fn cast(operand: Expr, target: CastTarget, form: CastForm, span: Span) -> Expr {
    Expr {
        kind: ExprKind::Cast(Box::new(CastCall {
            operand,
            target,
            form,
        })),
        span,
    }
}

/// Size checked by the parser to fit a byte.
fn narrow(size: u32) -> u8 {
    u8::try_from(size).expect("a size checked against its bound")
}
