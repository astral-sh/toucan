//! C declarations, declarators, type names, and initializers.

use super::{PResult, Parser, TokenKind};
use ast::*;
use astutil::ts18661_float;
use driver::Standard;
use env::{find_declarator_name, Symbol};
use span::{Node, Span};

#[derive(Clone, Copy, PartialEq)]
enum DeclaratorMode {
    Named,
    Parameter,
    Abstract,
}

#[derive(Clone, Copy, PartialEq)]
enum DeclarationContext {
    File,
    Block,
    Parameter,
}

/// Groups type specifiers to locate the end of a declaration's specifiers.
/// Combinations accepted here may still be rejected by semantic analysis.
#[derive(Clone, Copy, PartialEq)]
enum TypeClass {
    Unique,
    Arithmetic,
}

impl<'s, 'e> Parser<'s, 'e> {
    pub(super) fn starts_type_name(&self) -> bool {
        self.type_class().is_some()
            || self.is_type_qualifier()
            || self.at("_Alignas")
            || self.starts_attribute()
            || self.starts_declspec()
            || self.is_calling_convention()
    }

    pub(super) fn starts_declaration(&self) -> bool {
        let distance = usize::from(self.env.extensions_gnu && self.at("__extension__"));
        let token = self.peek(distance);
        let text = self.token_text(token);
        self.type_class_at(distance).is_some()
            || self.type_qualifier_at(distance).is_some()
            || self.storage_class_at(distance).is_some()
            || self.function_specifier_at(distance).is_some()
            || text == "_Alignas"
            || self.env.extensions_gnu && matches!(text, "__attribute" | "__attribute__")
            || self.env.extensions_msvc && matches!(text, "__declspec" | "_declspec")
            || self.calling_convention_name(text)
    }

    pub(super) fn declaration(&mut self) -> PResult<Node<Declaration>> {
        self.nested(|parser| {
            let start = parser.position();
            parser.extension_prefix()?;
            let specifiers = parser.declaration_specifiers(DeclarationContext::Block)?;
            let (is_typedef, is_auto) = declaration_flags(&specifiers);
            let mut declarators = Vec::new();
            if !parser.at(";") {
                loop {
                    let declarator_start = parser.position();
                    let declarator = parser.declarator(DeclaratorMode::Named)?;
                    declarators.push(parser.finish_init_declarator(
                        declarator,
                        declarator_start,
                        is_typedef,
                        is_auto,
                    )?);
                    if !parser.eat(",")? {
                        break;
                    }
                }
            }
            parser.expect(";")?;
            parser.node(
                Declaration {
                    specifiers,
                    declarators,
                },
                start,
            )
        })
    }

    pub(super) fn external_declaration(&mut self) -> PResult<Node<ExternalDeclaration>> {
        self.nested(|parser| {
            let start = parser.position();
            parser.extension_prefix()?;
            let external = if parser.at("_Static_assert") {
                let mut assertion = parser.static_assert()?;
                assertion.span.start = start;
                ExternalDeclaration::StaticAssert(assertion)
            } else {
                let specifiers = parser.declaration_specifiers(DeclarationContext::File)?;
                let (is_typedef, is_auto) = declaration_flags(&specifiers);
                if parser.at(";") && specifiers.is_empty() {
                    return parser.fail("declaration specifier");
                }
                if parser.eat(";")? {
                    let declaration = parser.node(
                        Declaration {
                            specifiers,
                            declarators: Vec::new(),
                        },
                        start,
                    )?;
                    ExternalDeclaration::Declaration(declaration)
                } else {
                    let declarator_start = parser.position();
                    parser.env.begin_function_definition();
                    let result = parser.declarator(DeclaratorMode::Named);
                    let mut declarator = match result {
                        Ok(declarator) => declarator,
                        Err(()) => {
                            parser.env.finish_function_definition(None);
                            return Err(());
                        }
                    };
                    if parser.definition_follows_declarator()? {
                        if is_typedef || is_auto {
                            parser.env.finish_function_definition(None);
                            return parser.fail("function definition");
                        }
                        parser
                            .env
                            .handle_declarator(&declarator, Symbol::Identifier);
                        let body = parser.scoped(|parser| {
                            parser.env.finish_function_definition(Some(&declarator));
                            let extensions = parser.attribute_specifier_list()?;
                            if !extensions.is_empty() {
                                if !parser.env.extensions_clang {
                                    return parser.fail("function definition");
                                }
                                declarator.node.extensions.extend(extensions);
                                declarator = parser.node_span(declarator.node, declarator.span)?;
                            }
                            let mut declarations = Vec::new();
                            while !parser.at("{") {
                                declarations.push(parser.declaration()?);
                            }
                            let statement = parser.compound_body()?;
                            parser.node(
                                FunctionDefinition {
                                    specifiers,
                                    declarator,
                                    declarations,
                                    statement,
                                },
                                start,
                            )
                        });
                        ExternalDeclaration::FunctionDefinition(body?)
                    } else {
                        parser.env.finish_function_definition(None);
                        let mut declarators = vec![parser.finish_init_declarator(
                            declarator,
                            declarator_start,
                            is_typedef,
                            is_auto,
                        )?];
                        while parser.eat(",")? {
                            let declarator_start = parser.position();
                            let declarator = parser.declarator(DeclaratorMode::Named)?;
                            declarators.push(parser.finish_init_declarator(
                                declarator,
                                declarator_start,
                                is_typedef,
                                is_auto,
                            )?);
                        }
                        parser.expect(";")?;
                        ExternalDeclaration::Declaration(parser.node(
                            Declaration {
                                specifiers,
                                declarators,
                            },
                            start,
                        )?)
                    }
                }
            };
            if parser.env.extensions_gnu {
                while parser.eat(";")? {}
            }
            parser.node(external, start)
        })
    }

    /// Distinguishes the shared declaration/definition prefix without parsing it twice.
    /// Attributes must see the definition's parameter scope, while a prototype's
    /// parameter scope ends before the declaration's trailing attributes.
    fn definition_follows_declarator(&mut self) -> PResult<bool> {
        let mut distance = 0;
        loop {
            let token = self.peek(distance);
            if !self.env.extensions_gnu
                || !matches!(self.token_text(token), "__attribute" | "__attribute__")
            {
                return Ok(self.token_text(token) == "{"
                    || token.kind == TokenKind::Identifier
                        && !matches!(self.token_text(token), "asm" | "__asm" | "__asm__"));
            }
            if !self.budget.step(token.span.start) {
                return Err(());
            }
            distance += 1;
            if self.token_text(self.peek(distance)) != "(" {
                return Ok(false);
            }
            let mut depth = 0usize;
            loop {
                let token = self.peek(distance);
                if token.kind == TokenKind::End {
                    return Ok(false);
                }
                if !self.budget.step(token.span.start) {
                    return Err(());
                }
                match self.token_text(token) {
                    "(" => depth += 1,
                    ")" => depth -= 1,
                    _ => {}
                }
                distance += 1;
                if depth == 0 {
                    break;
                }
            }
        }
    }

    fn extension_prefix(&mut self) -> PResult<()> {
        if self.env.extensions_gnu {
            self.eat("__extension__")?;
        }
        Ok(())
    }

    /// Registers ordinary identifiers before trailing extensions, typedefs after
    /// extensions, and GNU `__auto_type` names after their initializer, so each
    /// operand sees the appropriate scope.
    fn finish_init_declarator(
        &mut self,
        mut declarator: Node<Declarator>,
        start: usize,
        is_typedef: bool,
        is_auto: bool,
    ) -> PResult<Node<InitDeclarator>> {
        if !is_auto && !is_typedef {
            self.env.handle_declarator(&declarator, Symbol::Identifier);
        }
        let extensions = self.declarator_suffix_extensions()?;
        if !extensions.is_empty() {
            declarator.node.extensions.extend(extensions);
            declarator = self.node_span(declarator.node, declarator.span)?;
        }
        if is_typedef {
            self.env.handle_declarator(&declarator, Symbol::Typename);
        }
        let initializer = if self.at("=") {
            if is_typedef {
                return self.fail("typedef declarator without an initializer");
            }
            let start = self.position();
            self.bump()?;
            let initializer = self.initializer()?;
            Some(self.node(initializer.node, start)?)
        } else {
            None
        };
        if is_auto {
            self.env.handle_declarator(&declarator, Symbol::Identifier);
        }
        self.node(
            InitDeclarator {
                declarator,
                initializer,
            },
            start,
        )
    }

    fn declaration_specifiers(
        &mut self,
        context: DeclarationContext,
    ) -> PResult<Vec<Node<DeclarationSpecifier>>> {
        let mut specifiers = Vec::new();
        let mut seen_type = None;
        let mut seen_typedef = false;
        let mut seen_auto = false;
        loop {
            let start = self.position();
            let specifier = if let Some(storage) = self.storage_class() {
                if storage == StorageClassSpecifier::Typedef {
                    if seen_typedef || seen_auto || context == DeclarationContext::Parameter {
                        break;
                    }
                    seen_typedef = true;
                }
                self.bump()?;
                DeclarationSpecifier::StorageClass(self.node(storage, start)?)
            } else if self.is_type_qualifier() {
                DeclarationSpecifier::TypeQualifier(self.type_qualifier()?)
            } else if let Some(function) = self.function_specifier_value() {
                self.bump()?;
                DeclarationSpecifier::Function(self.node(function, start)?)
            } else if self.at("_Alignas") {
                DeclarationSpecifier::Alignment(self.alignment_specifier()?)
            } else if self.starts_attribute() {
                DeclarationSpecifier::Extension(self.attribute_specifier()?)
            } else if self.starts_declspec() {
                DeclarationSpecifier::Extension(self.declspec_specifier()?)
            } else if self.is_calling_convention() {
                DeclarationSpecifier::Extension(vec![self.calling_convention()?])
            } else if let Some(class) = self.type_class() {
                if seen_type.is_some()
                    && (class == TypeClass::Unique || seen_type == Some(TypeClass::Unique))
                {
                    break;
                }
                seen_auto |= self.env.extensions_gnu && self.at("__auto_type");
                seen_type = Some(
                    if seen_auto && !seen_typedef && context != DeclarationContext::Parameter {
                        TypeClass::Arithmetic
                    } else {
                        class
                    },
                );
                DeclarationSpecifier::TypeSpecifier(self.type_specifier()?)
            } else {
                break;
            };
            specifiers.push(self.node(specifier, start)?);
        }
        if seen_type.is_none()
            && (self.env.standard != Standard::C90
                || specifiers.is_empty() && context != DeclarationContext::File)
        {
            return self.fail("declaration specifier");
        }
        Ok(specifiers)
    }

    fn specifier_qualifiers(&mut self) -> PResult<Vec<Node<SpecifierQualifier>>> {
        let mut specifiers = Vec::new();
        let mut seen_type = None;
        loop {
            let start = self.position();
            let specifier = if self.is_type_qualifier() {
                SpecifierQualifier::TypeQualifier(self.type_qualifier()?)
            } else if self.at("_Alignas") {
                SpecifierQualifier::Alignment(self.alignment_specifier()?)
            } else if self.starts_attribute() {
                SpecifierQualifier::Extension(self.attribute_specifier()?)
            } else if self.starts_declspec() {
                SpecifierQualifier::Extension(self.declspec_specifier()?)
            } else if self.is_calling_convention() {
                SpecifierQualifier::Extension(vec![self.calling_convention()?])
            } else if let Some(class) = self.type_class() {
                if seen_type.is_some()
                    && (class == TypeClass::Unique || seen_type == Some(TypeClass::Unique))
                {
                    break;
                }
                seen_type = Some(class);
                SpecifierQualifier::TypeSpecifier(self.type_specifier()?)
            } else {
                break;
            };
            specifiers.push(self.node(specifier, start)?);
        }
        if seen_type.is_none() && (self.env.standard != Standard::C90 || specifiers.is_empty()) {
            return self.fail("type specifier");
        }
        Ok(specifiers)
    }

    fn storage_class(&self) -> Option<StorageClassSpecifier> {
        self.storage_class_at(0)
    }

    fn storage_class_at(&self, distance: usize) -> Option<StorageClassSpecifier> {
        Some(match self.token_text(self.peek(distance)) {
            "typedef" => StorageClassSpecifier::Typedef,
            "extern" => StorageClassSpecifier::Extern,
            "static" => StorageClassSpecifier::Static,
            "_Thread_local" => StorageClassSpecifier::ThreadLocal,
            "__thread" if self.env.extensions_gnu => StorageClassSpecifier::GnuThreadLocal,
            "auto" => StorageClassSpecifier::Auto,
            "register" => StorageClassSpecifier::Register,
            _ => return None,
        })
    }

    fn function_specifier_value(&self) -> Option<FunctionSpecifier> {
        self.function_specifier_at(0)
    }

    fn function_specifier_at(&self, distance: usize) -> Option<FunctionSpecifier> {
        Some(match self.token_text(self.peek(distance)) {
            "inline" if self.env.standard != Standard::C90 || self.env.gnu_keywords => {
                FunctionSpecifier::Inline
            }
            "__inline" | "__inline__" if self.env.extensions_gnu => FunctionSpecifier::Inline,
            "__forceinline" if self.env.extensions_msvc => FunctionSpecifier::Inline,
            "_Noreturn" => FunctionSpecifier::Noreturn,
            _ => return None,
        })
    }

    fn type_class(&self) -> Option<TypeClass> {
        self.type_class_at(0)
    }

    fn type_class_at(&self, distance: usize) -> Option<TypeClass> {
        let token = self.peek(distance);
        match self.token_text(token) {
            "void" | "_Bool" | "struct" | "union" | "enum" => Some(TypeClass::Unique),
            "_Atomic" if self.token_text(self.peek(distance + 1)) == "(" => Some(TypeClass::Unique),
            "__auto_type" | "__bf16" if self.env.extensions_gnu => Some(TypeClass::Unique),
            "char" | "short" | "int" | "long" | "float" | "double" | "signed" | "unsigned"
            | "_Complex" => Some(TypeClass::Arithmetic),
            "__signed" | "__signed__" | "__complex" | "__complex__" | "__typeof" | "__typeof__"
                if self.env.extensions_gnu =>
            {
                Some(TypeClass::Arithmetic)
            }
            "typeof" if self.env.gnu_keywords => Some(TypeClass::Arithmetic),
            "__float128" if self.env.extensions_clang && !self.env.gnu_float128_typedef => {
                Some(TypeClass::Arithmetic)
            }
            name if self.env.extensions_msvc && msvc_integer_width(name).is_some() => {
                Some(TypeClass::Arithmetic)
            }
            name if ts18661_type(name).is_some_and(|ty| self.env.is_ts18661_keyword(&ty)) => {
                Some(TypeClass::Arithmetic)
            }
            name if token.kind == TokenKind::Identifier
                && !self.env.reserved.contains(name)
                && self.env.is_typename(name) =>
            {
                Some(TypeClass::Unique)
            }
            _ => None,
        }
    }

    fn type_specifier(&mut self) -> PResult<Node<TypeSpecifier>> {
        self.nested(|parser| {
            let start = parser.position();
            let specifier = match parser.text() {
                "void" => TypeSpecifier::Void,
                "_Bool" => TypeSpecifier::Bool,
                "char" => TypeSpecifier::Char,
                "short" => TypeSpecifier::Short,
                "int" => TypeSpecifier::Int,
                "long" => TypeSpecifier::Long,
                "float" => TypeSpecifier::Float,
                "double" => TypeSpecifier::Double,
                "signed" => TypeSpecifier::Signed,
                "__signed" | "__signed__" if parser.env.extensions_gnu => TypeSpecifier::Signed,
                "unsigned" => TypeSpecifier::Unsigned,
                "_Complex" => TypeSpecifier::Complex,
                "__complex" | "__complex__" if parser.env.extensions_gnu => TypeSpecifier::Complex,
                "__auto_type" if parser.env.extensions_gnu => TypeSpecifier::AutoType,
                "__bf16" if parser.env.extensions_gnu => TypeSpecifier::BFloat16,
                "__float128" if parser.env.extensions_clang && !parser.env.gnu_float128_typedef => {
                    TypeSpecifier::Float128
                }
                "_Atomic" => {
                    parser.bump()?;
                    parser.expect("(")?;
                    let ty = parser.type_name()?;
                    parser.expect(")")?;
                    return parser.node(TypeSpecifier::Atomic(ty), start);
                }
                "struct" | "union" => {
                    let record = parser.struct_specifier()?;
                    return parser.node(TypeSpecifier::Struct(record), start);
                }
                "enum" => {
                    let enumeration = parser.enum_specifier()?;
                    return parser.node(TypeSpecifier::Enum(enumeration), start);
                }
                name if name == "typeof" && parser.env.gnu_keywords
                    || matches!(name, "__typeof" | "__typeof__") && parser.env.extensions_gnu =>
                {
                    parser.bump()?;
                    parser.expect("(")?;
                    let inner_start = parser.position();
                    let operand = if parser.starts_type_name() {
                        TypeOf::Type(parser.type_name()?)
                    } else {
                        TypeOf::Expression(parser.expression()?)
                    };
                    let operand = parser.node(operand, inner_start)?;
                    parser.expect(")")?;
                    return parser.node(TypeSpecifier::TypeOf(operand), start);
                }
                name if parser.env.extensions_msvc && msvc_integer_width(name).is_some() => {
                    TypeSpecifier::MsvcInteger(msvc_integer_width(name).unwrap())
                }
                name if ts18661_type(name).is_some_and(|ty| parser.env.is_ts18661_keyword(&ty)) => {
                    TypeSpecifier::TS18661Float(ts18661_type(name).unwrap())
                }
                _ => {
                    let identifier = parser.identifier()?;
                    if !parser.env.is_typename(&identifier.node.name) {
                        return parser.fail("typedef name");
                    }
                    return parser.node(TypeSpecifier::TypedefName(identifier), start);
                }
            };
            parser.bump()?;
            parser.node(specifier, start)
        })
    }

    pub(super) fn is_type_qualifier(&self) -> bool {
        self.type_qualifier_value().is_some()
    }

    fn type_qualifier_value(&self) -> Option<TypeQualifier> {
        self.type_qualifier_at(0)
    }

    fn type_qualifier_at(&self, distance: usize) -> Option<TypeQualifier> {
        Some(match self.token_text(self.peek(distance)) {
            "const" => TypeQualifier::Const,
            "__const" | "__const__" if self.env.extensions_gnu => TypeQualifier::Const,
            "restrict" if self.env.standard != Standard::C90 => TypeQualifier::Restrict,
            "__restrict" | "__restrict__" if self.env.extensions_gnu => TypeQualifier::Restrict,
            "volatile" => TypeQualifier::Volatile,
            "__volatile" | "__volatile__" if self.env.extensions_gnu => TypeQualifier::Volatile,
            "_unaligned" | "__unaligned" if self.env.extensions_msvc => TypeQualifier::Unaligned,
            "_Nonnull" if self.env.extensions_clang => TypeQualifier::Nonnull,
            "_Null_unspecified" if self.env.extensions_clang => TypeQualifier::NullUnspecified,
            "_Nullable" if self.env.extensions_clang => TypeQualifier::Nullable,
            "_Atomic" if self.token_text(self.peek(distance + 1)) != "(" => TypeQualifier::Atomic,
            _ => return None,
        })
    }

    pub(super) fn type_qualifier(&mut self) -> PResult<Node<TypeQualifier>> {
        let start = self.position();
        let Some(qualifier) = self.type_qualifier_value() else {
            return self.fail("type qualifier");
        };
        self.bump()?;
        self.node(qualifier, start)
    }

    fn alignment_specifier(&mut self) -> PResult<Node<AlignmentSpecifier>> {
        let start = self.position();
        self.expect("_Alignas")?;
        self.expect("(")?;
        let alignment = if self.starts_type_name() {
            AlignmentSpecifier::Type(self.type_name()?)
        } else {
            AlignmentSpecifier::Constant(Box::new(self.conditional_expression()?))
        };
        self.expect(")")?;
        self.node(alignment, start)
    }

    fn struct_specifier(&mut self) -> PResult<Node<StructType>> {
        let start = self.position();
        let kind = if self.eat("struct")? {
            StructKind::Struct
        } else {
            self.expect("union")?;
            StructKind::Union
        };
        let kind = self.node(kind, start)?;
        let extensions = self.tag_extension_specifier_list()?;
        let identifier = if self.token().kind == TokenKind::Identifier
            && !self.env.reserved.contains(self.text())
        {
            Some(self.identifier()?)
        } else {
            None
        };
        let declarations = if self.eat("{")? {
            let mut declarations = Vec::new();
            while !self.at("}") {
                declarations.push(self.struct_declaration()?);
            }
            if declarations.is_empty() && !self.env.extensions_gnu {
                return self.fail("struct declaration");
            }
            self.expect("}")?;
            Some(declarations)
        } else if identifier.is_some() {
            None
        } else {
            return self.fail("struct name or body");
        };
        self.node(
            StructType {
                kind,
                identifier,
                declarations,
                extensions,
            },
            start,
        )
    }

    /// Parses one record declaration. Named field declarators retain their original
    /// spans when trailing attributes are attached.
    fn struct_declaration(&mut self) -> PResult<Node<StructDeclaration>> {
        let start = self.position();
        if self.env.extensions_gnu {
            while self.eat("__extension__")? {}
        }
        let declaration = if self.at("_Static_assert") {
            StructDeclaration::StaticAssert(self.static_assert()?)
        } else {
            let field_start = self.position();
            let specifiers = self.specifier_qualifiers()?;
            let mut declarators = Vec::new();
            if !self.at(";") {
                loop {
                    let declarator_start = self.position();
                    let declarator = if self.at(":") {
                        None
                    } else {
                        Some(self.declarator(DeclaratorMode::Named)?)
                    };
                    let bit_width = if self.eat(":")? {
                        Some(Box::new(self.conditional_expression()?))
                    } else {
                        None
                    };
                    let extensions = self.attribute_specifier_list()?;
                    let declarator = if let Some(mut declarator) = declarator {
                        declarator.node.extensions.extend(extensions);
                        Some(self.node_span(declarator.node, declarator.span)?)
                    } else {
                        None
                    };
                    declarators.push(self.node(
                        StructDeclarator {
                            declarator,
                            bit_width,
                        },
                        declarator_start,
                    )?);
                    if !self.eat(",")? {
                        break;
                    }
                }
            }
            self.expect(";")?;
            if self.env.extensions_gnu {
                while self.eat(";")? {}
            }
            StructDeclaration::Field(self.node(
                StructField {
                    specifiers,
                    declarators,
                },
                field_start,
            )?)
        };
        self.node(declaration, start)
    }

    fn enum_specifier(&mut self) -> PResult<Node<EnumType>> {
        let start = self.position();
        self.expect("enum")?;
        let extensions = self.tag_extension_specifier_list()?;
        let identifier = if self.token().kind == TokenKind::Identifier
            && !self.env.reserved.contains(self.text())
        {
            Some(self.identifier()?)
        } else {
            None
        };
        let mut enumerators = Vec::new();
        if self.eat("{")? {
            loop {
                let enumerator_start = self.position();
                let identifier = self.identifier()?;
                let extensions = self.attribute_specifier_list()?;
                let expression = if self.eat("=")? {
                    Some(Box::new(self.conditional_expression()?))
                } else {
                    None
                };
                self.env
                    .add_symbol(&identifier.node.name, Symbol::Identifier);
                enumerators.push(self.node(
                    Enumerator {
                        identifier,
                        expression,
                        extensions,
                    },
                    enumerator_start,
                )?);
                if !self.eat(",")? || self.at("}") {
                    break;
                }
            }
            self.expect("}")?;
        } else if identifier.is_none() {
            return self.fail("enum name or body");
        }
        self.node(
            EnumType {
                identifier,
                enumerators,
                extensions,
            },
            start,
        )
    }

    pub(super) fn type_name(&mut self) -> PResult<Node<TypeName>> {
        self.nested(|parser| {
            let start = parser.position();
            let specifiers = parser.specifier_qualifiers()?;
            let declarator = if parser.starts_abstract_declarator() {
                Some(parser.declarator(DeclaratorMode::Abstract)?)
            } else {
                None
            };
            parser.node(
                TypeName {
                    specifiers,
                    declarator,
                },
                start,
            )
        })
    }

    fn starts_abstract_declarator(&self) -> bool {
        self.at("*")
            || self.at("(")
            || self.at("[")
            || self.env.extensions_clang && self.at("^")
            || self.is_calling_convention()
    }

    fn declarator(&mut self, mode: DeclaratorMode) -> PResult<Node<Declarator>> {
        self.nested(|parser| parser.declarator_inner(mode))
    }

    fn declarator_inner(&mut self, mode: DeclaratorMode) -> PResult<Node<Declarator>> {
        let start = self.position();
        let mut extensions = Vec::new();
        while self.starts_attribute() || self.is_calling_convention() {
            if self.starts_attribute() {
                extensions.extend(self.attribute_specifier()?);
            } else {
                extensions.push(self.calling_convention()?);
            }
        }
        let mut derived = Vec::new();
        while self.at("*") || self.env.extensions_clang && self.at("^") {
            let pointer_start = self.position();
            let block = self.eat("^")?;
            if !block {
                self.expect("*")?;
            }
            let mut qualifiers = Vec::new();
            loop {
                let qualifier_start = self.position();
                let qualifier = if self.is_type_qualifier() {
                    PointerQualifier::TypeQualifier(self.type_qualifier()?)
                } else if self.is_calling_convention() {
                    PointerQualifier::Extension(vec![self.calling_convention()?])
                } else if self.env.extensions_msvc && (self.at("__ptr32") || self.at("__ptr64")) {
                    let width = if self.at("__ptr32") { 32 } else { 64 };
                    self.bump()?;
                    PointerQualifier::MsvcPointerWidth(width)
                } else if self.starts_attribute() {
                    PointerQualifier::Extension(self.attribute_specifier()?)
                } else {
                    break;
                };
                qualifiers.push(self.node(qualifier, qualifier_start)?);
            }
            derived.push(self.node(
                if block {
                    DerivedDeclarator::Block(qualifiers)
                } else {
                    DerivedDeclarator::Pointer(qualifiers)
                },
                pointer_start,
            )?);
        }
        let kind_start = self.position();
        let kind = if self.token().kind == TokenKind::Identifier
            && !self.env.reserved.contains(self.text())
            && mode != DeclaratorMode::Abstract
        {
            let identifier = self.identifier()?;
            self.node(DeclaratorKind::Identifier(identifier), kind_start)?
        } else if self.at("(")
            && (mode == DeclaratorMode::Named || self.parenthesized_declarator(mode))
        {
            self.bump()?;
            let declarator = self.declarator(mode)?;
            self.expect(")")?;
            self.node(DeclaratorKind::Declarator(Box::new(declarator)), kind_start)?
        } else if mode != DeclaratorMode::Named {
            self.node_span(DeclaratorKind::Abstract, Span::span(kind_start, kind_start))?
        } else {
            return self.fail("declarator");
        };
        while self.at("[") || self.at("(") {
            let derived_start = self.position();
            let value = if self.eat("[")? {
                DerivedDeclarator::Array(self.array_declarator()?)
            } else {
                self.expect("(")?;
                self.function_declarator_suffix(find_declarator_name(&kind.node).is_none())?
            };
            derived.push(self.node(value, derived_start)?);
        }
        if matches!(kind.node, DeclaratorKind::Abstract) && derived.is_empty() {
            return self.fail("abstract declarator");
        }
        self.node(
            Declarator {
                kind,
                derived,
                extensions,
            },
            start,
        )
    }

    fn parenthesized_declarator(&self, mode: DeclaratorMode) -> bool {
        let next = self.peek(1);
        let text = self.token_text(next);
        matches!(text, "*" | "^" | "(" | "[")
            || next.kind == TokenKind::Identifier
                && !self.env.reserved.contains(text)
                && mode != DeclaratorMode::Abstract
                // C17 6.7.6.3p11 resolves an ambiguous parenthesized parameter
                // as a typedef name, not a redeclaration of that name.
                && !self.env.is_typename(text)
            || self.calling_convention_name(text)
            || self.env.extensions_gnu && matches!(text, "__attribute" | "__attribute__")
    }

    fn array_declarator(&mut self) -> PResult<Node<ArrayDeclarator>> {
        let start = self.position();
        let mut is_static = self.eat("static")?;
        let mut qualifiers = Vec::new();
        while self.is_type_qualifier() {
            qualifiers.push(self.type_qualifier()?);
        }
        if !is_static && !qualifiers.is_empty() {
            is_static = self.eat("static")?;
        }
        let size = if self.at("]") && !is_static {
            ArraySize::Unknown
        } else if self.at("*") && self.token_text(self.peek(1)) == "]" && !is_static {
            self.bump()?;
            ArraySize::VariableUnknown
        } else {
            let expression = Box::new(self.assignment_expression()?);
            if is_static {
                ArraySize::StaticExpression(expression)
            } else {
                ArraySize::VariableExpression(expression)
            }
        };
        self.expect("]")?;
        self.node(ArrayDeclarator { qualifiers, size }, start)
    }

    fn function_declarator_suffix(&mut self, abstract_allowed: bool) -> PResult<DerivedDeclarator> {
        let start = self.position();
        if self.at(")") {
            let result = if abstract_allowed {
                DerivedDeclarator::Function(self.node_span(
                    FunctionDeclarator {
                        parameters: Vec::new(),
                        ellipsis: Ellipsis::None,
                    },
                    Span::span(start, start),
                )?)
            } else {
                DerivedDeclarator::KRFunction(Vec::new())
            };
            self.expect(")")?;
            return Ok(result);
        }
        if !self.starts_declaration() && !abstract_allowed {
            let mut parameters = Vec::new();
            loop {
                parameters.push(self.identifier()?);
                if !self.eat(",")? {
                    break;
                }
            }
            self.expect(")")?;
            return Ok(DerivedDeclarator::KRFunction(parameters));
        }
        self.env.enter_scope();
        let function = (|| {
            let mut parameters = Vec::new();
            let mut ellipsis = Ellipsis::None;
            loop {
                parameters.push(self.parameter_declaration()?);
                if !self.eat(",")? {
                    break;
                }
                if self.eat("...")? {
                    ellipsis = Ellipsis::Some;
                    break;
                }
            }
            self.node(
                FunctionDeclarator {
                    parameters,
                    ellipsis,
                },
                start,
            )
        })();
        self.env.leave_function_scope(function.as_ref().ok());
        let function = function?;
        self.expect(")")?;
        Ok(DerivedDeclarator::Function(function))
    }

    fn parameter_declaration(&mut self) -> PResult<Node<ParameterDeclaration>> {
        let start = self.position();
        let specifiers = self.declaration_specifiers(DeclarationContext::Parameter)?;
        let declarator = if self.starts_abstract_declarator()
            || self.token().kind == TokenKind::Identifier
                && !self.env.reserved.contains(self.text())
        {
            let declarator = self.declarator(DeclaratorMode::Parameter)?;
            self.env.handle_declarator(&declarator, Symbol::Identifier);
            Some(declarator)
        } else {
            None
        };
        let extensions = self.attribute_specifier_list()?;
        self.node(
            ParameterDeclaration {
                specifiers,
                declarator,
                extensions,
            },
            start,
        )
    }

    pub(super) fn initializer(&mut self) -> PResult<Node<Initializer>> {
        self.nested(|parser| {
            let start = parser.position();
            let initializer = if parser.at("{") {
                Initializer::List(parser.initializer_list()?)
            } else {
                Initializer::Expression(Box::new(parser.assignment_expression()?))
            };
            parser.node(initializer, start)
        })
    }

    pub(super) fn initializer_list(&mut self) -> PResult<Vec<Node<InitializerListItem>>> {
        self.expect("{")?;
        let mut items = Vec::new();
        if self.at("}") && !self.env.extensions_gnu {
            return self.fail("initializer");
        }
        while !self.at("}") {
            let start = self.position();
            let mut designation = Vec::new();
            let mut colon = false;
            if self.env.extensions_gnu
                && self.token().kind == TokenKind::Identifier
                && self.token_text(self.peek(1)) == ":"
            {
                let member = self.identifier()?;
                self.expect(":")?;
                designation.push(self.node(Designator::Member(member), start)?);
                colon = true;
            } else {
                while self.at("[") || self.at(".") {
                    let designator_start = self.position();
                    let designator = if self.eat(".")? {
                        Designator::Member(self.identifier()?)
                    } else {
                        self.expect("[")?;
                        let from = self.conditional_expression()?;
                        let designator = if self.env.extensions_gnu && self.eat("...")? {
                            let to = self.conditional_expression()?;
                            let span = Span::span(from.span.start, to.span.end);
                            Designator::Range(self.node_span(RangeDesignator { from, to }, span)?)
                        } else {
                            Designator::Index(from)
                        };
                        self.expect("]")?;
                        designator
                    };
                    designation.push(self.node(designator, designator_start)?);
                }
            }
            if !designation.is_empty()
                && !colon
                && !self.eat("=")?
                && !(self.env.extensions_gnu
                    && designation.len() == 1
                    && matches!(
                        designation[0].node,
                        Designator::Index(_) | Designator::Range(_)
                    ))
            {
                return self.fail("=");
            }
            let initializer = Box::new(self.initializer()?);
            items.push(self.node(
                InitializerListItem {
                    designation,
                    initializer,
                },
                start,
            )?);
            if !self.eat(",")? {
                break;
            }
        }
        self.expect("}")?;
        Ok(items)
    }

    pub(super) fn static_assert(&mut self) -> PResult<Node<StaticAssert>> {
        let start = self.position();
        self.extension_prefix()?;
        self.expect("_Static_assert")?;
        self.expect("(")?;
        let expression = Box::new(self.conditional_expression()?);
        self.expect(",")?;
        let message = self.string_literal()?;
        self.expect(")")?;
        self.expect(";")?;
        self.node(
            StaticAssert {
                expression,
                message,
            },
            start,
        )
    }

    pub(super) fn starts_attribute(&self) -> bool {
        self.env.extensions_gnu && matches!(self.text(), "__attribute" | "__attribute__")
    }

    pub(super) fn attribute_specifier_list(&mut self) -> PResult<Vec<Node<Extension>>> {
        let mut attributes = Vec::new();
        while self.starts_attribute() {
            attributes.extend(self.attribute_specifier()?);
        }
        Ok(attributes)
    }

    pub(super) fn attribute_specifier(&mut self) -> PResult<Vec<Node<Extension>>> {
        if !self.starts_attribute() {
            return self.fail("GNU attribute");
        }
        self.bump()?;
        self.expect("(")?;
        self.expect("(")?;
        let mut attributes = Vec::new();
        while !self.at(")") {
            let start = self.position();
            let attribute = if self.env.extensions_clang
                && self.at("availability")
                && self.token_text(self.peek(1)) == "("
            {
                Extension::AvailabilityAttribute(self.availability_attribute()?)
            } else {
                let name = self.attribute_name(false)?;
                let arguments = self.attribute_arguments()?;
                Extension::Attribute(Attribute { name, arguments })
            };
            attributes.push(self.node(attribute, start)?);
            if !self.eat(",")? {
                break;
            }
        }
        self.expect(")")?;
        self.expect(")")?;
        Ok(attributes)
    }

    fn starts_declspec(&self) -> bool {
        self.env.extensions_msvc && matches!(self.text(), "_declspec" | "__declspec")
    }

    fn tag_extension_specifier_list(&mut self) -> PResult<Vec<Node<Extension>>> {
        let mut attributes = Vec::new();
        loop {
            if self.starts_attribute() {
                attributes.extend(self.attribute_specifier()?);
            } else if self.starts_declspec() {
                attributes.extend(self.declspec_specifier()?);
            } else {
                break;
            }
        }
        Ok(attributes)
    }

    fn declspec_specifier(&mut self) -> PResult<Vec<Node<Extension>>> {
        self.bump()?;
        self.expect("(")?;
        let mut attributes = Vec::new();
        while !self.at(")") {
            if self.eat(",")? {
                continue;
            }
            let start = self.position();
            let name = self.attribute_name(true)?;
            let arguments = self.attribute_arguments()?;
            attributes.push(self.node(Extension::Declspec(Attribute { name, arguments }), start)?);
        }
        self.expect(")")?;
        Ok(attributes)
    }

    fn attribute_name(&mut self, string_allowed: bool) -> PResult<Node<String>> {
        let token = self.token();
        if token.kind != TokenKind::Identifier
            && !(string_allowed && token.kind == TokenKind::String)
        {
            return self.fail("attribute name");
        }
        let name = self.text().to_owned();
        self.bump()?;
        self.node_span(name, token.span)
    }

    fn attribute_arguments(&mut self) -> PResult<Vec<Node<Expression>>> {
        let mut arguments = Vec::new();
        if self.eat("(")? {
            if !self.at(")") {
                loop {
                    arguments.push(self.assignment_expression()?);
                    if !self.eat(",")? {
                        break;
                    }
                }
            }
            self.expect(")")?;
        }
        Ok(arguments)
    }

    /// Preserves availability clauses in source order, including repeated clauses.
    fn availability_attribute(&mut self) -> PResult<Node<AvailabilityAttribute>> {
        let start = self.position();
        self.expect("availability")?;
        self.expect("(")?;
        let platform = self.identifier()?;
        self.expect(",")?;
        let mut clauses = Vec::new();
        loop {
            let clause_start = self.position();
            let name = self.text();
            self.bump()?;
            let clause = match name {
                "introduced" | "deprecated" | "obsoleted" => {
                    self.expect("=")?;
                    let version = self.availability_version()?;
                    match name {
                        "introduced" => AvailabilityClause::Introduced(version),
                        "deprecated" => AvailabilityClause::Deprecated(version),
                        _ => AvailabilityClause::Obsoleted(version),
                    }
                }
                "unavailable" => AvailabilityClause::Unavailable,
                "message" | "replacement" => {
                    self.expect("=")?;
                    let message = self.string_literal()?;
                    if name == "message" {
                        AvailabilityClause::Message(message)
                    } else {
                        AvailabilityClause::Replacement(message)
                    }
                }
                _ => return self.fail("availability clause"),
            };
            clauses.push(self.node(clause, clause_start)?);
            if !self.eat(",")? {
                break;
            }
        }
        self.expect(")")?;
        self.node(AvailabilityAttribute { platform, clauses }, start)
    }

    fn availability_version(&mut self) -> PResult<Node<AvailabilityVersion>> {
        let start = self.position();
        let text = self.text();
        let mut components = text.split('.');
        let major = components.next().unwrap_or("");
        let minor = components.next();
        let subminor = components.next();
        if components.next().is_some()
            || major.is_empty()
            || !major.bytes().all(|byte| byte.is_ascii_digit())
            || [minor, subminor]
                .iter()
                .flatten()
                .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return self.fail("availability version");
        }
        let version = AvailabilityVersion {
            major: major.to_owned(),
            minor: minor.map(str::to_owned),
            subminor: subminor.map(str::to_owned),
        };
        self.bump()?;
        self.node(version, start)
    }

    fn is_calling_convention(&self) -> bool {
        self.calling_convention_name(self.text())
    }

    fn calling_convention_name(&self, text: &str) -> bool {
        (self.env.clang_calling_conventions || self.env.extensions_msvc)
            && matches!(
                text,
                "__cdecl"
                    | "__stdcall"
                    | "__fastcall"
                    | "__thiscall"
                    | "__vectorcall"
                    | "__regcall"
                    | "__pascal"
            )
            || self.env.extensions_msvc
                && matches!(
                    text,
                    "_cdecl" | "_stdcall" | "_fastcall" | "_thiscall" | "_vectorcall"
                )
    }

    fn calling_convention(&mut self) -> PResult<Node<Extension>> {
        let start = self.position();
        let name = self.text().to_owned();
        self.bump()?;
        let name = self.node(name, start)?;
        self.node(
            Extension::CallingConvention(Attribute {
                name,
                arguments: Vec::new(),
            }),
            start,
        )
    }

    fn declarator_suffix_extensions(&mut self) -> PResult<Vec<Node<Extension>>> {
        let mut extensions = Vec::new();
        if self.env.extensions_gnu
            && (matches!(self.text(), "__asm" | "__asm__")
                || self.env.gnu_keywords && self.at("asm"))
        {
            let start = self.position();
            self.bump()?;
            self.expect("(")?;
            let name = self.string_literal()?;
            self.expect(")")?;
            extensions.push(self.node(Extension::AsmLabel(name), start)?);
        }
        extensions.extend(self.attribute_specifier_list()?);
        Ok(extensions)
    }
}

fn declaration_flags(specifiers: &[Node<DeclarationSpecifier>]) -> (bool, bool) {
    (
        specifiers.iter().any(|specifier| {
            matches!(
                specifier.node,
                DeclarationSpecifier::StorageClass(Node {
                    node: StorageClassSpecifier::Typedef,
                    ..
                })
            )
        }),
        specifiers.iter().any(|specifier| {
            matches!(
                specifier.node,
                DeclarationSpecifier::TypeSpecifier(Node {
                    node: TypeSpecifier::AutoType,
                    ..
                })
            )
        }),
    )
}

fn msvc_integer_width(name: &str) -> Option<u8> {
    Some(match name {
        "_int8" | "__int8" => 8,
        "_int16" | "__int16" => 16,
        "_int32" | "__int32" => 32,
        "_int64" | "__int64" => 64,
        _ => return None,
    })
}

/// Decodes a TS 18661 type spelling and width. Callers decide whether the selected
/// dialect recognizes it as a keyword.
fn ts18661_type(name: &str) -> Option<TS18661FloatType> {
    let (width, binary) = if let Some(width) = name.strip_prefix("_Float") {
        (width, true)
    } else {
        (name.strip_prefix("_Decimal")?, false)
    };
    let (width, extended) = width
        .strip_suffix('x')
        .map_or((width, false), |width| (width, true));
    let width = match width {
        "16" if binary => 16,
        "32" => 32,
        "64" => 64,
        "128" => 128,
        _ => return None,
    };
    Some(ts18661_float(binary, width, extended))
}

#[cfg(test)]
mod tests {
    use ast::{
        DeclarationSpecifier, DeclaratorKind, DerivedDeclarator, Extension, ExternalDeclaration,
        TypeOf, TypeSpecifier,
    };
    use driver::{self, Config, Flavor, Standard};
    use span::Span;
    use visit::{self, Visit};

    #[derive(Default)]
    struct TypeOfOperands(Vec<bool>);

    impl<'ast> Visit<'ast> for TypeOfOperands {
        fn visit_type_specifier(&mut self, value: &'ast TypeSpecifier, span: &'ast Span) {
            if let TypeSpecifier::TypeOf(operand) = value {
                self.0.push(matches!(operand.node, TypeOf::Type(_)));
            }
            visit::visit_type_specifier(self, value, span);
        }
    }

    #[test]
    fn tag_attributes_preserve_written_positions() {
        for source in [
            "struct __attribute__((aligned(sizeof(int)))) S { int x; };",
            "union __attribute__((aligned(sizeof(int)))) U { int x; };",
            "enum __attribute__((aligned(sizeof(int)))) E { A };",
        ] {
            for config in [Config::with_gcc(), Config::with_clang()] {
                let parsed = driver::parse_preprocessed(&config, source.to_owned()).unwrap();
                let ExternalDeclaration::Declaration(declaration) = &parsed.unit.0[0].node else {
                    panic!("expected declaration");
                };
                let DeclarationSpecifier::TypeSpecifier(specifier) =
                    &declaration.node.specifiers[0].node
                else {
                    panic!("expected type specifier");
                };
                let extensions = match &specifier.node {
                    TypeSpecifier::Struct(tag) => &tag.node.extensions,
                    TypeSpecifier::Enum(tag) => &tag.node.extensions,
                    _ => panic!("expected tag"),
                };
                assert_eq!(extensions.len(), 1);
                let Extension::Attribute(attribute) = &extensions[0].node else {
                    panic!("expected GNU attribute");
                };
                assert_eq!(attribute.name.node, "aligned");
                let span = attribute.arguments[0].span;
                assert_eq!(&source[span.start..span.end], "sizeof(int)");
            }
            let core = Config {
                flavor: Flavor::StdC11,
                ..Config::with_gcc()
            };
            assert!(driver::parse_preprocessed(&core, source.to_owned()).is_err());
        }
    }

    #[test]
    fn inactive_extension_spellings_remain_typedef_names() {
        #[derive(Default)]
        struct TypedefNames(Vec<String>);

        impl<'ast> Visit<'ast> for TypedefNames {
            fn visit_type_specifier(&mut self, value: &'ast TypeSpecifier, span: &'ast Span) {
                if let TypeSpecifier::TypedefName(identifier) = value {
                    self.0.push(identifier.node.name.clone());
                }
                visit::visit_type_specifier(self, value, span);
            }
        }

        let core = Config {
            flavor: Flavor::StdC11,
            gnu_keywords: false,
            ..Config::with_gcc()
        };
        let mut cases = vec![
            (
                core,
                vec![
                    "__signed",
                    "__signed__",
                    "__complex",
                    "__complex__",
                    "__auto_type",
                    "__bf16",
                    "__typeof",
                    "__typeof__",
                    "__const",
                    "__restrict",
                    "__inline",
                    "__thread",
                    "__attribute__",
                    "__extension__",
                ],
            ),
            (
                Config::with_clang(),
                vec![
                    "_Float32",
                    "_Float64",
                    "_Float32x",
                    "_Float64x",
                    "_Float128",
                ],
            ),
        ];
        for mut config in [Config::with_gcc(), Config::with_clang()] {
            cases.push((
                config.clone(),
                vec![
                    "_int8",
                    "__int8",
                    "__int16",
                    "__int32",
                    "__int64",
                    "__declspec",
                    "__forceinline",
                    "__unaligned",
                ],
            ));
            config.gnu_keywords = false;
            for standard in [Standard::C90, Standard::C99, Standard::C11, Standard::C17] {
                config.standard = standard;
                cases.push((config.clone(), vec!["asm", "typeof"]));
            }
        }
        for (config, names) in cases {
            for name in names {
                let source = format!("typedef int {0}; {0} value;", name);
                let parsed = driver::parse_preprocessed(&config, source.clone())
                    .unwrap_or_else(|error| panic!("{}: {}", source, error));
                let mut typedefs = TypedefNames::default();
                typedefs.visit_translation_unit(&parsed.unit);
                assert_eq!(typedefs.0, [name], "{}", source);
            }
        }
    }

    #[test]
    fn trailing_definition_attributes_use_the_selected_parameter_scope() {
        for (source, expected) in [
            (
                "typedef double T; int f(int T) __attribute__((aligned(sizeof(__typeof__(T))))) { return T; }",
                false,
            ),
            (
                "typedef double T; int f(int T) __attribute__((aligned(sizeof(__typeof__(T)))));",
                true,
            ),
            (
                "typedef double T; int (*f(int T))(int) __attribute__((aligned(sizeof(__typeof__(T))))) { return 0; }",
                false,
            ),
            (
                "typedef double T; int f(int (*callback)(int T)) __attribute__((aligned(sizeof(__typeof__(T))))) { return 0; }",
                true,
            ),
        ] {
            let parsed = driver::parse_preprocessed(&Config::with_clang(), source.to_owned()).unwrap();
            let mut operands = TypeOfOperands::default();
            operands.visit_translation_unit(&parsed.unit);
            assert_eq!(operands.0, [expected], "{}", source);
        }
    }

    #[test]
    fn typedef_and_inferred_type_specifiers_keep_their_grammar_context() {
        for config in [Config::with_gcc(), Config::with_clang()] {
            for source in [
                "int f(typedef int x);",
                "__auto_type typedef x = 1;",
                "__auto_type int typedef x = 1;",
                "typedef __auto_type int T;",
                "int f(__auto_type int x);",
            ] {
                assert!(
                    driver::parse_preprocessed(&config, source.to_owned()).is_err(),
                    "{}",
                    source
                );
            }
            // Type-combination validity remains a semantic constraint, as for
            // other arithmetic specifier combinations in a declaration.
            for source in [
                "__auto_type int x = 1;",
                "__auto_type unsigned x = 1;",
                "__auto_type const int x = 1;",
            ] {
                driver::parse_preprocessed(&config, source.to_owned()).unwrap();
            }
        }
    }

    #[test]
    fn extension_prefix_distinguishes_declarations_from_expressions() {
        let source = "typedef int T; void f(void) { int n = 1; __extension__ int value; __extension__ ({ __auto_type p = &n; __typeof__((void)0, *p) v = 2; *p = v; }); __extension__ ++n; __extension__ (T)n; }";
        for config in [Config::with_gcc(), Config::with_clang()] {
            driver::parse_preprocessed(&config, source.to_owned()).unwrap();
        }
    }

    #[test]
    fn type_names_reject_names_but_parameters_can_shadow_typedefs() {
        for source in [
            "typedef int T; void f(int T) { __typeof__(T) local; }",
            "typedef int T; void f(int (T)) { __typeof__(T) local; }",
            "typedef int T; void f(int (*T)(void)) { __typeof__(T) local; }",
            "typedef int T; void f(int (*)(T));",
        ] {
            assert!(
                driver::parse_preprocessed(&Config::with_clang(), source.to_owned()).is_ok(),
                "{}",
                source
            );
        }
        for source in [
            "int f(void) { return sizeof(int accidental); }",
            "int f(void) { return sizeof(int (*accidental)(void)); }",
        ] {
            assert!(
                driver::parse_preprocessed(&Config::with_clang(), source.to_owned()).is_err(),
                "{}",
                source
            );
        }
    }

    #[test]
    fn parenthesized_parameter_typedefs_denote_function_types() {
        for config in [Config::with_gcc(), Config::with_clang()] {
            for parameter in ["int (T)", "int ((T))", "int (T[4])", "int (T, T)"] {
                let source = format!("typedef int T; void f({}, T value);", parameter);
                driver::parse_preprocessed(&config, source.clone())
                    .unwrap_or_else(|error| panic!("{}: {}", source, error));
            }

            let source = "typedef int T; void f(int (T));";
            let parsed = driver::parse_preprocessed(&config, source.to_owned()).unwrap();
            let ExternalDeclaration::Declaration(declaration) = &parsed.unit.0[1].node else {
                panic!("expected declaration");
            };
            let DerivedDeclarator::Function(function) =
                &declaration.node.declarators[0].node.declarator.node.derived[0].node
            else {
                panic!("expected function");
            };
            let parameter = function.node.parameters[0]
                .node
                .declarator
                .as_ref()
                .unwrap();
            assert!(matches!(parameter.node.kind.node, DeclaratorKind::Abstract));
            let DerivedDeclarator::Function(callback) = &parameter.node.derived[0].node else {
                panic!("expected function parameter");
            };
            let DeclarationSpecifier::TypeSpecifier(specifier) =
                &callback.node.parameters[0].node.specifiers[0].node
            else {
                panic!("expected typedef specifier");
            };
            assert!(
                matches!(&specifier.node, TypeSpecifier::TypedefName(name) if name.node.name == "T")
            );

            // Explicit pointer declarators still redeclare the typedef name.
            let source = "typedef int T; void f(int (*T)) { __typeof__(T) local; }";
            let parsed = driver::parse_preprocessed(&config, source.to_owned()).unwrap();
            let mut operands = TypeOfOperands::default();
            operands.visit_translation_unit(&parsed.unit);
            assert_eq!(operands.0, [false]);
        }
    }
}
