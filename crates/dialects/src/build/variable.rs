//! User and system variables read in an expression, and the assignments of
//! user variables (`@x := 1`).
//!
//! A user variable takes the type of its value, read on the connection just
//! before the query runs, which the catalog gives (`docs/QUESTIONS.md`,
//! point 44); unknown otherwise. It is the type at the start of the
//! statement, the one the server reads even after an assignment in the same
//! statement (`SELECT @x := 'a', @x` reads `@x` as a number when it held
//! one). A system variable has a type unknown to Prisme.
//!
//! An assignment gives its value as the variable keeps it: a boolean as an
//! integer, a date or a time as text (in the character set of the connection
//! in MySQL, `latin1` in MariaDB: `@x := DATE '2024-05-06'` is a
//! `VARCHAR(10)`), a `JSON` as text (`utf8mb4_bin` in MySQL), a hexadecimal
//! or bit literal as a byte string. A `YEAR` keeps its type, not measured.

use prisme_plan::{Expr, Ident, Variable, VariableScope};
use prisme_types::{BinaryType, DataType, TextType, Type};

use super::scope::Scope;
use super::{BuildError, Builder};
use crate::Dialect;
use crate::sql::ast;

impl Builder<'_> {
    /// User variable `@name`, of the type the catalog gives.
    pub(super) fn user_variable(&self, name: &Ident) -> Expr {
        let ty = self
            .catalog
            .user_variable(&name.value)
            .unwrap_or_else(|| Type::nullable(DataType::Unknown));
        Expr::variable(Variable::user(name.clone()), ty)
    }

    /// Assignment `@name := value`.
    pub(super) fn assign(
        &mut self,
        name: &Ident,
        value: &ast::Expr,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let built = self.expr(value, scope)?;
        self.single(&built, value.span)?;
        let built = self.as_value(built);
        let ty = kept(&built.ty, self.dialect);
        Ok(Expr::assign(name.clone(), built, ty))
    }
}

/// System variable `@@x`, of a type unknown.
pub(super) fn system_variable(variable: &ast::SystemVariable) -> Expr {
    let scope = match &variable.scope {
        None => VariableScope::System,
        Some(word) if word.value.eq_ignore_ascii_case("GLOBAL") => VariableScope::Global,
        Some(_) => VariableScope::Session,
    };
    let variable = Variable {
        scope,
        scope_word: variable.scope.clone(),
        prefix: variable.prefix.clone(),
        name: variable.name.clone(),
    };
    Expr::variable(variable, Type::nullable(DataType::Unknown))
}

/// Type of a value of type `ty` as a user variable of `dialect` keeps it.
fn kept(ty: &Type, dialect: Dialect) -> Type {
    let text = |charset: Option<&str>, collation: Option<&str>| {
        DataType::Text(TextType {
            length: None,
            fixed: false,
            charset: charset.map(str::to_string),
            collation: collation.map(str::to_string),
        })
    };
    let data_type = match &ty.data_type {
        DataType::Date
        | DataType::Time { .. }
        | DataType::DateTime { .. }
        | DataType::Timestamp { .. } => match dialect {
            Dialect::MySql => text(None, None),
            Dialect::MariaDb => text(Some("latin1"), Some("latin1_swedish_ci")),
        },
        DataType::Json => match dialect {
            Dialect::MySql => text(Some("utf8mb4"), Some("utf8mb4_bin")),
            Dialect::MariaDb => text(None, None),
        },
        DataType::Hybrid { .. } | DataType::Bit { .. } => DataType::Binary(BinaryType {
            length: None,
            fixed: false,
        }),
        other => other.clone(),
    };
    Type {
        data_type,
        nullable: ty.nullable,
    }
}
