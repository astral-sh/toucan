//! Generic data-cache prefetch hints with compiler-specific constant requirements.

use lang_c::{ast, span::Node};
use serde::Serialize;
use toucan_target::Compiler;

use crate::x86::ImmediateStage;
use crate::{BuiltinFunction, Error, Type, TypeKind, analyze::Analyzer};

/// Optional operands of `__builtin_prefetch`. The hint does not read or write a
/// C object, imply volatile/atomic access, or authorize dropping argument effects.
/// A backend may omit the cache instruction while preserving those evaluations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum PrefetchHint {
    ReadWrite,
    Locality,
}
impl PrefetchHint {
    pub const ALL: [Self; 2] = [Self::ReadWrite, Self::Locality];

    /// Logical argument position after any GNU variadic-pack expansion; address
    /// is argument zero.
    pub fn argument(self) -> usize {
        match self {
            Self::ReadWrite => 1,
            Self::Locality => 2,
        }
    }
    /// Value used when this optional argument was omitted.
    pub fn default_value(self) -> u8 {
        match self {
            Self::ReadWrite => 0,
            Self::Locality => 3,
        }
    }
    /// Inclusive maximum; the minimum is zero. Read/write is zero for read and
    /// one for write; larger locality values request longer cache retention.
    pub fn maximum(self) -> u8 {
        match self {
            Self::ReadWrite => 1,
            Self::Locality => 3,
        }
    }
    /// Clang checks an ICE before lowering. GNU permits folding and inlining to
    /// produce a constant, including effects evaluated separately from its value.
    /// An unresolved GNU hint cannot be emitted merely because analysis succeeded.
    pub fn stage(self, compiler: Compiler) -> ImmediateStage {
        match compiler {
            Compiler::Gnu => ImmediateStage::AfterInlining,
            Compiler::Clang => ImmediateStage::Frontend,
        }
    }
    /// Interprets a proven constant's low 64 bits using the pinned compiler's
    /// hint policy. This operation does not change the retained operand type or
    /// establish that the source expression is constant. Values wider than 64
    /// bits expose implementation limits in Clang's assertion-enabled checker
    /// and LLVM lowering; source acceptance is not proof of valid machine code.
    pub fn normalized_value(self, value_bits: u128, compiler: Compiler) -> Option<u8> {
        let value = value_bits as u64;
        if value <= u64::from(self.maximum()) {
            Some(value as u8)
        } else {
            self.out_of_range_value(compiler)
        }
    }

    /// GNU diagnoses an out-of-range lowered constant and substitutes zero.
    /// Clang rejects it. The range applies after the 64-bit hint interpretation, without
    /// truncating an ordinary long integer to C int first.
    pub fn out_of_range_value(self, compiler: Compiler) -> Option<u8> {
        match compiler {
            Compiler::Gnu => Some(0),
            Compiler::Clang => None,
        }
    }
}

pub(crate) fn address_type() -> Type {
    let mut pointee = Type::new(TypeKind::Void);
    pointee.qualifiers.is_const = true;
    pointee.pointer()
}

impl crate::expression::ExpressionInfo {
    pub(crate) fn check_prefetch_value_operation(&self, offset: usize) -> Result<(), Error> {
        if self.is_prefetch_designator() {
            Err(Error::new(
                offset,
                "prefetch builtin does not permit this direct function-designator operation",
            ))
        } else {
            Ok(())
        }
    }
}

impl Analyzer {
    pub(crate) fn prefetch_call_type(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        let offset = call.span.start;
        let clang = self.unit.compiler == Compiler::Clang;
        if call.node.arguments.is_empty() || clang && call.node.arguments.len() > 3 {
            return Err(Error::new(
                offset,
                "__builtin_prefetch requires one to three arguments on Clang, and at least one on GNU",
            ));
        }
        if clang {
            // Share the lazy symbol/first-use state with other function builtins.
            self.mark_builtin_function_use(BuiltinFunction::Prefetch, offset)?;
        }
        let signature = self.builtin_function_type(BuiltinFunction::Prefetch)?;
        self.check_assignment(&signature.parameters[0].ty, &call.node.arguments[0])?;
        for (index, argument) in call.node.arguments.iter().enumerate().skip(1) {
            let ty = self.value_expression_type(argument)?;
            self.require_complete_object(&ty, argument.span.start)?;
            self.check_argument_pack(argument, index, call.node.arguments.len(), 1, true)?;
        }
        if clang {
            for hint in PrefetchHint::ALL {
                let Some(argument) = call.node.arguments.get(hint.argument()) else {
                    continue;
                };
                if !self.is_integer_constant_expression(argument, 0)? {
                    return Err(Error::new(
                        argument.span.start,
                        "prefetch hint requires an integer constant expression",
                    ));
                }
                let value = self.eval(argument)?;
                if hint
                    .normalized_value(value.value, Compiler::Clang)
                    .is_none()
                {
                    return Err(Error::new(
                        argument.span.start,
                        format!("prefetch hint must be in 0..={}", hint.maximum()),
                    ));
                }
            }
        }
        Ok(Type::new(TypeKind::Void))
    }
}

impl crate::checked::CheckedCode {
    /// Arguments of a known prefetch intrinsic call, including GNU function
    /// designators reached through indirection, comma/statement results or a
    /// checked generic/choose selection. The original promoted operand
    /// types remain authoritative; [`PrefetchHint`] describes lowering checks.
    ///
    /// Casts, address-taking and escaped pointer variables are deliberately not
    /// resolved. `None` does not establish that those forms can be lowered as
    /// ordinary calls: GCC rejects some explicit address/cast forms and may
    /// recover the builtin through optimization of an escaped pointer.
    pub fn prefetch_arguments(
        &self,
        call: crate::checked::ExprId,
    ) -> Option<&[crate::checked::ExprUse]> {
        use crate::checked::{Builtin, ExprKind, Unary};
        let (mut current, arguments) = match self.expression(call)?.kind() {
            ExprKind::BuiltinCall {
                builtin: Builtin::Prefetch,
                arguments,
                ..
            } => return Some(arguments),
            ExprKind::Call {
                callee, arguments, ..
            } => (callee.expression(), arguments),
            _ => return None,
        };
        // Owned expression edges are acyclic; this bound also keeps inspection
        // finite if a future graph producer changes that invariant.
        for _ in 0..self.expressions.len() {
            match self.expression(current)?.kind() {
                ExprKind::BuiltinFunction(BuiltinFunction::Prefetch) => return Some(arguments),
                ExprKind::Unary {
                    operator: Unary::Indirection,
                    operand,
                    ..
                } => current = operand.expression(),
                ExprKind::Comma(values) => current = values.last()?.expression(),
                ExprKind::StatementExpression {
                    result: Some(value),
                    ..
                } => current = value.expression(),
                ExprKind::Choose {
                    then_expression,
                    else_expression,
                    then_selected,
                    ..
                } => {
                    current = if *then_selected {
                        *then_expression
                    } else {
                        *else_expression
                    }
                }
                ExprKind::Generic { arms, selected, .. } => {
                    current = arms.get(*selected)?.expression
                }
                _ => return None,
            }
        }
        None
    }
}
