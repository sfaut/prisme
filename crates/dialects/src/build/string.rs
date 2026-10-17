//! Functions of strings whose arguments have a syntax of their own: `TRIM`
//! and `POSITION` in the plan, typed as unknown as the other calls until the
//! types of the function catalog (`call` module).

use prisme_plan::{Expr, Position, Trim};
use prisme_types::{DataType, Type};

use super::scope::Scope;
use super::{BuildError, Builder};
use crate::sql::ast;

impl Builder<'_> {
    /// String without a pattern at its start or its end.
    pub(super) fn trim(&mut self, call: &ast::TrimCall, scope: &Scope) -> Result<Expr, BuildError> {
        let pattern = call
            .pattern
            .as_ref()
            .map(|pattern| self.value(pattern, scope))
            .transpose()?;
        let trim = Trim {
            side: call.side,
            pattern,
            operand: self.value(&call.operand, scope)?,
        };
        Ok(Expr::trim(trim, Type::nullable(DataType::Unknown)))
    }

    /// Position of a substring in a string.
    pub(super) fn position(
        &mut self,
        call: &ast::PositionCall,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let position = Position {
            substring: self.value(&call.substring, scope)?,
            string: self.value(&call.string, scope)?,
        };
        Ok(Expr::position(position, Type::nullable(DataType::Unknown)))
    }
}
