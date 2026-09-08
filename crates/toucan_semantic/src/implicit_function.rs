//! C90 declarations implied by calls to undeclared ordinary function names.

use lang_c::{ast, span::Node};

use crate::analyze::{Analyzer, BlockExtern};
use crate::{
    CallingConvention, Declaration, DeclarationKind, Error, FunctionType, IntegerKind,
    SymbolBinding, Type, TypeKind,
};

impl Analyzer {
    /// Installs the equivalent of `extern int name()` in the innermost scope.
    /// Parenthesized names are not eligible, and an existing local name shadows
    /// an external function just as it does for an explicit declaration.
    pub(crate) fn implicit_function(
        &mut self,
        call: &Node<ast::CallExpression>,
    ) -> Result<(), Error> {
        if !self.unit.language_mode.is_c90() {
            return Ok(());
        }
        let ast::Expression::Identifier(identifier) = &call.node.callee.node else {
            return Ok(());
        };
        if identifier.span.start != call.span.start {
            return Ok(());
        }
        let Some(name) = self.builtin_name(call) else {
            return Ok(());
        };
        if name.starts_with("__builtin_") {
            return Err(Error::new(
                call.span.start,
                format!("unknown or unsupported builtin `{name}`"),
            ));
        }
        if name.starts_with('_') {
            return Err(Error::new(
                call.span.start,
                format!(
                    "implicit declaration of reserved function name `{name}` is unsupported; supply its declaration"
                ),
            ));
        }
        if library_name(name) {
            return Err(Error::new(
                call.span.start,
                format!(
                    "implicit declaration of library builtin `{name}` is unsupported; supply its declaration"
                ),
            ));
        }
        let ty = Type::new(TypeKind::Function(Box::new(FunctionType {
            noreturn: false,
            parameter_contracts: None,
            return_type: Type::new(TypeKind::Integer(IntegerKind::Int)),
            parameters: Vec::new(),
            variadic: false,
            prototype: false,
            calling_convention: CallingConvention::C,
        })));
        if let Some(previous) = self.block_externs.get(name)
            && (!self.compatible(&previous.ty, &ty)? || previous.thread_local)
        {
            return Err(Error::new(
                call.span.start,
                "implicit function conflicts with an out-of-scope declaration; compiler warning recovery is unsupported",
            ));
        }
        self.dll_implicit_function(name);
        let name = name.to_owned();
        if self.lexical_scopes.is_empty() {
            self.unit.declarations.push(Declaration {
                function_definition_kind: None,
                dll_storage_class: None,
                alignment: Default::default(),
                returns_twice: false,
                noreturn: false,
                symbol_binding: SymbolBinding::Strong,
                name: name.clone(),
                ty: ty.clone(),
                kind: DeclarationKind::Function,
                link_name: None,
                is_static: false,
                is_thread_local: false,
                is_definition: false,
                flexible_array_storage: None,
            });
        } else {
            self.bind_local(&name, ty.clone(), true, false, call.span.start)?;
            self.lexical_scopes
                .last_mut()
                .expect("current scope")
                .linked
                .insert(name.clone());
        }
        self.block_externs.insert(
            name.clone(),
            BlockExtern {
                noreturn: false,
                alignment: Default::default(),
                ty: ty.clone(),
                thread_local: false,
                is_static: false,
            },
        );
        if let Some(checked) = &mut self.checked {
            checked.implicit_function_declaration(&call.node.callee, &name, &ty)?;
        }
        Ok(())
    }
}

/// Library builtin signatures depend on compiler, mode, and target. Until those
/// implicit signatures are modeled, their catalogued names require declarations.
/// The conservative union includes names disabled in individual profiles.
fn library_name(name: &str) -> bool {
    LIBRARY_NAMES.binary_search(&name).is_ok()
}

include!("implicit_library_names.rs");
