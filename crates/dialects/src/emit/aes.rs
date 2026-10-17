//! `AES_ENCRYPT` and `AES_DECRYPT` between MySQL and MariaDB. With a string
//! and a key, both encrypt alike in the mode `aes-128-ecb`, the default of
//! the variable `block_encryption_mode`, which Prisme leaves to the session.
//! MySQL takes an initialization vector, third argument, which that mode
//! ignores, and from 8.0.30 a key derivation, fourth to sixth arguments;
//! MariaDB takes the vector and a mode, fourth argument, from 11.2.1
//! (construct `aes_iv`).
//!
//! Measured on MySQL 8.0.11, 8.0.28 and 8.4, MariaDB 10.6, 11.1, 11.2, 11.4
//! and 11.8.

use prisme_plan::{Expr, ExprKind, FunctionCall};
use prisme_types::Value;

use super::Emitter;
use crate::Dialect;
use crate::builtins::Rule;
use crate::report::StepKind;
use crate::versions::Construct;

/// The default of `block_encryption_mode` in both dialects.
const DEFAULT_MODE: &str = "aes-128-ecb";

impl Emitter<'_> {
    /// `AES_ENCRYPT(string, key, ...)` and `AES_DECRYPT(string, key, ...)`:
    /// natively with two arguments; the vector left out towards a MariaDB
    /// without it, the mode `aes-128-ecb` left out towards MySQL; a key
    /// derivation, or another mode, is an error.
    pub(super) fn aes(&mut self, rule: Rule, expr: &Expr, call: &FunctionCall) {
        let name = call.name.value.to_ascii_uppercase();
        let target = self.target();
        let kept = match (call.args.len(), self.output.dialect) {
            // A call of MariaDB with more arguments, or with too few: the
            // server reports the number of arguments.
            (0..=2, _) | (3, Dialect::MySql) | (5.., Dialect::MySql) => None,
            (3, Dialect::MariaDb) if self.has(Construct::AesIv) => None,
            (3, Dialect::MariaDb) => {
                let message = format!(
                    "{target} takes no initialization vector in `{name}`: MySQL ignores it in the mode `{DEFAULT_MODE}`, the default of `block_encryption_mode`, which Prisme leaves to the session; left out, without evaluating it"
                );
                self.step(rule.id(), StepKind::Rewrite, message, expr.span);
                Some(2)
            }
            (_, Dialect::MariaDb) => {
                let message = format!(
                    "`{name}` with a key derivation (`hkdf`, `pbkdf2_hmac`), fourth to sixth arguments of MySQL: {target} has none, its fourth argument is the mode"
                );
                self.error(rule.id(), &message, expr.span);
                None
            }
            (4, Dialect::MySql) => {
                let default = matches!(&call.args[3].kind,
                    ExprKind::Literal(Value::Text(mode)) if mode.eq_ignore_ascii_case(DEFAULT_MODE));
                if default {
                    let message = format!(
                        "{target} takes the mode of `{name}` from the variable `block_encryption_mode`, `{DEFAULT_MODE}` by default, which Prisme leaves to the session: the mode `{DEFAULT_MODE}` left out"
                    );
                    self.step(rule.id(), StepKind::Rewrite, message, expr.span);
                    Some(3)
                } else {
                    let message = format!(
                        "`{name}` with a mode other than `{DEFAULT_MODE}`, or unknown: {target} takes the mode from the variable `block_encryption_mode`, never from an argument"
                    );
                    self.error(rule.id(), &message, expr.span);
                    None
                }
            }
        };
        let args = &call.args[..kept.unwrap_or(call.args.len())];
        let form = self.form(&call.name, expr.span);
        self.call_text(&call.name.value, form, args);
    }
}
