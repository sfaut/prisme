//! User and system variables, and the assignments of user variables
//! (`@x := 1`).
//!
//! A user variable is written `@name`, its name as read, between backticks
//! when it was quoted (`@'my var'` is written `` @`my var` ``, read alike
//! whatever the SQL mode). The variable of an empty name (`@''`, `@` alone),
//! refused by MariaDB from 10.7.1, is an explicit error there; its
//! assignment too, refused by MySQL.
//! An assignment is written between parentheses inside another expression:
//! its value extends as far as an expression goes (`1 + @x := 2 * 3` assigns
//! 6).
//!
//! A system variable gives the value of the server of the output: it is
//! written as read, with its scope. `tx_isolation` and `tx_read_only` of
//! MariaDB, absent from MySQL, and `transaction_isolation` and
//! `transaction_read_only` of MySQL, absent from MariaDB before 11.1.1, take
//! the name of the other dialect, of the same values. Any other variable
//! absent from the output is refused by its server (error 1193:
//! `max_execution_time` of MySQL, `max_statement_time` of MariaDB).

use prisme_plan::{Expr, Ident, Span, Variable, VariableScope};

use super::Emitter;
use crate::report::StepKind;
use crate::versions::Construct;

/// Names of the system variables of the transactions: the old one, of
/// MariaDB, and the one of MySQL.
const TRANSACTION_VARIABLES: [(&str, &str); 2] = [
    ("tx_isolation", "transaction_isolation"),
    ("tx_read_only", "transaction_read_only"),
];

impl Emitter<'_> {
    /// Writes the variable `variable`.
    pub(super) fn variable(&mut self, variable: &Variable, span: Option<Span>) {
        if variable.scope == VariableScope::User {
            self.write("@");
            self.user_variable_name(&variable.name, Construct::EmptyUserVariable, span);
            return;
        }
        self.write("@@");
        match (&variable.scope_word, variable.scope) {
            (Some(word), _) => self.write(&format!("{}.", word.value)),
            (None, VariableScope::Session) => self.write("session."),
            (None, VariableScope::Global) => self.write("global."),
            (None, VariableScope::System | VariableScope::User) => {}
        }
        if let Some(prefix) = &variable.prefix {
            self.write(&format!("{}.", prefix.value));
        }
        let name = self.system_variable_name(&variable.name, span);
        self.ident(&name, span);
    }

    /// Writes the assignment `@name := value`.
    pub(super) fn assign(&mut self, expr: &Expr, name: &Ident, value: &Expr) {
        self.write("@");
        self.user_variable_name(name, Construct::EmptyUserVariableAssignment, expr.span);
        self.write(" := ");
        self.expr(value, 0);
    }

    /// Writes the name of a user variable after its `@`; an empty name is an
    /// error when the output has no `empty`.
    fn user_variable_name(&mut self, name: &Ident, empty: Construct, span: Option<Span>) {
        if name.value.is_empty() && !self.has(empty) {
            let message = format!("{} has no {}", self.target(), empty.label());
            self.error("variable.empty", &message, span);
        }
        if name.quoted {
            let escaped = name.value.replace('`', "``");
            self.write(&format!("`{escaped}`"));
        } else {
            self.write(&name.value);
        }
    }

    /// Name of the system variable `name` in the output: that of the other
    /// dialect for a variable of the transactions it lacks.
    fn system_variable_name(&mut self, name: &Ident, span: Option<Span>) -> Ident {
        for (old, new) in TRANSACTION_VARIABLES {
            let (from, to, construct) = if name.value.eq_ignore_ascii_case(old) {
                (old, new, Construct::TxVariables)
            } else if name.value.eq_ignore_ascii_case(new) {
                (new, old, Construct::TransactionVariables)
            } else {
                continue;
            };
            if self.has(construct) {
                return name.clone();
            }
            let message = format!(
                "{} has no system variable `{from}`: written `{to}`, of the same values",
                self.target()
            );
            self.step("variable.renamed", StepKind::Rewrite, message, span);
            return Ident {
                value: to.to_string(),
                quoted: name.quoted,
            };
        }
        name.clone()
    }
}
