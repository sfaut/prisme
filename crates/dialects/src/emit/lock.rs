//! Locking clauses (`prisme_plan::Lock`), written after the `LIMIT` of the
//! block they lock, in the form of the output (rules `lock.*`,
//! `crates/probe/cases/locking.sql`):
//!
//! - `FOR SHARE` (MySQL) and `LOCK IN SHARE MODE` take the same lock:
//!   towards MariaDB, `FOR SHARE` is written `LOCK IN SHARE MODE`, which
//!   takes the same options; towards MySQL, `LOCK IN SHARE MODE` followed by
//!   an option is written `FOR SHARE`, which takes it;
//! - `OF` (MySQL) naming every table of the block is left out towards
//!   MariaDB, which locks them all; naming some of them, an error;
//! - several clauses (MySQL) of the same lock and the same wait, together
//!   on every table of the block, are written as one towards MariaDB; an
//!   error otherwise;
//! - `WAIT n` (MariaDB): towards MySQL, `WAIT 0` is written `NOWAIT`, which
//!   it is by the documentation of MariaDB; another timeout is an error,
//!   MySQL having no timeout of the locks per statement (`SET_VAR` refuses
//!   `innodb_lock_wait_timeout`, error 3637);
//! - `NOWAIT`, `SKIP LOCKED`: an error towards a server without them.
//!
//! The tables of a common table expression or of a derived table read by
//! the block are locked by no clause of the block: only its tables of the
//! database count for `OF`.

use prisme_plan::{BlockTableKind, Lock, LockClause, LockForm, LockStrength, LockWait, Span};

use super::Emitter;
use crate::report::StepKind;
use crate::versions::Construct;

/// What a query does with the locking clauses at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LockEnd {
    /// Writes them after its `ORDER BY` and its `LIMIT`.
    Write,
    /// Leaves them to the query around, which writes them after its own
    /// `ORDER BY` and `LIMIT`: the last operand of a set operation without
    /// parentheses, a query between parentheses whose clause the output
    /// would drop there (`locking` module).
    Lift,
    /// Writes the locked operands of its set operation between parentheses,
    /// their clauses inside: an operand of a set operation, written without
    /// parentheses, that does not end the query around.
    Avoid,
}

impl Emitter<'_> {
    /// Locking clauses `clauses`, each one after a space.
    pub(super) fn write_lock_clauses(&mut self, clauses: &[LockClause]) {
        for clause in clauses {
            self.lock_clause(clause);
        }
    }

    /// Clauses of `lock` that the output takes: as written, or merged into
    /// one, `OF` left out, towards an output without several clauses or
    /// without `OF`.
    pub(super) fn output_clauses(&mut self, lock: &Lock) -> Vec<LockClause> {
        let span = lock.clauses.first().and_then(|clause| clause.span);
        let mut clauses = lock.clauses.clone();
        if clauses.len() > 1 && !self.has(Construct::LockClauses) {
            let first = &clauses[0];
            let alike = (clauses.iter())
                .all(|clause| clause.strength == first.strength && clause.wait == first.wait);
            if !alike || !self.cover_every_table(lock, &clauses) {
                let message = format!(
                    "{} takes one locking clause, on every table of the block: several clauses \
                     of different locks, or on some of its tables, not supported by Prisme",
                    self.target()
                );
                self.error("lock.clauses", &message, span);
                return clauses;
            }
            let message = format!(
                "{} takes one locking clause: the clauses, of the same lock on every table of \
                 the block, written as one",
                self.target()
            );
            self.step("lock.clauses", StepKind::Rewrite, message, span);
            let mut merged = clauses.swap_remove(0);
            merged.tables.clear();
            clauses = vec![merged];
        }
        if !self.has(Construct::LockOf) {
            for clause in &mut clauses {
                if clause.tables.is_empty() {
                    continue;
                }
                let one = std::slice::from_ref(clause);
                if !self.cover_every_table(lock, one) {
                    let message = format!(
                        "{} has no {}, and locks every table of the block: a lock on some of \
                         them, not supported by Prisme",
                        self.target(),
                        Construct::LockOf.label()
                    );
                    self.error("lock.of", &message, clause.span);
                    continue;
                }
                let message = format!(
                    "{} has no {}: left out, the clause naming every table of the block",
                    self.target(),
                    Construct::LockOf.label()
                );
                self.step("lock.of", StepKind::Omission, message, clause.span);
                clause.tables.clear();
            }
        }
        clauses
    }

    /// Tells whether `clauses` name together every table of the database
    /// that the block of `lock` reads, a clause without `OF` naming them
    /// all.
    fn cover_every_table(&self, lock: &Lock, clauses: &[LockClause]) -> bool {
        (lock.tables().iter())
            .filter(|table| table.kind == BlockTableKind::Table)
            .all(|table| {
                clauses.iter().any(|clause| {
                    clause.tables.is_empty()
                        || clause.tables.iter().any(|name| table.is_named(name))
                })
            })
    }

    /// ` FOR UPDATE`, ` FOR SHARE` or ` LOCK IN SHARE MODE`, then its
    /// tables and its wait.
    fn lock_clause(&mut self, clause: &LockClause) {
        self.at(clause.span);
        let wait = self.output_wait(&clause.wait, clause.span);
        let share_mode = match (clause.strength, clause.form) {
            (LockStrength::Update, _) => false,
            (LockStrength::Share, LockForm::For) if !self.has(Construct::LockShare) => {
                let message = format!(
                    "{} has no `FOR SHARE`: written `LOCK IN SHARE MODE`, the same lock",
                    self.target()
                );
                self.step("lock.share", StepKind::Rewrite, message, clause.span);
                true
            }
            (LockStrength::Share, LockForm::For) => false,
            (LockStrength::Share, LockForm::ShareMode)
                if wait != LockWait::Wait && !self.has(Construct::ShareModeOptions) =>
            {
                let message = format!(
                    "{} has no {}: written `FOR SHARE`, the same lock",
                    self.target(),
                    Construct::ShareModeOptions.label()
                );
                self.step("lock.share", StepKind::Rewrite, message, clause.span);
                false
            }
            (LockStrength::Share, LockForm::ShareMode) => true,
        };
        self.clause(match (clause.strength, share_mode) {
            (LockStrength::Update, _) => " FOR UPDATE",
            (LockStrength::Share, false) => " FOR SHARE",
            (LockStrength::Share, true) => " LOCK IN SHARE MODE",
        });
        if !clause.tables.is_empty() {
            self.write(" OF ");
            for (index, table) in clause.tables.iter().enumerate() {
                if index > 0 {
                    self.write(", ");
                }
                if let Some(database) = &table.database {
                    self.ident(database, clause.span);
                    self.write(".");
                }
                self.ident(&table.name, clause.span);
            }
        }
        match &wait {
            LockWait::Wait => {}
            LockWait::NoWait => self.write(" NOWAIT"),
            LockWait::SkipLocked => self.write(" SKIP LOCKED"),
            LockWait::Seconds(seconds) => self.write(&format!(" WAIT {seconds}")),
        }
    }

    /// `wait` as the output takes it: `WAIT 0` written `NOWAIT` towards an
    /// output without `WAIT n`; an error for a wait the output does not
    /// have, written as is.
    fn output_wait(&mut self, wait: &LockWait, span: Option<Span>) -> LockWait {
        let construct = match wait {
            LockWait::Wait => return LockWait::Wait,
            LockWait::NoWait => Construct::LockNowait,
            LockWait::SkipLocked => Construct::LockSkipLocked,
            LockWait::Seconds(_) => Construct::LockWaitSeconds,
        };
        if self.has(construct) {
            return wait.clone();
        }
        if let LockWait::Seconds(seconds) = wait
            && is_zero(seconds)
            && self.has(Construct::LockNowait)
        {
            let message = format!(
                "{} has no {}: `WAIT 0` written `NOWAIT`, which it is",
                self.target(),
                construct.label()
            );
            self.step("lock.wait", StepKind::Rewrite, message, span);
            return LockWait::NoWait;
        }
        let message = if construct == Construct::LockWaitSeconds {
            format!(
                "{} has no {}, nor a timeout of the locks for one statement (`SET_VAR` refuses \
                 `innodb_lock_wait_timeout`, error 3637): not supported by Prisme",
                self.target(),
                construct.label()
            )
        } else {
            format!(
                "{} has no {}: not supported by Prisme",
                self.target(),
                construct.label()
            )
        };
        self.error("lock.wait", &message, span);
        wait.clone()
    }
}

/// Tells whether `seconds`, a number of `WAIT` as written (`0`, `0.0`,
/// `0e0`, `0x0`), is zero.
fn is_zero(seconds: &str) -> bool {
    let lower = seconds.to_ascii_lowercase();
    match lower.strip_prefix("0x") {
        Some(digits) => digits.chars().all(|digit| digit == '0'),
        None => lower.parse::<f64>().is_ok_and(|value| value == 0.0),
    }
}
