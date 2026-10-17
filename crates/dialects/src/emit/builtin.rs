//! Calls to scalar functions, and the names of the built-in functions.
//!
//! A built-in function of the input is written from the verdict of the
//! reference file of its dialect (`builtins` module): the function of the
//! same name of the output, another name, a rule of Prisme, or an explicit
//! error, the function being absent from the output, untranslatable or not
//! studied yet. The output must have the function in the version of the
//! server.
//!
//! A function stored in the database keeps its name. When the output has a
//! built-in function of that name, the call is written in a form that still
//! calls the stored function: a space before `(` or the name between
//! backticks, according to the grammar of the output; otherwise its database
//! must be named.

use prisme_plan::{Expr, FunctionCall, Ident, Span};

use super::Emitter;
use crate::builtins::{self, Kind, Rule, Translation};
use crate::report::StepKind;

/// How the name of a call is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameForm {
    /// `name(`.
    Plain,
    /// `name (`.
    Spaced,
    /// `` `name`( ``.
    Quoted,
}

/// How the output writes a built-in function of the input.
enum Target {
    /// Under this name, with the same arguments.
    Name(String),
    /// By this rule.
    Rule(Rule),
}

impl Emitter<'_> {
    /// Call to a scalar function, at level `min` at least, read as a value
    /// of its type when `column` (see `Emitter::column`).
    pub(super) fn function_call(
        &mut self,
        expr: &Expr,
        call: &FunctionCall,
        min: u8,
        column: bool,
    ) {
        if call.stored {
            self.stored_call(expr, call);
            return;
        }
        match self.builtin_target(&call.name, true, expr.span) {
            Some(Target::Name(name)) => {
                let form = self.builtin_form(&name, &call.name, expr.span);
                self.call_text(&name, form, &call.args);
            }
            Some(Target::Rule(rule)) => self.builtin_rule(rule, expr, call, min, column),
            // The error is recorded: the call is written as in the input.
            None => self.input_call(expr, call),
        }
    }

    /// Name under which the output writes the built-in aggregate or window
    /// function `name` of the input; the name of the input after an error.
    pub(super) fn builtin_name(&mut self, name: &Ident, span: Option<Span>) -> String {
        match self.builtin_target(name, false, span) {
            Some(Target::Name(written)) => written,
            Some(Target::Rule(rule)) => {
                self.rule_not_written(rule, name, span);
                name.value.clone()
            }
            None => name.value.clone(),
        }
    }

    /// How the output writes the built-in function `name` of the input,
    /// called without a window when `scalar`; `None` after an error.
    fn builtin_target(&mut self, name: &Ident, scalar: bool, span: Option<Span>) -> Option<Target> {
        let input = self.source.dialect;
        let output = self.output.dialect;
        let upper = name.value.to_ascii_uppercase();
        let of = format!("function `{upper}` of {}", input.label());
        let builtin = builtins::lookup(input, &upper);
        if input == output {
            return self
                .available(&upper, span)
                .then(|| Target::Name(name.value.clone()));
        }
        // Without a window, `LAST_VALUE` of MariaDB is a scalar function of
        // its own.
        if scalar && builtin.is_some_and(|builtin| builtin.kind != Kind::Scalar) {
            let message = format!("{of} without a window: absent from {}", output.label());
            self.error("function.absent", &message, span);
            return None;
        }
        let translation = builtin.and_then(|builtin| builtin.function.translation(output));
        match translation {
            None if builtins::lookup(output, &upper).is_some() => {
                let message = format!(
                    "{of}: its translation to {} is not studied yet by Prisme",
                    output.label()
                );
                self.error("function.unstudied", &message, span);
                None
            }
            None => {
                let message = format!("{of}: absent from {}", output.label());
                self.error("function.absent", &message, span);
                None
            }
            Some(Translation::Error(reason)) => {
                let message = format!("{of}: untranslatable to {}, {reason}", output.label());
                self.error("function.untranslatable", &message, span);
                None
            }
            Some(Translation::Rule(rule)) => Some(Target::Rule(*rule)),
            Some(Translation::Same) => self
                .available(&upper, span)
                .then(|| Target::Name(name.value.clone())),
            Some(Translation::Name(other)) => {
                let message = format!("{of}: named `{other}` in {}", output.label());
                self.step("function.name", StepKind::Rewrite, message, span);
                self.available(other, span)
                    .then(|| Target::Name(other.clone()))
            }
        }
    }

    /// Tells whether the server has the built-in function `name`, in
    /// capitals, in its version; records the error otherwise.
    fn available(&mut self, name: &str, span: Option<Span>) -> bool {
        let output = self.output.dialect;
        let Some(builtin) = builtins::lookup(output, name) else {
            let message = format!("function `{name}`: absent from {}", output.label());
            self.error("function.absent", &message, span);
            return false;
        };
        let function = builtin.function;
        let version = self.output.version;
        let message = match (function.since, function.until) {
            (Some(since), _) if version < since => format!(
                "function `{name}`: from {} {since}, server version {version}",
                output.label()
            ),
            (_, Some(until)) if version >= until => format!(
                "function `{name}`: removed in {} {until}, server version {version}",
                output.label()
            ),
            _ => return true,
        };
        self.error("function.version", &message, span);
        false
    }

    /// Call to a function stored in the database, in a form the output reads
    /// as a call to it.
    fn stored_call(&mut self, expr: &Expr, call: &FunctionCall) {
        let name = &call.name;
        let written = self.form(name, expr.span);
        if let Some(qualifier) = &call.qualifier {
            self.ident(qualifier, expr.span);
            self.write(".");
            self.call_text(&name.value, written, &call.args);
            return;
        }
        let upper = name.value.to_ascii_uppercase();
        let builtin = builtins::builtin(self.output.dialect, self.output.version, &upper);
        let Some(function) = builtin.map(|builtin| builtin.function) else {
            self.call_text(&name.value, written, &call.args);
            return;
        };
        let spaced = function.touching && !self.output.mode.ignore_space;
        let form = match written {
            NameForm::Quoted if function.grammar => NameForm::Quoted,
            NameForm::Spaced if spaced => NameForm::Spaced,
            NameForm::Plain if spaced => {
                let message = format!(
                    "`{upper}` is a built-in function of {}: a space before `(` calls the stored function",
                    self.target()
                );
                self.step("function.stored", StepKind::Rewrite, message, expr.span);
                NameForm::Spaced
            }
            _ if function.grammar => {
                let message = format!(
                    "`{upper}` is a built-in function of {}: written between backticks, the name calls the stored function",
                    self.target()
                );
                self.step("function.stored", StepKind::Rewrite, message, expr.span);
                NameForm::Quoted
            }
            _ => {
                let message = format!(
                    "function `{}` stored in the database: {} reads the name as its built-in function, the call needs the name of its database (`db.{}`)",
                    name.value,
                    self.target(),
                    name.value
                );
                self.error("function.stored", &message, expr.span);
                written
            }
        };
        self.call_text(&name.value, form, &call.args);
    }

    /// Form of `name` in the input of the call at `span`.
    pub(super) fn form(&self, name: &Ident, span: Option<Span>) -> NameForm {
        if name.quoted {
            return NameForm::Quoted;
        }
        // A space or a comment between the name and `(`.
        let spaced = span.is_some_and(|span| {
            let rest = self.source.text.get(span.start..).unwrap_or_default();
            rest.strip_prefix(name.value.as_str())
                .is_some_and(|after| !after.starts_with('('))
        });
        if spaced {
            NameForm::Spaced
        } else {
            NameForm::Plain
        }
    }

    /// Form under which the output reads `name`, the built-in function of
    /// the input `written` at `span`, as its built-in function: the form of
    /// the input when possible, recorded otherwise.
    pub(super) fn builtin_form(
        &mut self,
        name: &str,
        written: &Ident,
        span: Option<Span>,
    ) -> NameForm {
        let upper = name.to_ascii_uppercase();
        let function =
            builtins::lookup(self.output.dialect, &upper).map(|builtin| builtin.function);
        match self.form(written, span) {
            NameForm::Quoted if function.is_some_and(|function| function.grammar) => {
                let message = format!(
                    "`{upper}` between backticks calls a stored function in {}: written without them",
                    self.target()
                );
                self.step("function.quoted", StepKind::Rewrite, message, span);
                NameForm::Plain
            }
            NameForm::Spaced
                if function.is_some_and(|function| function.touching)
                    && !self.output.mode.ignore_space =>
            {
                let message = format!(
                    "`{upper}` followed by a space calls a stored function in {}: written without it",
                    self.target()
                );
                self.step("function.spaced", StepKind::Rewrite, message, span);
                NameForm::Plain
            }
            form => form,
        }
    }

    /// Writes `name(args)` in `form`.
    pub(super) fn call_text(&mut self, name: &str, form: NameForm, args: &[Expr]) {
        self.call_name(name, form);
        self.expr_list(args);
        self.write(")");
    }

    /// Writes `name(` in `form`.
    pub(super) fn call_name(&mut self, name: &str, form: NameForm) {
        match form {
            NameForm::Plain => self.write(name),
            NameForm::Spaced => {
                self.write(name);
                self.write(" ");
            }
            NameForm::Quoted => {
                let escaped = name.replace('`', "``");
                self.write(&format!("`{escaped}`"));
            }
        }
        self.write("(");
    }

    /// Records the error of a rule not written yet.
    pub(super) fn rule_not_written(&mut self, rule: Rule, name: &Ident, span: Option<Span>) {
        let message = format!(
            "function `{}` of {}: its translation to {} is not written yet in Prisme",
            name.value.to_ascii_uppercase(),
            self.source.dialect.label(),
            self.output.dialect.label()
        );
        self.error(rule.id(), &message, span);
    }
}
