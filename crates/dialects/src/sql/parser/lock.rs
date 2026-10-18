//! Locking clauses of a query, as the servers read them
//! (`crates/probe/cases/locking.sql`): after the `LIMIT`, `FOR UPDATE`,
//! `FOR SHARE` (MySQL) or `LOCK IN SHARE MODE`; after `FOR`, `OF` and its
//! tables (MySQL), then `NOWAIT` or `SKIP LOCKED`; MariaDB takes these
//! options after `LOCK IN SHARE MODE` too, and `WAIT n`. MySQL takes
//! several clauses, each one on its own tables, which the builder checks.

use prisme_plan::{LockForm, LockStrength, LockWait, Span};

use super::{ParseError, Parser};
use crate::lexer::{Punct, TokenKind};
use crate::sql::ast::Locking;
use crate::versions::Construct;

impl Parser<'_> {
    /// Locking clauses after a query, in order; empty when none comes next.
    pub(super) fn locks(&mut self) -> Result<Vec<Locking>, ParseError> {
        let mut locks = Vec::new();
        while let Some(lock) = self.lock()? {
            if !locks.is_empty() {
                self.check(Construct::LockClauses, lock.span)?;
            }
            locks.push(lock);
        }
        Ok(locks)
    }

    /// Locking clause, when one comes next.
    fn lock(&mut self) -> Result<Option<Locking>, ParseError> {
        let start = self.here().start;
        let (strength, form) = if self.at_keyword("FOR")
            && ["UPDATE", "SHARE"]
                .iter()
                .any(|word| self.is_word(self.peek_at(1), word))
        {
            self.advance();
            if self.eat_keyword("UPDATE").is_some() {
                (LockStrength::Update, LockForm::For)
            } else {
                let share = self.expect_keyword("SHARE")?;
                self.check(Construct::LockShare, Span::new(start, share.end))?;
                (LockStrength::Share, LockForm::For)
            }
        } else if self.eat_keyword("LOCK").is_some() {
            self.expect_keyword("IN")?;
            self.expect_keyword("SHARE")?;
            self.expect_keyword("MODE")?;
            (LockStrength::Share, LockForm::ShareMode)
        } else {
            return Ok(None);
        };
        let mut tables = Vec::new();
        if form == LockForm::For
            && let Some(of) = self.eat_keyword("OF")
        {
            self.check(Construct::LockOf, of)?;
            tables.push(self.object_name(2)?);
            while self.eat_punct(Punct::Comma).is_some() {
                tables.push(self.object_name(2)?);
            }
        }
        let option = self.here();
        let wait = self.lock_wait()?;
        if form == LockForm::ShareMode && wait != LockWait::Wait {
            self.check(Construct::ShareModeOptions, self.span_from(option.start))?;
        }
        Ok(Some(Locking {
            strength,
            tables,
            wait,
            form,
            span: self.span_from(start),
        }))
    }

    /// `NOWAIT`, `SKIP LOCKED`, `WAIT n` (MariaDB), or nothing.
    fn lock_wait(&mut self) -> Result<LockWait, ParseError> {
        let start = self.here().start;
        if self.eat_keyword("NOWAIT").is_some() {
            self.check(Construct::LockNowait, self.span_from(start))?;
            return Ok(LockWait::NoWait);
        }
        if self.at_keyword("SKIP") && self.is_word(self.peek_at(1), "LOCKED") {
            self.advance();
            self.advance();
            self.check(Construct::LockSkipLocked, self.span_from(start))?;
            return Ok(LockWait::SkipLocked);
        }
        if !self.at_keyword("WAIT") {
            return Ok(LockWait::Wait);
        }
        self.advance();
        self.check(Construct::LockWaitSeconds, self.span_from(start))?;
        // MariaDB reads any number there, of any form (`WAIT 0.5`, `WAIT
        // 1e1`, `WAIT 0x2`), and nothing else (a string, a sign).
        match self.peek() {
            Some(token)
                if matches!(
                    token.kind,
                    TokenKind::Integer
                        | TokenKind::Decimal
                        | TokenKind::Float
                        | TokenKind::HexNumber(_)
                ) =>
            {
                let seconds = token.text(self.text).to_string();
                self.advance();
                Ok(LockWait::Seconds(seconds))
            }
            _ => Err(self.expected("a number of seconds")),
        }
    }
}
