//! C allocation builtins and the library symbols used by their GNU addresses.

use lang_c::{ast, span::Node};
use rustc_hash::FxHashMap;
use serde::Serialize;
use toucan_target::{Compiler, Target};

use crate::{
    BuiltinFunction, CallingConvention, Error, FunctionType, IntegerKind, Parameter, Type,
    TypeKind, analyze::Analyzer,
};

/// Allocation operations with the ordinary C library's parameter and result types.
/// Calls may modify allocation state and are not constant expressions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[non_exhaustive]
pub enum AllocationOperation {
    Malloc,
    Calloc,
    Realloc,
    Free,
}

pub(crate) struct Signature {
    pub(crate) result: Type,
    parameters: [Type; 2],
    arity: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct Evaluation {
    evaluated: bool,
    deferred: u8,
    unevaluated_depth: u16,
}
impl Default for Evaluation {
    fn default() -> Self {
        Self {
            evaluated: true,
            deferred: 0,
            unevaluated_depth: 0,
        }
    }
}

#[derive(Default)]
pub(crate) struct Symbols {
    file: [Option<String>; 5],
    scopes: FxHashMap<usize, [Option<String>; 5]>,
    bytes: usize,
}

impl Signature {
    pub(crate) fn parameters(&self) -> &[Type] {
        &self.parameters[..self.arity]
    }
}

impl AllocationOperation {
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::from_library_name(name.strip_prefix("__builtin_")?)
    }

    pub(crate) fn from_library_name(name: &str) -> Option<Self> {
        Some(match name {
            "malloc" => Self::Malloc,
            "calloc" => Self::Calloc,
            "realloc" => Self::Realloc,
            "free" => Self::Free,
            _ => return None,
        })
    }

    /// C library symbol denoted by an unshadowed GNU builtin function address.
    pub fn library_symbol(self) -> &'static str {
        match self {
            Self::Malloc => "malloc",
            Self::Calloc => "calloc",
            Self::Realloc => "realloc",
            Self::Free => "free",
        }
    }

    pub(crate) fn parameters(self, target: Target) -> Signature {
        let void = Type::new(TypeKind::Void);
        let pointer = void.clone().pointer();
        let size = Type::new(TypeKind::Integer(
            if target.long_width() == target.pointer_width() {
                IntegerKind::UnsignedLong
            } else {
                IntegerKind::UnsignedLongLong
            },
        ));
        let (parameters, arity) = match self {
            Self::Malloc => ([size, void.clone()], 1),
            Self::Calloc => ([size.clone(), size], 2),
            Self::Realloc => ([pointer.clone(), size], 2),
            Self::Free => ([pointer.clone(), void.clone()], 1),
        };
        Signature {
            result: if self == Self::Free { void } else { pointer },
            parameters,
            arity,
        }
    }

    /// Shared with C90 library-name admission; this method does not create names.
    pub(crate) fn signature(self, target: Target) -> FunctionType {
        let signature = self.parameters(target);
        FunctionType {
            parameters: signature
                .parameters()
                .iter()
                .cloned()
                .map(|ty| Parameter { name: None, ty })
                .collect(),
            return_type: signature.result,
            noreturn: false,
            parameter_contracts: None,
            variadic: false,
            prototype: true,
            calling_convention: CallingConvention::C,
        }
    }
}

impl<'ast> Analyzer<'ast> {
    pub(crate) fn allocation_context(&mut self, evaluated: bool) -> Evaluation {
        self.enter_dll_context(evaluated);
        let previous = self.allocation_evaluation;
        self.allocation_evaluation = Evaluation {
            evaluated,
            deferred: 0,
            unevaluated_depth: if evaluated {
                previous.unevaluated_depth
            } else {
                previous.unevaluated_depth.saturating_add(1)
            },
        };
        previous
    }

    pub(crate) fn restore_allocation_context(&mut self, previous: Evaluation, promote: bool) {
        self.restore_dll_context(promote);
        if promote {
            self.allocation_uses |= self.allocation_evaluation.deferred;
        }
        self.allocation_evaluation = previous;
    }

    pub(crate) fn finish_allocation_operand<T>(
        &mut self,
        previous: Evaluation,
        value: Result<T, Error>,
        promote: impl FnOnce(&Self, &T) -> Result<bool, Error>,
    ) -> Result<T, Error> {
        let evaluated = match &value {
            Ok(value) => promote(self, value),
            Err(_) => Ok(false),
        };
        self.restore_allocation_context(previous, matches!(&evaluated, Ok(true)));
        evaluated?;
        value
    }

    pub(crate) fn allocation_symbol(&self, operation: BuiltinFunction) -> &str {
        if let Some(symbols) = &self.allocation_symbols {
            for depth in (1..=self.lexical_scopes.len()).rev() {
                if let Some(Some(symbol)) = symbols
                    .scopes
                    .get(&depth)
                    .map(|entries| &entries[operation.index()])
                {
                    return symbol;
                }
            }
            if let Some(symbol) = &symbols.file[operation.index()] {
                return symbol;
            }
        }
        if self.unit.compiler == Compiler::Clang
            && let Some(link) = self
                .unit
                .declarations
                .iter()
                .find(|declaration| {
                    !declaration.is_static && declaration.name == operation.source_name()
                })
                .and_then(|declaration| declaration.link_name.as_deref())
        {
            return link;
        }
        operation.symbol()
    }

    pub(crate) fn leave_allocation_scope(&mut self) {
        if let Some(symbols) = &mut self.allocation_symbols
            && let Some(entries) = symbols.scopes.remove(&self.lexical_scopes.len())
        {
            symbols.bytes -= entries.iter().flatten().map(String::len).sum::<usize>();
        }
    }

    fn allocation_label(
        &mut self,
        operation: BuiltinFunction,
        label: Option<&str>,
        offset: usize,
    ) -> Result<String, Error> {
        let inherited = self.allocation_symbol(operation);
        if self.unit.compiler == Compiler::Gnu {
            return Ok(operation.symbol().to_owned());
        }
        let Some(label) = label else {
            return Ok(inherited.to_owned());
        };
        if self.allocation_evaluation.unevaluated_depth != 0 {
            return Err(Error::new(
                offset,
                if operation == BuiltinFunction::Prefetch {
                    "prefetch builtin asm declarations inside unevaluated operands are unsupported"
                } else {
                    "allocation builtin asm declarations inside unevaluated operands are unsupported"
                },
            ));
        }
        if inherited != operation.symbol() && inherited != label {
            return Err(Error::new(
                offset,
                "conflicting asm label for allocation builtin",
            ));
        }
        if self.allocation_uses & (1 << operation.index()) != 0 && inherited != label {
            return Err(Error::new(
                offset,
                "cannot apply asm label to allocation builtin after its first use",
            ));
        }
        let depth = self.lexical_scopes.len();
        let symbols = self.allocation_symbols.get_or_insert_with(Default::default);
        let entries = if depth == 0 {
            &mut symbols.file
        } else {
            symbols.scopes.entry(depth).or_default()
        };
        if entries[operation.index()].as_deref() != Some(label) {
            let old = entries[operation.index()].as_ref().map_or(0, String::len);
            let bytes = symbols.bytes - old + label.len();
            if bytes > 1_048_576 {
                return Err(Error::new(
                    offset,
                    "allocation builtin symbol names exceed the 1 MiB limit",
                ));
            }
            symbols.bytes = bytes;
            entries[operation.index()] = Some(label.to_owned());
        }
        Ok(label.to_owned())
    }

    /// Whether lookup still denotes the predefined function, rather than a
    /// local object, typedef, internal function, or GNU replacement prototype.
    pub(crate) fn builtin_function_reference(
        &self,
        name: &str,
    ) -> Result<Option<BuiltinFunction>, Error> {
        let Some(operation) = BuiltinFunction::from_name(name) else {
            return Ok(None);
        };
        for scope in self.lexical_scopes.iter().rev() {
            if scope.names.contains_key(name) {
                let Some(ty) = self.parameter_type(name) else {
                    return Ok(None);
                };
                if !scope.linked.contains(name)
                    || !matches!(self.unit.resolve(ty)?.kind, TypeKind::Function(_))
                {
                    return Ok(None);
                }
                if self
                    .unit
                    .declarations
                    .iter()
                    .any(|d| d.name == name && d.is_static)
                {
                    return Ok(None);
                }
                return Ok(self
                    .builtin_function_matches(operation, ty)?
                    .then_some(operation));
            }
        }
        if self.unit.constants.contains_key(name) || self.unit.typedefs.contains_key(name) {
            return Ok(None);
        }
        if let Some(declaration) = self.unit.declarations.iter().find(|d| d.name == name) {
            if declaration.is_static || declaration.kind != crate::DeclarationKind::Function {
                return Ok(None);
            }
            return Ok(self
                .builtin_function_matches(operation, &declaration.ty)?
                .then_some(operation));
        }
        Ok(Some(operation))
    }

    fn builtin_function_matches(
        &self,
        operation: BuiltinFunction,
        ty: &Type,
    ) -> Result<bool, Error> {
        let resolved = self.unit.resolve(ty)?;
        if operation == BuiltinFunction::Prefetch
            && self.unit.compiler == Compiler::Gnu
            && let TypeKind::Function(function) = &resolved.kind
            && function.prototype
            && function.variadic
            && function.parameters.len() == 1
            && function.calling_convention == CallingConvention::C
            && matches!(
                self.unit.resolve(&function.return_type)?.kind,
                TypeKind::Void
            )
            && let TypeKind::Pointer(pointee) = &self.unit.resolve(&function.parameters[0].ty)?.kind
            && matches!(self.unit.resolve(pointee)?.kind, TypeKind::Void)
        {
            return Ok(true);
        }
        self.compatible(
            ty,
            &Type::new(TypeKind::Function(Box::new(
                operation.signature(self.unit.target),
            ))),
        )
    }

    pub(crate) fn builtin_function_type(
        &self,
        operation: BuiltinFunction,
    ) -> Result<FunctionType, Error> {
        let name = operation.source_name();
        let ty = self.parameter_type(name).or_else(|| {
            self.unit
                .declarations
                .iter()
                .find(|d| d.name == name)
                .map(|d| &d.ty)
        });
        if operation == BuiltinFunction::Prefetch
            && let Some(ty) = ty
            && let TypeKind::Function(function) = &self.unit.resolve(ty)?.kind
        {
            return Ok((**function).clone());
        }
        Ok(operation.signature(self.unit.target))
    }

    pub(crate) fn mark_builtin_function_use(
        &mut self,
        operation: BuiltinFunction,
        offset: usize,
    ) -> Result<(), Error> {
        if self.unit.compiler == Compiler::Clang {
            self.record_dll_use(operation.source_name(), offset)?;
            if self.allocation_evaluation.evaluated {
                self.allocation_uses |= 1 << operation.index();
            } else {
                self.allocation_evaluation.deferred |= 1 << operation.index();
            }
        }
        Ok(())
    }

    pub(crate) fn allocation_call_type(
        &mut self,
        operation: AllocationOperation,
        call: &Node<ast::CallExpression>,
    ) -> Result<Type, Error> {
        self.mark_builtin_function_use(BuiltinFunction::Allocation(operation), call.span.start)?;
        let signature = operation.parameters(self.unit.target);
        if call.node.arguments.len() != signature.arity {
            return Err(Error::new(
                call.span.start,
                format!(
                    "__builtin_{} requires {} arguments",
                    operation.library_symbol(),
                    signature.arity
                ),
            ));
        }
        for (argument, parameter) in call.node.arguments.iter().zip(signature.parameters()) {
            self.check_assignment(parameter, argument)?;
        }
        Ok(signature.result)
    }

    /// Complete compatible builtin prototypes and preserve compiler-specific
    /// replacement rules. Internal declarations use the ordinary C namespace.
    pub(crate) fn builtin_function_declaration(
        &mut self,
        name: &str,
        ty: &mut Type,
        external: bool,
        definition: bool,
        link_name: &mut Option<String>,
        offset: usize,
    ) -> Result<bool, Error> {
        if !external {
            return Ok(false);
        }
        let Some(operation) = BuiltinFunction::from_name(name) else {
            return Ok(false);
        };
        let mut resolved = self.unit.resolve(ty)?.clone();
        let TypeKind::Function(function) = &mut resolved.kind else {
            if self.unit.compiler == Compiler::Clang {
                return Err(Error::new(
                    offset,
                    format!("declaration conflicts with builtin function `{name}`"),
                ));
            }
            return Ok(false);
        };
        if self.unit.compiler == Compiler::Clang {
            if definition {
                return Err(Error::new(offset, "definition of a builtin function"));
            }
            // Clang diagnoses and ignores convention annotations on its builtin.
            function.calling_convention = CallingConvention::C;
        }
        let expected = Type::new(TypeKind::Function(Box::new(
            operation.signature(self.unit.target),
        )));
        let prototype = function.prototype;
        if !self.builtin_function_matches(operation, &resolved)? {
            if self.unit.compiler == Compiler::Clang {
                return Err(Error::new(
                    offset,
                    format!("conflicting type for builtin function `{name}`"),
                ));
            }
            return Ok(false);
        }
        if definition && !prototype {
            return Err(Error::new(
                offset,
                "identifier-list definitions of allocation builtins are unsupported",
            ));
        }
        *ty = if operation == BuiltinFunction::Prefetch {
            resolved
        } else {
            crate::noescape::composite_type!(self, &resolved, &expected, 0)?
        };
        *link_name = Some(self.allocation_label(operation, link_name.as_deref(), offset)?);
        Ok(true)
    }
}
