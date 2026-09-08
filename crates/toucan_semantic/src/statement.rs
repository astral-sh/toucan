use std::collections::{BTreeMap, HashSet};

use lang_c::{ast, span::Node};

use crate::analyze::{Analyzer, LexicalScope, Tag};
use crate::integer::{convert, promote};
use crate::{DeclarationKind, Error, IntegerValue, Parameter, Scope, Type, TypeKind};

/// Bindings introduced in a definition's parameter list remain visible in its body.
pub(crate) struct FunctionScope {
    pub(crate) record_ids: Vec<usize>,
    pub(crate) enum_ids: Vec<usize>,
    pub(crate) tags: Vec<(String, Tag)>,
    pub(crate) constants: Vec<(String, IntegerValue)>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) register: HashSet<String>,
}

struct FunctionContext {
    return_type: Type,
    loops: usize,
    switches: Vec<SwitchContext>,
    labels: BTreeMap<String, Option<usize>>,
    gotos: Vec<(String, usize, Option<usize>)>,
}

struct SwitchContext {
    ty: IntegerValue,
    ranges: BTreeMap<u128, u128>,
    has_default: bool,
    variably_modified: Option<usize>,
}

impl Analyzer {
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
        check: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        if self.lexical_scopes.len() >= 128 {
            return Err(Error::new(
                0,
                "lexical scope nesting exceeds the 128-level limit",
            ));
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
        let result = self.declaration(&declaration, true);
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
        let mut context = FunctionContext {
            return_type: function.return_type,
            loops: 0,
            switches: Vec::new(),
            labels: BTreeMap::new(),
            gotos: Vec::new(),
        };
        let parameters = self.function_scope.take();
        self.with_block(|analyzer| {
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
            }
            let ast::Statement::Compound(items) = &definition.node.statement.node else {
                return Err(Error::new(
                    definition.span.start,
                    "function body must be a compound statement",
                ));
            };
            analyzer.block_items(items, &mut context)?;
            // VM declarations are visited in lexical preorder. An ancestor
            // therefore contains one contiguous interval of descendant IDs.
            let mut scope_ends: Vec<_> = (0..analyzer.variably_modified_parents.len()).collect();
            for id in (0..scope_ends.len()).rev() {
                if let Some(parent) = analyzer.variably_modified_parents[id] {
                    scope_ends[parent] = scope_ends[parent].max(scope_ends[id]);
                }
            }
            for (name, offset, active) in &context.gotos {
                let Some(target) = context.labels.get(name) else {
                    return Err(Error::new(
                        *offset,
                        format!("goto targets undefined label `{name}`"),
                    ));
                };
                if target.is_some_and(|target| {
                    active.is_none_or(|active| active < target || active > scope_ends[target])
                }) {
                    return Err(Error::new(
                        *offset,
                        "goto enters the scope of a variably modified identifier",
                    ));
                }
            }
            Ok(())
        })
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

    fn block_items(
        &mut self,
        items: &[Node<ast::BlockItem>],
        context: &mut FunctionContext,
    ) -> Result<(), Error> {
        for item in items {
            match &item.node {
                ast::BlockItem::Declaration(declaration) => {
                    self.block_declaration(declaration, false)?
                }
                ast::BlockItem::StaticAssert(assertion) => self.static_assert(assertion)?,
                ast::BlockItem::Statement(statement) => self.statement(statement, context)?,
            }
        }
        Ok(())
    }

    fn block_declaration(
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
        let (base, _) = self.specifiers(&declaration.node.specifiers)?;
        for item in &declaration.node.declarators {
            let (name, mut ty, _) = self.declarator(base.clone(), &item.node.declarator)?;
            let name =
                name.ok_or_else(|| Error::new(item.span.start, "local declaration has no name"))?;
            let variably_modified = self.unit.is_variably_modified(&ty)?;
            let function = matches!(self.unit.resolve(&ty)?.kind, TypeKind::Function(_));
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
                    if variably_modified || !self.compatible(previous, &ty)? {
                        return Err(Error::new(item.span.start, "conflicting block typedef"));
                    }
                    continue;
                }
                if scope.names.contains_key(&name) {
                    return Err(Error::new(
                        item.span.start,
                        "typedef conflicts with a local identifier",
                    ));
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
            if let Some(initializer) = &item.node.initializer {
                ty = self.check_initializer(&ty, initializer, is_static)?;
                let scope = self.lexical_scopes.last_mut().expect("block scope");
                let index = scope.names[&name].expect("object binding");
                scope.parameters[index].ty = ty.clone();
            }
            if !linked && !self.is_complete_object(&ty, 0)? {
                return Err(Error::new(
                    item.span.start,
                    "local object requires a complete type",
                ));
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

    fn scalar_condition(&mut self, expression: &Node<ast::Expression>) -> Result<(), Error> {
        let ty = self.value_expression_type(expression)?;
        self.require_scalar(&ty, expression.span.start)
    }

    fn statement(
        &mut self,
        statement: &Node<ast::Statement>,
        context: &mut FunctionContext,
    ) -> Result<(), Error> {
        self.enter_expression(statement.span.start)?;
        let result = self.statement_inner(statement, context);
        self.leave_expression();
        result
    }

    fn substatement(
        &mut self,
        statement: &Node<ast::Statement>,
        context: &mut FunctionContext,
    ) -> Result<(), Error> {
        self.with_block(|analyzer| analyzer.statement(statement, context))
    }

    fn statement_inner(
        &mut self,
        statement: &Node<ast::Statement>,
        context: &mut FunctionContext,
    ) -> Result<(), Error> {
        let offset = statement.span.start;
        match &statement.node {
            ast::Statement::Compound(items) => {
                self.with_block(|analyzer| analyzer.block_items(items, context))
            }
            ast::Statement::Expression(expression) => {
                if let Some(expression) = expression {
                    self.value_expression_type(expression)?;
                }
                Ok(())
            }
            ast::Statement::Return(expression) => {
                let void = matches!(
                    self.unit.resolve(&context.return_type)?.kind,
                    TypeKind::Void
                );
                match (void, expression) {
                    (true, None) => Ok(()),
                    (false, Some(expression)) => {
                        self.check_assignment(&context.return_type, expression)
                    }
                    (true, Some(_)) => {
                        Err(Error::new(offset, "void function cannot return a value"))
                    }
                    (false, None) => {
                        Err(Error::new(offset, "non-void function must return a value"))
                    }
                }
            }
            ast::Statement::If(selection) => self.with_block(|analyzer| {
                analyzer.scalar_condition(&selection.node.condition)?;
                analyzer.substatement(&selection.node.then_statement, context)?;
                if let Some(statement) = &selection.node.else_statement {
                    analyzer.substatement(statement, context)?;
                }
                Ok(())
            }),
            ast::Statement::While(iteration) => self.with_block(|analyzer| {
                analyzer.scalar_condition(&iteration.node.expression)?;
                context.loops += 1;
                let result = analyzer.substatement(&iteration.node.statement, context);
                context.loops -= 1;
                result
            }),
            ast::Statement::DoWhile(iteration) => self.with_block(|analyzer| {
                context.loops += 1;
                let result = analyzer.substatement(&iteration.node.statement, context);
                context.loops -= 1;
                result?;
                analyzer.scalar_condition(&iteration.node.expression)
            }),
            ast::Statement::For(iteration) => self.with_block(|analyzer| {
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
                if let Some(condition) = &iteration.node.condition {
                    analyzer.scalar_condition(condition)?;
                }
                if let Some(step) = &iteration.node.step {
                    analyzer.value_expression_type(step)?;
                }
                context.loops += 1;
                let result = analyzer.substatement(&iteration.node.statement, context);
                context.loops -= 1;
                result
            }),
            ast::Statement::Switch(selection) => self.with_block(|analyzer| {
                let ty = analyzer.value_expression_type(&selection.node.expression)?;
                let ty = promote(analyzer.integer_type(&ty, selection.node.expression.span.start)?);
                context.switches.push(SwitchContext {
                    ty,
                    ranges: BTreeMap::new(),
                    has_default: false,
                    variably_modified: analyzer.active_variably_modified(),
                });
                let result = analyzer.substatement(&selection.node.statement, context);
                context.switches.pop();
                result
            }),
            ast::Statement::Labeled(labeled) => {
                match &labeled.node.label.node {
                    ast::Label::Identifier(identifier) => {
                        if context
                            .labels
                            .insert(
                                identifier.node.name.clone(),
                                self.active_variably_modified(),
                            )
                            .is_some()
                        {
                            return Err(Error::new(offset, "duplicate label in function"));
                        }
                    }
                    ast::Label::Case(expression) => {
                        let value = self.eval(expression)?;
                        self.case_range(context, value, value, offset)?;
                    }
                    ast::Label::CaseRange(range) => {
                        let low = self.eval(&range.node.low)?;
                        let high = self.eval(&range.node.high)?;
                        self.case_range(context, low, high, offset)?;
                    }
                    ast::Label::Default => {
                        let switch = context.switches.last_mut().ok_or_else(|| {
                            Error::new(offset, "default label is outside a switch")
                        })?;
                        self.check_switch_entry(switch, offset)?;
                        if switch.has_default {
                            return Err(Error::new(offset, "duplicate default label"));
                        }
                        switch.has_default = true;
                    }
                }
                self.statement(&labeled.node.statement, context)
            }
            ast::Statement::Goto(identifier) => {
                context.gotos.push((
                    identifier.node.name.clone(),
                    offset,
                    self.active_variably_modified(),
                ));
                Ok(())
            }
            ast::Statement::Break => {
                if context.loops == 0 && context.switches.is_empty() {
                    return Err(Error::new(offset, "break is outside a loop or switch"));
                }
                Ok(())
            }
            ast::Statement::Continue => {
                if context.loops == 0 {
                    return Err(Error::new(offset, "continue is outside a loop"));
                }
                Ok(())
            }
            ast::Statement::Asm(_) => Err(Error::new(
                offset,
                "inline assembly checking is unsupported",
            )),
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

    fn check_switch_entry(&self, switch: &SwitchContext, offset: usize) -> Result<(), Error> {
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
        &self,
        context: &mut FunctionContext,
        low: IntegerValue,
        high: IntegerValue,
        offset: usize,
    ) -> Result<(), Error> {
        let switch = context
            .switches
            .last_mut()
            .ok_or_else(|| Error::new(offset, "case label is outside a switch"))?;
        self.check_switch_entry(switch, offset)?;
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
