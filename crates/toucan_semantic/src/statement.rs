use std::collections::{BTreeMap, HashSet};

use lang_c::{ast, span::Node};

use crate::analyze::{Analyzer, LexicalScope, Tag};
use crate::checked::statement::ControlKind;
use crate::checked::{
    EntityKind, LocalDeclaration, OccurrenceKind, ScopeId, ScopeKind, Storage, declarator_name_span,
};
use crate::expression::ExpressionInfo;
use crate::integer::{convert, promote};
use crate::{DeclarationKind, Error, FunctionType, IntegerValue, Parameter, Scope, Type, TypeKind};

/// Bindings introduced in a definition's parameter list remain visible in its body.
pub(crate) struct FunctionScope {
    pub(crate) checked_scope: Option<ScopeId>,
    pub(crate) record_ids: Vec<usize>,
    pub(crate) enum_ids: Vec<usize>,
    pub(crate) tags: Vec<(String, Tag)>,
    pub(crate) constants: Vec<(String, IntegerValue)>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) register: HashSet<String>,
}

pub(crate) struct FunctionContext {
    signature: FunctionType,
    parameter_scope: usize,
    statement_expressions: BTreeMap<(usize, usize), ExpressionInfo>,
    expression_parents: Vec<Option<usize>>,
    active_expression: Option<usize>,
    loops: usize,
    switches: Vec<SwitchContext>,
    labels: BTreeMap<String, JumpScope>,
    gotos: Vec<(String, usize, JumpScope)>,
    fallthrough: Vec<(usize, usize)>,
}

#[derive(Clone, Copy)]
struct JumpScope {
    variably_modified: Option<usize>,
    statement_expression: Option<usize>,
}

struct SwitchContext {
    offset: usize,
    ty: IntegerValue,
    ranges: BTreeMap<u128, u128>,
    has_default: bool,
    variably_modified: Option<usize>,
    statement_expression: Option<usize>,
}

impl Analyzer {
    fn function_context(&self) -> &FunctionContext {
        self.current_function.as_ref().expect("function context")
    }

    fn function_context_mut(&mut self) -> &mut FunctionContext {
        self.current_function.as_mut().expect("function context")
    }

    pub(crate) fn current_function_signature(&self) -> Option<&FunctionType> {
        self.current_function
            .as_ref()
            .map(|context| &context.signature)
    }

    pub(crate) fn current_function_parameter(&self, name: &str) -> Option<&Parameter> {
        let context = self.current_function.as_ref()?;
        let (scope, _) = self
            .lexical_scopes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, scope)| scope.names.contains_key(name))?;
        if scope != context.parameter_scope {
            return None;
        }
        context
            .signature
            .parameters
            .iter()
            .find(|parameter| parameter.name.as_deref() == Some(name))
    }

    fn jump_scope(&self) -> JumpScope {
        JumpScope {
            variably_modified: self.active_variably_modified(),
            statement_expression: self.function_context().active_expression,
        }
    }

    fn gnu_statement_expressions(&self) -> bool {
        matches!(
            self.unit.target,
            toucan_target::Target::X86_64UnknownLinuxGnu
                | toucan_target::Target::Aarch64UnknownLinuxGnu
        )
    }

    /// Checks the block in the enclosing function's control-flow environment.
    /// Repeated type queries reuse the result so labels and VM scopes are visited
    /// once, while the block's lexical bindings are restored before returning.
    pub(crate) fn statement_expression(
        &mut self,
        statement: &Node<ast::Statement>,
    ) -> Result<ExpressionInfo, Error> {
        let offset = statement.span.start;
        let key = (offset, statement.span.end);
        let context = self
            .current_function
            .as_mut()
            .ok_or_else(|| Error::new(offset, "statement expressions require a function body"))?;
        if let Some(result) = context.statement_expressions.get(&key) {
            return Ok(result.clone());
        }
        let previous = context.active_expression;
        let id = context.expression_parents.len();
        context.expression_parents.push(previous);
        context.active_expression = Some(id);
        let outer_fallthrough = std::mem::take(&mut context.fallthrough);
        let result = self.with_block(statement.span, |analyzer| {
            let ast::Statement::Compound(items) = &statement.node else {
                return Err(Error::new(
                    offset,
                    "statement expression requires a compound statement",
                ));
            };
            let checked_statement = analyzer
                .checked
                .as_mut()
                .map(|checked| checked.begin_statement(statement))
                .transpose()?
                .flatten();
            let gnu = analyzer.gnu_statement_expressions();
            let mut result = None;
            let mut final_value = None;
            let mut significant = 0;
            let mut direct_expression = false;
            for item in items {
                match &item.node {
                    ast::BlockItem::Declaration(declaration) => {
                        analyzer.block_declaration(declaration, false)?;
                        result = None;
                        final_value = None;
                        significant += 1;
                    }
                    ast::BlockItem::StaticAssert(assertion) => {
                        analyzer.static_assert(assertion)?;
                        if !gnu {
                            result = None;
                            final_value = None;
                        }
                    }
                    ast::BlockItem::Statement(statement) => {
                        if matches!(statement.node, ast::Statement::Expression(None)) {
                            if analyzer.checked.is_some() {
                                analyzer.statement(statement)?;
                            }
                            continue;
                        }
                        analyzer.statement(statement)?;
                        significant += 1;
                        direct_expression =
                            matches!(statement.node, ast::Statement::Expression(Some(_)));
                        final_value = final_expression(statement);
                        result = final_value
                            .map(|expression| analyzer.expression_info(expression))
                            .transpose()?;
                    }
                }
            }
            analyzer.require_no_fallthrough()?;
            if let Some(checked) = &mut analyzer.checked {
                checked.statement_expression_result(statement, final_value)?;
            }
            if let Some(id) = checked_statement {
                analyzer.retain_statement(statement, id)?;
            }
            let Some(result) = result else {
                return Ok(ExpressionInfo::value(Type::new(TypeKind::Void)));
            };
            if gnu && result.bitfield.is_some() {
                return Err(Error::new(
                    offset,
                    "GCC statement-expression bit-field result types are unsupported",
                ));
            }
            let ty = analyzer.converted_type(&result, offset)?;
            if gnu
                && significant == 1
                && direct_expression
                && result.lvalue
                && analyzer.unit.qualifiers(&result.ty)? == crate::Qualifiers::default()
                && !matches!(
                    analyzer.unit.resolve(&result.ty)?.kind,
                    TypeKind::Array { .. } | TypeKind::VariableArray { .. } | TypeKind::Function(_)
                )
            {
                Ok(result)
            } else {
                Ok(ExpressionInfo::value(ty))
            }
        });
        let context = self.function_context_mut();
        context.active_expression = previous;
        context.fallthrough = outer_fallthrough;
        if let Ok(result) = &result {
            context.statement_expressions.insert(key, result.clone());
        }
        result
    }

    pub(crate) fn in_function_body(&self) -> bool {
        self.lexical_scopes.iter().any(|scope| scope.is_block)
    }

    pub(crate) fn object_has_static_storage(&self, name: &str) -> bool {
        for scope in self.lexical_scopes.iter().rev() {
            if scope.names.contains_key(name) {
                return scope.static_storage.contains(name);
            }
        }
        self.unit.declarations.iter().any(|declaration| {
            declaration.name == name && declaration.kind != DeclarationKind::Typedef
        })
    }

    pub(crate) fn is_register_object(&self, name: &str) -> bool {
        for scope in self.lexical_scopes.iter().rev() {
            if scope.names.contains_key(name) {
                return scope.register.contains(name);
            }
        }
        false
    }

    /// Local aliases retain their resolved types in their lexical scope. This
    /// preserves shadowed types without adding invented names to the public IR.
    pub(crate) fn local_typedef(&self, name: &str) -> Option<&Type> {
        self.lexical_scopes
            .iter()
            .rev()
            .find(|scope| scope.names.contains_key(name))
            .and_then(|scope| scope.typedefs.get(name))
    }

    fn with_block<T>(
        &mut self,
        span: lang_c::span::Span,
        check: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.with_scope(span, None, ScopeKind::Block, check)
    }

    fn with_statement<T>(
        &mut self,
        span: lang_c::span::Span,
        check: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        self.with_block(span, |analyzer| {
            if let Some(checked) = &mut analyzer.checked {
                checked.statement_scope();
            }
            check(analyzer)
        })
    }

    fn with_scope<T>(
        &mut self,
        span: lang_c::span::Span,
        reuse: Option<ScopeId>,
        kind: ScopeKind,
        check: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.lexical_scopes.len() >= 128 {
            return Err(Error::new(
                0,
                "lexical scope nesting exceeds the 128-level limit",
            ));
        }
        if let Some(checked) = &mut self.checked {
            checked.enter_scope(kind, span, reuse)?;
        }
        self.lexical_scopes.push(LexicalScope {
            is_block: true,
            ..LexicalScope::default()
        });
        let result = check(self);
        self.leave_prototype();
        result
    }

    pub(crate) fn function_definition(
        &mut self,
        definition: &Node<ast::FunctionDefinition>,
    ) -> Result<(), Error> {
        if !definition.node.declarations.is_empty() {
            return Err(Error::new(
                definition.span.start,
                "K&R function definitions are unsupported",
            ));
        }
        let name = declarator_name(&definition.node.declarator)
            .ok_or_else(|| Error::new(definition.span.start, "function definition has no name"))?;
        let declaration = Node::new(
            ast::Declaration {
                specifiers: definition.node.specifiers.clone(),
                declarators: vec![Node::new(
                    ast::InitDeclarator {
                        declarator: definition.node.declarator.clone(),
                        initializer: None,
                    },
                    definition.span,
                )],
            },
            definition.span,
        );
        self.function_scope = None;
        self.capture_function_scope = true;
        if let Some(checked) = &mut self.checked {
            checked.definition = checked.find(OccurrenceKind::Function, definition)?;
        }
        let result = self.declaration(&declaration, true);
        if let Some(checked) = &mut self.checked {
            checked.definition = None;
        }
        self.capture_function_scope = false;
        result?;
        let declaration = self
            .unit
            .declarations
            .iter()
            .rev()
            .find(|declaration| declaration.name == name)
            .expect("definition was declared");
        let TypeKind::Function(function) = self.unit.resolve(&declaration.ty)?.kind.clone() else {
            return Err(Error::new(
                definition.span.start,
                "definition does not declare a function",
            ));
        };
        if !matches!(
            self.unit.resolve(&function.return_type)?.kind,
            TypeKind::Void
        ) && !self.is_complete_object(&function.return_type, 0)?
        {
            return Err(Error::new(
                definition.span.start,
                "function definition requires a complete return type",
            ));
        }
        self.variably_modified_parents.clear();
        let parameters = self.function_scope.take();
        let mut signature = *function;
        if let Some(parameters) = &parameters {
            signature.parameters = parameters.parameters.clone();
        }
        self.current_function = Some(FunctionContext {
            signature,
            parameter_scope: self.lexical_scopes.len(),
            statement_expressions: BTreeMap::new(),
            expression_parents: Vec::new(),
            active_expression: None,
            loops: 0,
            switches: Vec::new(),
            labels: BTreeMap::new(),
            gotos: Vec::new(),
            fallthrough: Vec::new(),
        });
        let reuse = parameters
            .as_ref()
            .and_then(|parameters| parameters.checked_scope);
        if let Some(checked) = &mut self.checked {
            checked.begin_body(definition)?;
        }
        let result = self.with_scope(definition.span, reuse, ScopeKind::Function, |analyzer| {
            if let Some(parameters) = parameters {
                for id in parameters.record_ids {
                    analyzer.unit.records[id].scope = Scope::Block;
                }
                for id in parameters.enum_ids {
                    analyzer.unit.enums[id].scope = Scope::Block;
                }
                for (name, tag) in parameters.tags {
                    analyzer.bind_tag(name, tag);
                }
                for (name, value) in parameters.constants {
                    let previous = analyzer.unit.constants.insert(name.clone(), value);
                    let scope = analyzer
                        .lexical_scopes
                        .last_mut()
                        .expect("function body scope");
                    scope.constants.push((name.clone(), previous));
                    scope.names.insert(name, None);
                }
                for parameter in parameters.parameters {
                    if matches!(analyzer.unit.resolve(&parameter.ty)?.kind, TypeKind::Void)
                        && parameter.name.is_none()
                    {
                        continue;
                    }
                    let name = parameter.name.ok_or_else(|| {
                        Error::new(
                            definition.span.start,
                            "function definition parameters require names in C11",
                        )
                    })?;
                    if !analyzer.is_complete_object(&parameter.ty, 0)? {
                        return Err(Error::new(
                            definition.span.start,
                            "function definition parameter has incomplete type",
                        ));
                    }
                    analyzer.bind_local(
                        &name,
                        parameter.ty,
                        false,
                        parameters.register.contains(&name),
                        definition.span.start,
                    )?;
                }
            }
            // Varargs validation relies on these names resolving to the actual
            // definition parameters before any nested block can shadow them.
            debug_assert!(
                analyzer
                    .current_function_signature()
                    .expect("function signature")
                    .parameters
                    .iter()
                    .filter_map(|parameter| parameter.name.as_deref())
                    .all(|name| analyzer.current_function_parameter(name).is_some())
            );
            let mut character = Type::new(TypeKind::Integer(crate::IntegerKind::Char));
            character.qualifiers.is_const = true;
            let function_name = Type::new(TypeKind::Array {
                element: Box::new(character),
                length: Some(name.len() as u64 + 1),
            });
            for predefined in ["__func__", "__FUNCTION__", "__PRETTY_FUNCTION__"] {
                analyzer.bind_local(
                    predefined,
                    function_name.clone(),
                    true,
                    false,
                    definition.span.start,
                )?;
                if let Some(checked) = &mut analyzer.checked {
                    checked.synthetic_object(predefined, definition.span.start)?;
                }
            }
            let ast::Statement::Compound(items) = &definition.node.statement.node else {
                return Err(Error::new(
                    definition.span.start,
                    "function body must be a compound statement",
                ));
            };
            let checked_statement = analyzer
                .checked
                .as_mut()
                .map(|checked| checked.begin_statement(&definition.node.statement))
                .transpose()?
                .flatten();
            analyzer.block_items(items)?;
            analyzer.require_no_fallthrough()?;
            if let Some(id) = checked_statement {
                analyzer.retain_statement(&definition.node.statement, id)?;
            }
            let context = analyzer.function_context();
            let expression_ends = ancestry_ends(&context.expression_parents);
            // VM declarations are visited in lexical preorder. An ancestor
            // therefore contains one contiguous interval of descendant IDs.
            let scope_ends = ancestry_ends(&analyzer.variably_modified_parents);
            for (name, offset, active) in &context.gotos {
                let Some(target) = context.labels.get(name) else {
                    return Err(Error::new(
                        *offset,
                        format!("goto targets undefined label `{name}`"),
                    ));
                };
                if enters_scope(
                    target.variably_modified,
                    active.variably_modified,
                    &scope_ends,
                ) {
                    return Err(Error::new(
                        *offset,
                        "goto enters the scope of a variably modified identifier",
                    ));
                }
                if enters_scope(
                    target.statement_expression,
                    active.statement_expression,
                    &expression_ends,
                ) {
                    return Err(Error::new(*offset, "goto enters a statement expression"));
                }
            }
            if analyzer.checked.is_some() {
                analyzer.retain_function_body(definition)?;
            }
            Ok(())
        });
        self.current_function = None;
        result
    }

    fn bind_local(
        &mut self,
        name: &str,
        ty: Type,
        static_storage: bool,
        register: bool,
        offset: usize,
    ) -> Result<(), Error> {
        let previous = self.unit.constants.remove(name);
        let scope = self.lexical_scopes.last_mut().expect("local scope");
        if scope.names.contains_key(name) {
            return Err(Error::new(
                offset,
                format!("duplicate local declaration `{name}`"),
            ));
        }
        scope.constants.push((name.to_owned(), previous));
        scope
            .names
            .insert(name.to_owned(), Some(scope.parameters.len()));
        scope.parameters.push(Parameter {
            name: Some(name.to_owned()),
            ty,
        });
        if static_storage {
            scope.static_storage.insert(name.to_owned());
        }
        if register {
            scope.register.insert(name.to_owned());
        }
        Ok(())
    }

    fn block_items(&mut self, items: &[Node<ast::BlockItem>]) -> Result<(), Error> {
        for item in items {
            match &item.node {
                ast::BlockItem::Declaration(declaration) => {
                    self.block_declaration(declaration, false)?
                }
                ast::BlockItem::StaticAssert(assertion) => self.static_assert(assertion)?,
                ast::BlockItem::Statement(statement) => self.statement(statement)?,
            }
        }
        Ok(())
    }

    fn block_declaration(
        &mut self,
        declaration: &Node<ast::Declaration>,
        for_initializer: bool,
    ) -> Result<(), Error> {
        let checkpoint = self
            .checked
            .as_ref()
            .map(|checked| checked.declaration_checkpoint());
        self.block_declaration_inner(declaration, for_initializer)?;
        if let (Some(checked), Some(checkpoint)) = (&mut self.checked, checkpoint) {
            checked.complete_declaration_group(declaration, checkpoint)?;
        }
        Ok(())
    }

    fn block_declaration_inner(
        &mut self,
        declaration: &Node<ast::Declaration>,
        for_initializer: bool,
    ) -> Result<(), Error> {
        let storage: Vec<_> = declaration
            .node
            .specifiers
            .iter()
            .filter_map(|specifier| {
                if let ast::DeclarationSpecifier::StorageClass(storage) = &specifier.node {
                    Some(&storage.node)
                } else {
                    None
                }
            })
            .collect();
        if storage.len() > 1 {
            return Err(Error::new(
                declaration.span.start,
                "multiple storage classes in block declaration",
            ));
        }
        let storage = storage.first().copied();
        if storage == Some(&ast::StorageClassSpecifier::ThreadLocal) {
            return Err(Error::new(
                declaration.span.start,
                "block thread-local objects are unsupported",
            ));
        }
        if for_initializer
            && storage.is_some_and(|storage| {
                !matches!(
                    storage,
                    ast::StorageClassSpecifier::Auto | ast::StorageClassSpecifier::Register
                )
            })
        {
            return Err(Error::new(
                declaration.span.start,
                "for declaration permits only auto or register storage",
            ));
        }
        let is_typedef = storage == Some(&ast::StorageClassSpecifier::Typedef);
        let is_extern = storage == Some(&ast::StorageClassSpecifier::Extern);
        let is_static = storage == Some(&ast::StorageClassSpecifier::Static);
        let register = storage == Some(&ast::StorageClassSpecifier::Register);
        // A standalone tag declaration starts a tag in this block even when an
        // outer tag of the same spelling exists.
        if declaration.node.declarators.is_empty() {
            for specifier in &declaration.node.specifiers {
                if let ast::DeclarationSpecifier::TypeSpecifier(Node {
                    node: ast::TypeSpecifier::Struct(record),
                    ..
                }) = &specifier.node
                    && record.node.declarations.is_none()
                    && let Some(identifier) = &record.node.identifier
                    && self
                        .tags
                        .get(&identifier.node.name)
                        .is_some_and(|binding| binding.depth < self.lexical_scopes.len())
                {
                    let previous = self.tags.remove(&identifier.node.name);
                    self.lexical_scopes
                        .last_mut()
                        .expect("block scope")
                        .tags
                        .push((identifier.node.name.clone(), previous));
                }
            }
        }
        let (base, attributes) = self.specifiers(&declaration.node.specifiers)?;
        if declaration.node.declarators.is_empty() {
            attributes.require_function_diagnostics(false)?;
            attributes.require_no_weak()?;
        }
        for item in &declaration.node.declarators {
            let (name, mut ty, extra) =
                self.declarator(base.clone(), &item.node.declarator, &attributes)?;
            if is_typedef {
                self.align_typedef(
                    &mut ty,
                    &declaration.node.specifiers,
                    &attributes,
                    &extra,
                    item.span.start,
                )?;
            }
            let name =
                name.ok_or_else(|| Error::new(item.span.start, "local declaration has no name"))?;
            let variably_modified = self.unit.is_variably_modified(&ty)?;
            let function = matches!(self.unit.resolve(&ty)?.kind, TypeKind::Function(_));
            if (!is_typedef && !function) || variably_modified {
                self.require_no_fallthrough()?;
            }
            extra.require_function_diagnostics(function && !is_typedef)?;
            self.check_diagnostic_attributes(&name, &extra.diagnostic_attributes)?;
            if extra.weak.is_some() && extra.link_name.is_some() {
                return Err(Error::new(
                    item.span.start,
                    "assembly labels on block weak declarations are unsupported",
                ));
            }
            let previous_symbol = extra
                .weak
                .and_then(|_| self.unit.declarations.iter().find(|item| item.name == name));
            let external = !is_typedef
                && (is_extern || function)
                && !is_static
                && previous_symbol.is_none_or(|item| !item.is_static);
            let previous_definition = previous_symbol.is_some_and(|item| item.is_definition);
            let symbol_binding =
                self.check_symbol_binding(&name, extra.weak, external, previous_definition)?;

            if variably_modified && (is_extern || function) {
                return Err(Error::new(
                    item.span.start,
                    "variably modified identifiers cannot have linkage",
                ));
            }
            if is_static && self.unit.is_variable_length_array(&ty)? {
                return Err(Error::new(
                    item.span.start,
                    "variable-length arrays cannot have static storage duration",
                ));
            }
            if function
                && (for_initializer
                    || storage
                        .is_some_and(|storage| storage != &ast::StorageClassSpecifier::Extern))
                && !is_typedef
            {
                return Err(Error::new(
                    item.span.start,
                    "invalid storage class for a block function declaration",
                ));
            }
            if is_typedef {
                if item.node.initializer.is_some() {
                    return Err(Error::new(
                        item.span.start,
                        "typedef cannot have an initializer",
                    ));
                }
                let scope = self.lexical_scopes.last().expect("block scope");
                if let Some(previous) = scope.typedefs.get(&name) {
                    if variably_modified || !self.same_type(previous, &ty, 0)? {
                        return Err(Error::new(item.span.start, "conflicting block typedef"));
                    }
                    let alignment = self
                        .unit
                        .typedef_alignment(previous)?
                        .max(self.unit.typedef_alignment(&ty)?);
                    ty = self.composite_type(previous, &ty, 0)?;
                    ty.alignment = alignment;
                    if let Some(checked) = &mut self.checked {
                        checked.local_declaration(
                            item,
                            OccurrenceKind::InitDeclarator,
                            LocalDeclaration {
                                name: Some(&name),
                                name_span: declarator_name_span(&item.node.declarator),
                                ty: &ty,
                                kind: EntityKind::Typedef,
                                storage: Storage::None,
                                linked: false,
                                register: false,
                                definition: false,
                                allocation: None,
                            },
                        )?;
                    }
                    self.lexical_scopes
                        .last_mut()
                        .expect("block scope")
                        .typedefs
                        .insert(name, ty);
                    continue;
                }
                if scope.names.contains_key(&name) {
                    return Err(Error::new(
                        item.span.start,
                        "typedef conflicts with a local identifier",
                    ));
                }
                if let Some(checked) = &mut self.checked {
                    checked.local_declaration(
                        item,
                        OccurrenceKind::InitDeclarator,
                        LocalDeclaration {
                            name: Some(&name),
                            name_span: declarator_name_span(&item.node.declarator),
                            ty: &ty,
                            kind: EntityKind::Typedef,
                            storage: Storage::None,
                            linked: false,
                            register: false,
                            definition: false,
                            allocation: None,
                        },
                    )?;
                }
                let previous = self.unit.constants.remove(&name);
                let scope = self.lexical_scopes.last_mut().expect("block scope");
                scope.constants.push((name.clone(), previous));
                scope.names.insert(name.clone(), None);
                scope.typedefs.insert(name, ty);
                if variably_modified {
                    self.declare_variably_modified();
                }
                continue;
            }
            if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Void) {
                return Err(Error::new(
                    item.span.start,
                    "local object cannot have void type",
                ));
            }
            let linked = is_extern || function;
            if function
                && let Some(previous) = self.block_externs.get(&name).or_else(|| {
                    self.unit
                        .declarations
                        .iter()
                        .find(|declaration| declaration.name == name)
                        .map(|declaration| &declaration.ty)
                })
            {
                ty = self.inherit_calling_convention(ty, previous)?;
            }
            if linked {
                if item.node.initializer.is_some() {
                    return Err(Error::new(
                        item.span.start,
                        "block extern declaration cannot have an initializer",
                    ));
                }
                if let Some(previous) = self.block_externs.get(&name)
                    && !self.compatible(previous, &ty)?
                {
                    return Err(Error::new(
                        item.span.start,
                        "conflicting block extern declarations",
                    ));
                }
                if let Some(previous) = self
                    .unit
                    .declarations
                    .iter()
                    .find(|declaration| declaration.name == name)
                    && previous.kind != DeclarationKind::Typedef
                    && !self.compatible(&previous.ty, &ty)?
                {
                    return Err(Error::new(
                        item.span.start,
                        "block extern conflicts with a file declaration",
                    ));
                }
                self.block_externs.insert(name.clone(), ty.clone());
            }
            let existing = self
                .lexical_scopes
                .last()
                .expect("block scope")
                .names
                .get(&name)
                .copied();
            if let Some(Some(index)) = existing {
                let previous =
                    &self.lexical_scopes.last().expect("block scope").parameters[index].ty;
                if linked
                    && self
                        .lexical_scopes
                        .last()
                        .expect("block scope")
                        .linked
                        .contains(&name)
                    && self.compatible(previous, &ty)?
                {
                    let composite = self.composite_type(previous, &ty, 0)?;
                    if let Some(checked) = &mut self.checked {
                        let site = checked.local_declaration(
                            item,
                            OccurrenceKind::InitDeclarator,
                            LocalDeclaration {
                                name: Some(&name),
                                name_span: declarator_name_span(&item.node.declarator),
                                ty: &composite,
                                kind: if function {
                                    EntityKind::Function
                                } else {
                                    EntityKind::Variable
                                },
                                storage: if function {
                                    Storage::None
                                } else {
                                    Storage::Static
                                },
                                linked: true,
                                register,
                                definition: false,
                                allocation: None,
                            },
                        )?;
                        if let Some(site) = site {
                            checked
                                .attach_diagnostic_attributes(site, &extra.diagnostic_attributes)?;
                            checked.attach_symbol_binding(site, symbol_binding, extra.weak);
                        }
                    }
                    self.lexical_scopes
                        .last_mut()
                        .expect("block scope")
                        .parameters[index]
                        .ty = composite.clone();
                    self.block_externs.insert(name, composite);
                    continue;
                }
            }
            self.bind_local(
                &name,
                ty.clone(),
                is_static || linked,
                register,
                item.span.start,
            )?;
            if variably_modified {
                self.declare_variably_modified();
            }
            if linked {
                self.lexical_scopes
                    .last_mut()
                    .expect("block scope")
                    .linked
                    .insert(name.clone());
            }
            let checked_site = if let Some(checked) = &mut self.checked {
                checked.local_declaration(
                    item,
                    OccurrenceKind::InitDeclarator,
                    LocalDeclaration {
                        name: Some(&name),
                        name_span: declarator_name_span(&item.node.declarator),
                        ty: &ty,
                        kind: if function {
                            EntityKind::Function
                        } else {
                            EntityKind::Variable
                        },
                        storage: if function {
                            Storage::None
                        } else if is_static || linked {
                            Storage::Static
                        } else {
                            Storage::Automatic
                        },
                        linked,
                        register,
                        definition: !linked,
                        allocation: None,
                    },
                )?
            } else {
                None
            };
            if let Some(initializer) = &item.node.initializer {
                let (completed, storage) =
                    self.check_object_initializer(&ty, initializer, is_static)?;
                ty = completed;
                let scope = self.lexical_scopes.last_mut().expect("block scope");
                if let Some(storage) = storage {
                    scope.flexible_array_storage.insert(name.clone(), storage);
                }
                let index = scope.names[&name].expect("object binding");
                scope.parameters[index].ty = ty.clone();
            }
            if !linked && !self.is_complete_object(&ty, 0)? {
                return Err(Error::new(
                    item.span.start,
                    "local object requires a complete type",
                ));
            }
            if let (Some(checked), Some(site)) = (&mut self.checked, checked_site) {
                checked.attach_diagnostic_attributes(site, &extra.diagnostic_attributes)?;
                checked.attach_symbol_binding(site, symbol_binding, extra.weak);
                let allocation = self
                    .lexical_scopes
                    .last()
                    .and_then(|scope| scope.flexible_array_storage.get(&name));
                checked.complete_declaration(site, &ty, allocation)?;
                if let Some(initializer) = &item.node.initializer {
                    checked.attach_initializer(site, initializer)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_block_externs(&self) -> Result<(), Error> {
        for declaration in &self.unit.declarations {
            if let Some(previous) = self.block_externs.get(&declaration.name)
                && declaration.kind != DeclarationKind::Typedef
                && !self.compatible(previous, &declaration.ty)?
            {
                return Err(Error::new(
                    0,
                    format!(
                        "file declaration conflicts with block extern `{}`",
                        declaration.name
                    ),
                ));
            }
        }
        Ok(())
    }

    fn with_loop(
        &mut self,
        check: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.function_context_mut().loops += 1;
        if let Some(checked) = &mut self.checked {
            checked.enter_control(ControlKind::Loop, 0)?;
        }
        let result = check(self).and_then(|()| self.require_no_fallthrough());
        if let Some(checked) = &mut self.checked {
            checked.leave_control();
        }
        self.function_context_mut().loops -= 1;
        result
    }

    fn for_clauses(&mut self, iteration: &ast::ForStatement) -> Result<(), Error> {
        if let Some(condition) = &iteration.condition {
            self.scalar_condition(condition)?;
        }
        if let Some(step) = &iteration.step {
            self.value_expression_type(step)?;
        }
        Ok(())
    }

    fn scalar_condition(&mut self, expression: &Node<ast::Expression>) -> Result<(), Error> {
        let ty = self.value_expression_type(expression)?;
        self.require_scalar(&ty, expression.span.start)
    }

    fn statement(&mut self, statement: &Node<ast::Statement>) -> Result<(), Error> {
        let checked_statement = if let Some(checked) = &mut self.checked {
            let Some(id) = checked.begin_statement(statement)? else {
                return Ok(());
            };
            Some(id)
        } else {
            None
        };
        self.enter_expression(statement.span.start)?;
        let result = self.statement_inner(statement).and_then(|()| {
            if let Some(id) = checked_statement {
                self.retain_statement(statement, id)?;
            }
            Ok(())
        });
        self.leave_expression();
        result
    }

    fn substatement(&mut self, statement: &Node<ast::Statement>) -> Result<(), Error> {
        self.with_block(statement.span, |analyzer| analyzer.statement(statement))
    }

    /// Pending annotations follow ordinary control flow through empty blocks and
    /// merged if branches, and must reach a label in their enclosing switch.
    fn require_no_fallthrough(&self) -> Result<(), Error> {
        if let Some((offset, _)) = self.function_context().fallthrough.first() {
            return Err(Error::new(
                *offset,
                "fallthrough annotation does not directly precede a switch label",
            ));
        }
        Ok(())
    }

    fn statement_inner(&mut self, statement: &Node<ast::Statement>) -> Result<(), Error> {
        let offset = statement.span.start;
        match &statement.node {
            ast::Statement::Compound(_)
            | ast::Statement::Expression(None)
            | ast::Statement::Attribute(_) => {}
            ast::Statement::Labeled(labeled) => match &labeled.node.label.node {
                ast::Label::Identifier(_) => {
                    let mut child = &labeled.node.statement.node;
                    while let ast::Statement::Labeled(nested) = child {
                        if !matches!(nested.node.label.node, ast::Label::Identifier(_)) {
                            break;
                        }
                        child = &nested.node.statement.node;
                    }
                    if !matches!(
                        child,
                        ast::Statement::Expression(None) | ast::Statement::Compound(_)
                    ) {
                        self.require_no_fallthrough()?;
                    }
                }
                _ => {
                    let context = self.function_context();
                    if let Some((annotation, target)) = context.fallthrough.first()
                        && context
                            .switches
                            .last()
                            .is_none_or(|current| current.offset != *target)
                    {
                        return Err(Error::new(
                            *annotation,
                            "fallthrough annotation crosses a switch boundary",
                        ));
                    }
                    self.function_context_mut().fallthrough.clear();
                }
            },
            _ => self.require_no_fallthrough()?,
        }
        match &statement.node {
            ast::Statement::Compound(items) => {
                self.with_statement(statement.span, |analyzer| analyzer.block_items(items))
            }
            ast::Statement::Expression(expression) => {
                if let Some(expression) = expression {
                    self.value_expression_type(expression)?;
                }
                Ok(())
            }
            ast::Statement::Attribute(attributes) => {
                self.require_no_fallthrough()?;
                let [attribute] = attributes.as_slice() else {
                    return Err(Error::new(
                        offset,
                        "a fallthrough statement requires exactly one attribute",
                    ));
                };
                let ast::Extension::Attribute(attribute) = &attribute.node else {
                    return Err(Error::new(offset, "unsupported statement attribute"));
                };
                if attribute.name.node.trim_matches('_') != "fallthrough" {
                    return Err(Error::new(
                        offset,
                        "only fallthrough attributes are supported on null statements",
                    ));
                }
                if !attribute.arguments.is_empty() {
                    return Err(Error::new(
                        offset,
                        "fallthrough attributes do not take arguments",
                    ));
                }
                let context = self.function_context_mut();
                let switch = context.switches.last().ok_or_else(|| {
                    Error::new(offset, "fallthrough annotation is outside a switch")
                })?;
                context.fallthrough.push((offset, switch.offset));
                Ok(())
            }
            ast::Statement::Return(expression) => {
                let return_type = self.function_context().signature.return_type.clone();
                let void = matches!(self.unit.resolve(&return_type)?.kind, TypeKind::Void);
                match (void, expression) {
                    (true, None) => Ok(()),
                    (false, Some(expression)) => self.check_assignment(&return_type, expression),
                    (true, Some(_)) => {
                        Err(Error::new(offset, "void function cannot return a value"))
                    }
                    (false, None) => {
                        Err(Error::new(offset, "non-void function must return a value"))
                    }
                }
            }
            ast::Statement::If(selection) => self.with_statement(statement.span, |analyzer| {
                analyzer.scalar_condition(&selection.node.condition)?;
                analyzer.substatement(&selection.node.then_statement)?;
                let mut then_fallthrough =
                    std::mem::take(&mut analyzer.function_context_mut().fallthrough);
                if let Some(statement) = &selection.node.else_statement {
                    analyzer.substatement(statement)?;
                }
                then_fallthrough.append(&mut analyzer.function_context_mut().fallthrough);
                analyzer.function_context_mut().fallthrough = then_fallthrough;
                Ok(())
            }),
            ast::Statement::While(iteration) => self.with_statement(statement.span, |analyzer| {
                if analyzer.gnu_statement_expressions() {
                    analyzer.scalar_condition(&iteration.node.expression)?;
                    analyzer.with_loop(|analyzer| analyzer.substatement(&iteration.node.statement))
                } else {
                    analyzer.with_loop(|analyzer| {
                        analyzer.scalar_condition(&iteration.node.expression)?;
                        analyzer.substatement(&iteration.node.statement)
                    })
                }
            }),
            ast::Statement::DoWhile(iteration) => self.with_statement(statement.span, |analyzer| {
                if analyzer.gnu_statement_expressions() {
                    analyzer
                        .with_loop(|analyzer| analyzer.substatement(&iteration.node.statement))?;
                    analyzer.scalar_condition(&iteration.node.expression)
                } else {
                    analyzer.with_loop(|analyzer| {
                        analyzer.substatement(&iteration.node.statement)?;
                        analyzer.scalar_condition(&iteration.node.expression)
                    })
                }
            }),
            ast::Statement::For(iteration) => self.with_statement(statement.span, |analyzer| {
                match &iteration.node.initializer.node {
                    ast::ForInitializer::Empty => {}
                    ast::ForInitializer::Expression(expression) => {
                        analyzer.value_expression_type(expression)?;
                    }
                    ast::ForInitializer::Declaration(declaration) => {
                        analyzer.block_declaration(declaration, true)?
                    }
                    ast::ForInitializer::StaticAssert(assertion) => {
                        analyzer.static_assert(assertion)?
                    }
                }
                if analyzer.gnu_statement_expressions() {
                    analyzer.for_clauses(&iteration.node)?;
                    analyzer.with_loop(|analyzer| analyzer.substatement(&iteration.node.statement))
                } else {
                    analyzer.with_loop(|analyzer| {
                        analyzer.for_clauses(&iteration.node)?;
                        analyzer.substatement(&iteration.node.statement)
                    })
                }
            }),
            ast::Statement::Switch(selection) => self.with_statement(statement.span, |analyzer| {
                let ty = analyzer.value_expression_type(&selection.node.expression)?;
                let ty = promote(analyzer.integer_type(&ty, selection.node.expression.span.start)?);
                let variably_modified = analyzer.active_variably_modified();
                let context = analyzer.function_context_mut();
                context.switches.push(SwitchContext {
                    offset,
                    ty,
                    ranges: BTreeMap::new(),
                    has_default: false,
                    variably_modified,
                    statement_expression: context.active_expression,
                });
                if let Some(checked) = &mut analyzer.checked {
                    checked.enter_control(ControlKind::Switch, offset)?;
                }
                let result = analyzer
                    .substatement(&selection.node.statement)
                    .and_then(|()| analyzer.require_no_fallthrough());
                if let Some(checked) = &mut analyzer.checked {
                    checked.leave_control();
                }
                analyzer.function_context_mut().switches.pop();
                result
            }),
            ast::Statement::Labeled(labeled) => {
                match &labeled.node.label.node {
                    ast::Label::Identifier(identifier) => {
                        let scope = self.jump_scope();
                        if self
                            .function_context_mut()
                            .labels
                            .insert(identifier.node.name.clone(), scope)
                            .is_some()
                        {
                            return Err(Error::new(offset, "duplicate label in function"));
                        }
                    }
                    ast::Label::Case(expression) => {
                        let value = self.eval(expression)?;
                        self.case_range(value, value, offset)?;
                    }
                    ast::Label::CaseRange(range) => {
                        let low = self.eval(&range.node.low)?;
                        let high = self.eval(&range.node.high)?;
                        self.case_range(low, high, offset)?;
                    }
                    ast::Label::Default => {
                        let switch = self.function_context().switches.last().ok_or_else(|| {
                            Error::new(offset, "default label is outside a switch")
                        })?;
                        self.check_switch_entry(switch, offset)?;
                        let switch = self
                            .function_context_mut()
                            .switches
                            .last_mut()
                            .expect("checked switch");
                        if switch.has_default {
                            return Err(Error::new(offset, "duplicate default label"));
                        }
                        switch.has_default = true;
                    }
                }
                self.statement(&labeled.node.statement)
            }
            ast::Statement::Goto(identifier) => {
                let scope = self.jump_scope();
                self.function_context_mut().gotos.push((
                    identifier.node.name.clone(),
                    offset,
                    scope,
                ));
                Ok(())
            }
            ast::Statement::Break => {
                let context = self.function_context();
                if context.loops == 0 && context.switches.is_empty() {
                    return Err(Error::new(offset, "break is outside a loop or switch"));
                }
                Ok(())
            }
            ast::Statement::Continue => {
                if self.function_context().loops == 0 {
                    return Err(Error::new(offset, "continue is outside a loop"));
                }
                Ok(())
            }
            ast::Statement::Asm(assembly) => self.asm_statement(assembly),
        }
    }

    /// Uses one ancestry node per VM declaration instead of copying every live
    /// binding at every jump. Leaving a lexical scope restores its parent's node.
    fn declare_variably_modified(&mut self) {
        let parent = self.active_variably_modified();
        let id = self.variably_modified_parents.len();
        self.variably_modified_parents.push(parent);
        self.lexical_scopes
            .last_mut()
            .expect("block scope")
            .variably_modified = Some(id);
    }

    fn active_variably_modified(&self) -> Option<usize> {
        self.lexical_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.variably_modified)
    }

    pub(crate) fn retained_switch_integer_type(
        &self,
        offset: usize,
    ) -> Result<IntegerValue, Error> {
        self.function_context()
            .switches
            .last()
            .map(|switch| switch.ty)
            .ok_or_else(|| Error::new(offset, "case has no current switch"))
    }

    fn check_switch_entry(&self, switch: &SwitchContext, offset: usize) -> Result<(), Error> {
        if self.function_context().active_expression != switch.statement_expression {
            return Err(Error::new(offset, "switch enters a statement expression"));
        }
        // Every case is lexically inside its switch. A different active node
        // means an intervening VM declaration would be skipped on entry.
        if self.active_variably_modified() != switch.variably_modified {
            return Err(Error::new(
                offset,
                "switch enters the scope of a variably modified identifier",
            ));
        }
        Ok(())
    }

    fn case_range(
        &mut self,
        low: IntegerValue,
        high: IntegerValue,
        offset: usize,
    ) -> Result<(), Error> {
        let switch = self
            .function_context()
            .switches
            .last()
            .ok_or_else(|| Error::new(offset, "case label is outside a switch"))?;
        self.check_switch_entry(switch, offset)?;
        let switch = self
            .function_context_mut()
            .switches
            .last_mut()
            .expect("checked switch");
        let sign = if switch.ty.signed {
            1u128 << (switch.ty.bits - 1)
        } else {
            0
        };
        let low = convert(low, switch.ty).value ^ sign;
        let high = convert(high, switch.ty).value ^ sign;
        if low > high {
            return Err(Error::new(offset, "case range is empty after conversion"));
        }
        if switch
            .ranges
            .range(..=high)
            .next_back()
            .is_some_and(|(_, end)| low <= *end)
        {
            return Err(Error::new(offset, "duplicate or overlapping switch cases"));
        }
        switch.ranges.insert(low, high);
        Ok(())
    }
}

fn declarator_name(declarator: &Node<ast::Declarator>) -> Option<String> {
    match &declarator.node.kind.node {
        ast::DeclaratorKind::Identifier(identifier) => Some(identifier.node.name.clone()),
        ast::DeclaratorKind::Declarator(inner) => declarator_name(inner),
        ast::DeclaratorKind::Abstract => None,
    }
}

fn ancestry_ends(parents: &[Option<usize>]) -> Vec<usize> {
    let mut ends: Vec<_> = (0..parents.len()).collect();
    for id in (0..ends.len()).rev() {
        if let Some(parent) = parents[id] {
            ends[parent] = ends[parent].max(ends[id]);
        }
    }
    ends
}

fn enters_scope(target: Option<usize>, source: Option<usize>, ends: &[usize]) -> bool {
    target
        .is_some_and(|target| source.is_none_or(|source| source < target || source > ends[target]))
}

/// A label can prefix the final value-producing expression statement.
fn final_expression(statement: &Node<ast::Statement>) -> Option<&Node<ast::Expression>> {
    match &statement.node {
        ast::Statement::Expression(expression) => expression.as_deref(),
        ast::Statement::Labeled(labeled) => final_expression(&labeled.node.statement),
        _ => None,
    }
}
