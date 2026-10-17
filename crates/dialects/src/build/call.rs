//! Function calls of the MySQL family in the plan: scalar functions, built-in
//! or stored in the database (typed as unknown until the types of the
//! function catalog, those of the current date and time aside, `now`),
//! aggregate functions (read by the aggregation of their block) and window
//! functions (read by the window node of their block), with their named
//! windows.

use prisme_plan::{
    AggregateCall, Direction, Expr, Frame, FrameBound, FunctionCall, Ident, Nulls, OnEmpty,
    SortForm, SortKey, SortReference, Span, WindowCall, WindowDefinition,
};
use prisme_types::{DataType, Type};

use super::group::Clause;
use super::scope::Scope;
use super::{BuildError, Builder, functions, now, same_name};
use crate::Dialect;
use crate::builtins::{self, Kind};
use crate::sql::ast;

/// Parts of a window: partition, order, frame.
type WindowParts = (Vec<Expr>, Vec<SortKey>, Option<Frame>);

impl Builder<'_> {
    /// Call to a function.
    pub(super) fn call(
        &mut self,
        call: &ast::FunctionCall,
        span: Span,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let name = call.upper_name();
        if call.over.is_some() {
            return self.window_call(call, &name, span, scope);
        }
        if call.is_aggregate() {
            return self.aggregate_call(call, &name, span, scope);
        }
        let kind = (!call.stored)
            .then(|| builtins::builtin(self.dialect, self.version, &name))
            .flatten()
            .map(|builtin| builtin.kind);
        match kind {
            Some(Kind::Aggregate) => {
                let message = format!("aggregate function `{name}`: not supported yet by Prisme");
                return Err(self.error(message, span));
            }
            // Without a window, `LAST_VALUE` of MariaDB is also a scalar
            // function: the last of its arguments.
            Some(Kind::Window) if !(self.dialect == Dialect::MariaDb && name == "LAST_VALUE") => {
                return Err(self.error(format!("`{name}` needs a window (`OVER`)"), span));
            }
            _ => {}
        }
        let args = call
            .args
            .iter()
            .map(|arg| self.value(arg, scope))
            .collect::<Result<Vec<_>, _>>()?;
        let (qualifier, function) = match call.name.parts.as_slice() {
            [qualifier, function] => (Some(qualifier.clone()), function.clone()),
            parts => (None, parts[parts.len() - 1].clone()),
        };
        let ty = kind
            .is_some()
            .then(|| now::now_type(&name, &args, self.dialect, self.version))
            .flatten()
            .unwrap_or_else(|| Type::nullable(DataType::Unknown));
        let call = FunctionCall {
            qualifier,
            name: function,
            args,
            stored: call.stored,
        };
        Ok(Expr::function(call, ty).with_span(span))
    }

    /// Call to an aggregate function, read by the aggregation of its block.
    fn aggregate_call(
        &mut self,
        call: &ast::FunctionCall,
        name: &str,
        span: Span,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let block = self.block();
        let allowed = block.grouped
            && matches!(
                block.clause,
                Clause::Select
                    | Clause::Having
                    | Clause::OrderBy
                    | Clause::Window
                    | Clause::WindowFunction
            );
        if !allowed {
            let message = format!(
                "invalid use of the aggregate function `{name}` in {}",
                block.clause.label()
            );
            return Err(self.error(message, span));
        }
        let previous = self.set_clause(Clause::Aggregate);
        let built = self.aggregate_parts(call, scope);
        self.set_clause(previous);
        let (args, order_by) = built?;
        let (data_type, on_empty) = functions::aggregate(name, &args, self.dialect);
        let function = call.name.parts[0].clone();
        let aggregate = AggregateCall {
            function,
            args,
            distinct: call.distinct,
            order_by,
            separator: call.separator.clone(),
            implicit: false,
            data_type,
            on_empty,
            span: Some(span),
        };
        Ok(self.block_mut().aggregate(aggregate).with_span(span))
    }

    /// Arguments and order of an aggregate function.
    fn aggregate_parts(
        &mut self,
        call: &ast::FunctionCall,
        scope: &Scope,
    ) -> Result<(Vec<Expr>, Vec<SortKey>), BuildError> {
        let args = call
            .args
            .iter()
            .map(|arg| self.value(arg, scope))
            .collect::<Result<Vec<_>, _>>()?;
        let order_by = self.expression_keys(&call.order_by, scope)?;
        Ok((args, order_by))
    }

    /// Call to a window function, read by the window node of its block.
    fn window_call(
        &mut self,
        call: &ast::FunctionCall,
        name: &str,
        span: Span,
        scope: &Scope,
    ) -> Result<Expr, BuildError> {
        let clause = self.block().clause;
        if !matches!(clause, Clause::Select | Clause::OrderBy) {
            let message = format!("window function `{name}` in {}", clause.label());
            return Err(self.error(message, span));
        }
        let aggregate = call.is_aggregate();
        if !aggregate && !(call.name.parts.len() == 1 && functions::is_window_function(name)) {
            let message = format!("`{name}` with `OVER`: not supported yet by Prisme");
            return Err(self.error(message, span));
        }
        if call.distinct || !call.order_by.is_empty() {
            let message = format!("`DISTINCT` and `ORDER BY` in the window function `{name}`");
            return Err(self.error(message, span));
        }
        let over = call.over.as_ref().expect("a window function has a window");
        let previous = self.set_clause(Clause::WindowFunction);
        let built = self.window_parts_of_call(call, over, scope);
        self.set_clause(previous);
        let (args, (partition_by, order_by, frame)) = built?;
        let ty = if aggregate {
            let (data_type, on_empty) = functions::aggregate(name, &args, self.dialect);
            Type {
                data_type,
                nullable: on_empty == OnEmpty::Null,
            }
        } else {
            functions::window(name, &args, self.dialect)
        };
        let window = WindowCall {
            function: call.name.parts[0].clone(),
            args,
            partition_by,
            order_by,
            frame,
            window: over.base.clone(),
        };
        let expr = Expr::window(window, ty).with_span(span);
        Ok(self.block_mut().window(expr).with_span(span))
    }

    /// Arguments and window of a window function.
    fn window_parts_of_call(
        &mut self,
        call: &ast::FunctionCall,
        over: &ast::WindowSpec,
        scope: &Scope,
    ) -> Result<(Vec<Expr>, WindowParts), BuildError> {
        let args = call
            .args
            .iter()
            .map(|arg| self.value(arg, scope))
            .collect::<Result<Vec<_>, _>>()?;
        let window = self.over(over, scope)?;
        Ok((args, window))
    }

    /// Window of a call: its own parts, those of the named window it uses
    /// (`OVER w`), or those of the named window it completes
    /// (`OVER (w ORDER BY a)`).
    fn over(&mut self, spec: &ast::WindowSpec, scope: &Scope) -> Result<WindowParts, BuildError> {
        let own = self.window_parts(spec, scope)?;
        let Some(base) = &spec.base else {
            return Ok(own);
        };
        let named = self.named_window(base, spec.span, 0)?;
        // `OVER w`, without parentheses, uses the window as defined, its
        // frame included.
        if !self.written(spec.span).starts_with('(') {
            return Ok(named);
        }
        self.completed(base, spec.span, own, named)
    }

    /// Parts of a window that completes the named window `base`, of parts
    /// `named`: it adds an order when `base` has none, and a frame; `base`
    /// has no frame (the rule of MySQL and MariaDB).
    fn completed(
        &self,
        base: &Ident,
        span: Span,
        own: WindowParts,
        named: WindowParts,
    ) -> Result<WindowParts, BuildError> {
        let (partition_by, order_by, frame) = own;
        let (named_partition, named_order, named_frame) = named;
        let message = if !partition_by.is_empty() {
            Some(format!(
                "a window that completes window `{}` has no `PARTITION BY` of its own",
                base.value
            ))
        } else if !order_by.is_empty() && !named_order.is_empty() {
            Some(format!(
                "window `{}` has an `ORDER BY`: a window that completes it has none of its own",
                base.value
            ))
        } else if named_frame.is_some() {
            Some(format!(
                "window `{}` has a frame: no window can complete it",
                base.value
            ))
        } else {
            None
        };
        if let Some(message) = message {
            return Err(self.error(message, span));
        }
        let order_by = if order_by.is_empty() {
            named_order
        } else {
            order_by
        };
        Ok((named_partition, order_by, frame))
    }

    /// Parts of the named window `name`, completed by those of the window it
    /// completes, if any.
    fn named_window(
        &self,
        name: &Ident,
        span: Span,
        depth: usize,
    ) -> Result<WindowParts, BuildError> {
        let definitions = &self.block().definitions;
        if depth > definitions.len() {
            return Err(self.error("the named windows complete each other in a cycle", span));
        }
        let definition = definitions
            .iter()
            .find(|definition| same_name(&definition.name, name))
            .ok_or_else(|| self.error(format!("unknown window `{}`", name.value), span))?;
        let own = (
            definition.partition_by.clone(),
            definition.order_by.clone(),
            definition.frame.clone(),
        );
        let Some(base) = definition.base.clone() else {
            return Ok(own);
        };
        let named = self.named_window(&base, span, depth + 1)?;
        self.completed(&base, span, own, named)
    }

    /// Parts written in a window.
    fn window_parts(
        &mut self,
        spec: &ast::WindowSpec,
        scope: &Scope,
    ) -> Result<WindowParts, BuildError> {
        let partition_by = spec
            .partition_by
            .iter()
            .map(|value| self.value(value, scope))
            .collect::<Result<Vec<_>, _>>()?;
        let order_by = self.expression_keys(&spec.order_by, scope)?;
        let frame = match &spec.frame {
            Some(frame) => Some(Frame {
                units: frame.units,
                start: self.frame_bound(&frame.start, scope)?,
                end: match &frame.end {
                    Some(end) => Some(self.frame_bound(end, scope)?),
                    None => None,
                },
            }),
            None => None,
        };
        Ok((partition_by, order_by, frame))
    }

    /// Bound of a frame.
    fn frame_bound(
        &mut self,
        bound: &ast::FrameBound,
        scope: &Scope,
    ) -> Result<FrameBound, BuildError> {
        Ok(match bound {
            ast::FrameBound::UnboundedPreceding => FrameBound::UnboundedPreceding,
            ast::FrameBound::Preceding(value) => {
                FrameBound::Preceding(Box::new(self.value(value, scope)?))
            }
            ast::FrameBound::CurrentRow => FrameBound::CurrentRow,
            ast::FrameBound::Following(value) => {
                FrameBound::Following(Box::new(self.value(value, scope)?))
            }
            ast::FrameBound::UnboundedFollowing => FrameBound::UnboundedFollowing,
        })
    }

    /// Keys of an `ORDER BY` of a window or of an aggregate function: each
    /// one an expression, a number being a constant there.
    pub(super) fn expression_keys(
        &mut self,
        items: &[ast::OrderItem],
        scope: &Scope,
    ) -> Result<Vec<SortKey>, BuildError> {
        items
            .iter()
            .map(|item| {
                let direction = item.direction.unwrap_or(Direction::Ascending);
                Ok(SortKey {
                    expr: self.value(&item.expr, scope)?,
                    direction,
                    nulls: Nulls::as_smallest(direction),
                    form: SortForm {
                        direction: item.direction.is_some(),
                        nulls: false,
                        reference: SortReference::Expression,
                    },
                })
            })
            .collect()
    }

    /// Named windows of the `WINDOW` clause, each one checked against the
    /// window it completes: MySQL reads it anywhere in the clause, MariaDB
    /// before.
    pub(super) fn window_definitions(
        &mut self,
        windows: &[ast::NamedWindow],
        scope: &Scope,
    ) -> Result<(), BuildError> {
        for window in windows {
            if self
                .block()
                .definitions
                .iter()
                .any(|definition| same_name(&definition.name, &window.name))
            {
                let message = format!("window `{}` is defined twice", window.name.value);
                return Err(self.error(message, window.spec.span));
            }
            let (partition_by, order_by, frame) = self.window_parts(&window.spec, scope)?;
            self.block_mut().definitions.push(WindowDefinition {
                name: window.name.clone(),
                base: window.spec.base.clone(),
                partition_by,
                order_by,
                frame,
            });
        }
        for (index, window) in windows.iter().enumerate() {
            if let Some(base) = &window.spec.base
                && self.dialect == Dialect::MariaDb
                && !windows[..index]
                    .iter()
                    .any(|before| same_name(&before.name, base))
            {
                let message = format!(
                    "window `{}` is not defined before window `{}`",
                    base.value, window.name.value
                );
                return Err(self.error(message, window.spec.span));
            }
            self.named_window(&window.name, window.spec.span, 0)?;
        }
        Ok(())
    }
}
