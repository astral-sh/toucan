use std::collections::{BTreeMap, HashMap, HashSet};

use lang_c::{ast, driver, span::Node};
use toucan_target::Target;

use crate::{
    CallingConvention, Declaration, DeclarationKind, Enum, EnumVariant, Error, Field, FloatKind,
    FunctionType, IntegerKind, IntegerValue, Parameter, Qualifiers, Record, RecordKind, Scope,
    TranslationUnit, Type, TypeKind,
};

type PackEvents = Vec<(usize, Option<u64>)>;

/// Parses and analyzes preprocessed C declarations without invoking a subprocess.
///
/// Pack pragmas are interpreted before parsing. Function bodies are syntax-checked
/// only; the returned declarations mark definitions so binding generators can omit
/// inline functions without pretending that an external symbol exists.
pub fn analyze(source: &str, target: Target) -> Result<TranslationUnit, Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let (source, packs) = prepare_source(source)?;
    let parsed = parse(&source)?;
    let mut analyzer = Analyzer::new(target, packs);
    analyzer.record_attributes = parsed.record_attributes;
    for external in parsed.unit.0 {
        match external.node {
            ast::ExternalDeclaration::Declaration(declaration) => {
                analyzer.declaration(&declaration, false)?
            }
            ast::ExternalDeclaration::StaticAssert(assertion) => {
                analyzer.static_assert(&assertion)?
            }
            ast::ExternalDeclaration::FunctionDefinition(definition) => {
                if !definition.node.declarations.is_empty() {
                    return Err(Error::new(
                        definition.span.start,
                        "K&R function definitions are unsupported",
                    ));
                }
                let declaration = Node::new(
                    ast::Declaration {
                        specifiers: definition.node.specifiers,
                        declarators: vec![Node::new(
                            ast::InitDeclarator {
                                declarator: definition.node.declarator,
                                initializer: None,
                            },
                            definition.span,
                        )],
                    },
                    definition.span,
                );
                analyzer.declaration(&declaration, true)?;
            }
        }
    }
    Ok(analyzer.unit)
}

/// Evaluates an integer constant expression in the translation unit's type and
/// enumerator environment, using the target's C integer conversion rules.
pub fn evaluate_integer(unit: &TranslationUnit, expression: &str) -> Result<IntegerValue, Error> {
    validate_expression_source(expression)?;
    for value in unit.constants.values() {
        value.validate()?;
    }
    // Only the typedef *names* matter to the parser. The semantic environment below
    // retains their real types, avoiding reparsing every header for each macro.
    let mut source = String::new();
    let mut expected_declarations = 1;
    for name in unit.typedefs.keys() {
        if name != "__builtin_va_list" {
            if !name
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err(Error::new(
                    0,
                    "invalid typedef identifier in evaluation environment",
                ));
            }
            source.push_str("typedef int ");
            source.push_str(name);
            source.push_str(";\n");
            expected_declarations += 1;
        }
    }
    let expression_offset = source.len() + "int __toucan_expression = (".len();
    source.push_str("int __toucan_expression = (");
    source.push_str(expression);
    source.push_str(");\n");
    let parsed = parse(&source).map_err(|mut error| {
        error.offset = error.offset.saturating_sub(expression_offset);
        error
    })?;
    if parsed.unit.0.len() != expected_declarations {
        return Err(Error::new(0, "input is not a single integer expression"));
    }
    let last = parsed
        .unit
        .0
        .last()
        .ok_or_else(|| Error::new(0, "missing integer expression"))?;
    let ast::ExternalDeclaration::Declaration(declaration) = &last.node else {
        return Err(Error::new(0, "invalid integer expression"));
    };
    let [declarator] = declaration.node.declarators.as_slice() else {
        return Err(Error::new(0, "input is not a single integer expression"));
    };
    if !matches!(&declarator.node.declarator.node.kind.node, ast::DeclaratorKind::Identifier(identifier) if identifier.node.name == "__toucan_expression")
    {
        return Err(Error::new(0, "input is not a single integer expression"));
    }
    let initializer = &declarator.node.initializer;
    let Some(Node {
        node: ast::Initializer::Expression(expression),
        ..
    }) = initializer
    else {
        return Err(Error::new(0, "expected integer expression"));
    };
    let mut analyzer = Analyzer::from_unit(unit.clone());
    analyzer.record_attributes = parsed.record_attributes;
    analyzer.eval(expression).map_err(|mut error| {
        error.offset = error.offset.saturating_sub(expression_offset);
        error
    })
}

struct Parsed {
    unit: ast::TranslationUnit,
    record_attributes: HashSet<usize>,
}

fn parse(source: &str) -> Result<Parsed, Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let source = strip_comments(source)?;
    check_parse_limits(&source)?;
    let config = driver::Config {
        cpp_command: String::new(),
        cpp_options: Vec::new(),
        flavor: driver::Flavor::ClangC11,
    };
    let (source, record_attributes) = normalize_attributes(&source);
    let parsed = driver::parse_preprocessed(&config, source)
        .map_err(|error| Error::new(error.offset, format!("C syntax error: {error}")))?;
    Ok(Parsed {
        unit: parsed.unit,
        record_attributes,
    })
}

/// lang-c expects comments to have been replaced in translation phase three.
fn strip_comments(source: &str) -> Result<String, Error> {
    let mut bytes = source.as_bytes().to_vec();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    bytes[index] = b' ';
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let start = index;
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                if index + 1 >= bytes.len() {
                    return Err(Error::new(start, "unterminated C comment"));
                }
                index += 1;
                for byte in &mut bytes[start..=index] {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    Ok(String::from_utf8(bytes).expect("replacing comment bytes preserves UTF-8"))
}

/// Keeps a macro replacement inside the expression wrapper used to parse it.
/// Balanced delimiters and the absence of declaration separators are checked
/// independently of the parser, including comments and quoted literals.
fn validate_expression_source(expression: &str) -> Result<(), Error> {
    let bytes = expression.as_bytes();
    let mut index = 0;
    let mut delimiters = Vec::new();
    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
                if index >= bytes.len() {
                    return Err(Error::new(
                        index,
                        "unterminated literal in integer expression",
                    ));
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                if index + 1 >= bytes.len() {
                    return Err(Error::new(
                        index,
                        "unterminated comment in integer expression",
                    ));
                }
                index += 1;
            }
            b'(' | b'[' => delimiters.push(bytes[index]),
            b')' | b']' => {
                let expected = if bytes[index] == b')' { b'(' } else { b'[' };
                if delimiters.pop() != Some(expected) {
                    return Err(Error::new(
                        index,
                        "unbalanced delimiter in integer expression",
                    ));
                }
            }
            b';' | b'{' | b'}' | b'#' => {
                return Err(Error::new(
                    index,
                    "declarations and statements are not integer expressions",
                ));
            }
            _ => {}
        }
        index += 1;
    }
    if !delimiters.is_empty() {
        return Err(Error::new(
            expression.len(),
            "unbalanced delimiter in integer expression",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Default)]
struct Attributes {
    packed: bool,
    alignment: Option<u64>,
    link_name: Option<String>,
    mode: Option<String>,
}

#[derive(Clone, Copy)]
enum Tag {
    Record(usize),
    Enum(usize),
}

#[derive(Clone, Copy)]
struct TagBinding {
    tag: Tag,
    depth: usize,
}

/// Only bindings introduced in a prototype need to be saved and restored.
/// Existing file-scope maps are shared even for headers with many prototypes.
#[derive(Default)]
struct PrototypeScope {
    tags: Vec<(String, Option<TagBinding>)>,
    constants: Vec<(String, Option<IntegerValue>)>,
    /// A parameter index, or None for an enumerator in the ordinary namespace.
    names: HashMap<String, Option<usize>>,
    parameters: Vec<Parameter>,
}

pub(crate) struct Analyzer {
    pub(crate) unit: TranslationUnit,
    tags: HashMap<String, TagBinding>,
    prototype_scopes: Vec<PrototypeScope>,
    packs: PackEvents,
    record_attributes: HashSet<usize>,
    nesting: usize,
}

impl Analyzer {
    pub(crate) fn enter_expression(&mut self, offset: usize) -> Result<(), Error> {
        if self.nesting >= 128 {
            return Err(Error::new(offset, "expression nesting limit exceeded"));
        }
        self.nesting += 1;
        Ok(())
    }

    pub(crate) fn leave_expression(&mut self) {
        self.nesting -= 1;
    }

    fn new(target: Target, packs: PackEvents) -> Self {
        let mut analyzer = Self::from_unit(TranslationUnit {
            target,
            declarations: Vec::new(),
            records: Vec::new(),
            enums: Vec::new(),
            typedefs: BTreeMap::new(),
            constants: BTreeMap::new(),
        });
        analyzer.packs = packs;
        analyzer.install_builtin_va_list();
        analyzer
    }

    fn from_unit(unit: TranslationUnit) -> Self {
        let tags = unit
            .records
            .iter()
            .enumerate()
            .filter(|(_, record)| record.scope == Scope::File)
            .filter_map(|(id, record)| record.name.clone().map(|name| (name, Tag::Record(id))))
            .chain(
                unit.enums
                    .iter()
                    .enumerate()
                    .filter(|(_, value)| value.scope == Scope::File)
                    .filter_map(|(id, value)| value.name.clone().map(|name| (name, Tag::Enum(id)))),
            )
            .map(|(name, tag)| (name, TagBinding { tag, depth: 0 }))
            .collect();
        Self {
            unit,
            tags,
            prototype_scopes: Vec::new(),
            packs: Vec::new(),
            record_attributes: HashSet::new(),
            nesting: 0,
        }
    }

    fn scope(&self) -> Scope {
        if self.prototype_scopes.is_empty() {
            Scope::File
        } else {
            Scope::Prototype
        }
    }

    fn bind_tag(&mut self, name: String, tag: Tag) {
        let previous = self.tags.insert(
            name.clone(),
            TagBinding {
                tag,
                depth: self.prototype_scopes.len(),
            },
        );
        if let Some(scope) = self.prototype_scopes.last_mut() {
            scope.tags.push((name, previous));
        }
    }

    pub(crate) fn parameter_type(&self, name: &str) -> Option<&Type> {
        self.prototype_scopes
            .iter()
            .rev()
            .find_map(|scope| {
                scope
                    .names
                    .get(name)
                    .map(|index| index.map(|index| &scope.parameters[index].ty))
            })
            .flatten()
    }

    fn leave_prototype(&mut self) -> Vec<Parameter> {
        let scope = self
            .prototype_scopes
            .pop()
            .expect("prototype scope is active");
        for (name, previous) in scope.tags.into_iter().rev() {
            if let Some(previous) = previous {
                self.tags.insert(name, previous);
            } else {
                self.tags.remove(&name);
            }
        }
        for (name, previous) in scope.constants.into_iter().rev() {
            if let Some(previous) = previous {
                self.unit.constants.insert(name, previous);
            } else {
                self.unit.constants.remove(&name);
            }
        }
        scope.parameters
    }

    fn install_builtin_va_list(&mut self) {
        let pointer = Type::new(TypeKind::Void).pointer();
        let (fields, array) = match self.unit.target.triple() {
            "x86_64-unknown-linux-gnu" | "x86_64-apple-darwin" => (
                vec![
                    (
                        "gp_offset",
                        Type::new(TypeKind::Integer(IntegerKind::UnsignedInt)),
                    ),
                    (
                        "fp_offset",
                        Type::new(TypeKind::Integer(IntegerKind::UnsignedInt)),
                    ),
                    ("overflow_arg_area", pointer.clone()),
                    ("reg_save_area", pointer.clone()),
                ],
                true,
            ),
            "aarch64-unknown-linux-gnu" => (
                vec![
                    ("__stack", pointer.clone()),
                    ("__gr_top", pointer.clone()),
                    ("__vr_top", pointer.clone()),
                    ("__gr_offs", Type::new(TypeKind::Integer(IntegerKind::Int))),
                    ("__vr_offs", Type::new(TypeKind::Integer(IntegerKind::Int))),
                ],
                false,
            ),
            _ => {
                self.unit.typedefs.insert(
                    "__builtin_va_list".into(),
                    Type::new(TypeKind::Integer(IntegerKind::Char)).pointer(),
                );
                return;
            }
        };
        let id = self.unit.records.len();
        self.unit.records.push(Record {
            name: Some("__toucan_va_list_tag".into()),
            scope: Scope::File,
            kind: RecordKind::Struct,
            fields: Some(
                fields
                    .into_iter()
                    .map(|(name, ty)| Field {
                        name: Some(name.into()),
                        ty,
                        bit_width: None,
                        alignment: None,
                        packed: false,
                    })
                    .collect(),
            ),
            packed: false,
            alignment: None,
            pack: None,
        });
        let ty = Type::new(TypeKind::Record(id));
        let ty = if array {
            Type::new(TypeKind::Array {
                element: Box::new(ty),
                length: Some(1),
            })
        } else {
            ty
        };
        self.unit.typedefs.insert("__builtin_va_list".into(), ty);
    }

    fn declaration(
        &mut self,
        declaration: &Node<ast::Declaration>,
        definition: bool,
    ) -> Result<(), Error> {
        // glibc defines the TS spellings as typedefs for older compiler profiles.
        // lang-c recognizes their spelling as a type specifier even in this
        // declaration position, so recover the explicit typedef name here.
        if declaration.node.declarators.is_empty()
            && declaration.node.specifiers.iter().any(|specifier| matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(storage) if storage.node == ast::StorageClassSpecifier::Typedef))
            && let Some(Node { node: ast::DeclarationSpecifier::TypeSpecifier(Node { node: ast::TypeSpecifier::TS18661Float(float), .. }), .. }) = declaration.node.specifiers.last()
        {
            let name = extended_float_name(float);
            let (ty, attributes) = self.specifiers(&declaration.node.specifiers[..declaration.node.specifiers.len()-1])?;
            if attributes.packed || attributes.alignment.is_some() || attributes.mode.is_some() { return Err(Error::new(declaration.span.start, "attributes on extended float compatibility typedefs are unsupported")); }
            if self.unit.typedefs.insert(name.clone(), ty.clone()).is_some() { return Err(Error::new(declaration.span.start, "duplicate extended float typedef")); }
            self.unit.declarations.push(Declaration { name, ty, kind: DeclarationKind::Typedef, link_name: None, is_static: false, is_definition: false });
            return Ok(());
        }
        let (base, attributes) = self.specifiers(&declaration.node.specifiers)?;
        let is_typedef = declaration.node.specifiers.iter().any(|specifier| matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(s) if s.node == ast::StorageClassSpecifier::Typedef));
        let is_static = declaration.node.specifiers.iter().any(|specifier| matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(s) if s.node == ast::StorageClassSpecifier::Static));
        if declaration.node.specifiers.iter().any(|specifier| matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(s) if s.node == ast::StorageClassSpecifier::ThreadLocal)) {
            return Err(Error::new(declaration.span.start, "thread-local objects require unsupported Rust TLS bindings"));
        }
        for item in &declaration.node.declarators {
            let (name, mut ty, mut declarator_attributes) =
                self.declarator(base.clone(), &item.node.declarator)?;
            let name =
                name.ok_or_else(|| Error::new(item.span.start, "declaration has no name"))?;
            if declarator_attributes.link_name.is_none() {
                declarator_attributes.link_name = attributes.link_name.clone();
            }
            if is_typedef {
                if item.node.initializer.is_some() {
                    return Err(Error::new(
                        item.span.start,
                        "a typedef cannot have an initializer",
                    ));
                }
                if attributes.alignment.is_some() || declarator_attributes.alignment.is_some() {
                    return Err(Error::new(
                        item.span.start,
                        "aligned typedefs are unsupported",
                    ));
                }
                if let Some(previous) = self.unit.typedefs.get(&name) {
                    if self.unit.resolve(previous)? != self.unit.resolve(&ty)? {
                        return Err(Error::new(
                            item.span.start,
                            format!("conflicting typedef `{name}`"),
                        ));
                    }
                } else {
                    self.unit.typedefs.insert(name.clone(), ty.clone());
                }
            }
            let kind = if is_typedef {
                DeclarationKind::Typedef
            } else if matches!(self.unit.resolve(&ty)?.kind, TypeKind::Function(_)) {
                DeclarationKind::Function
            } else {
                DeclarationKind::Variable
            };
            if definition && kind != DeclarationKind::Function {
                return Err(Error::new(
                    item.span.start,
                    "function definition requires a function declarator",
                ));
            }
            if !is_typedef && matches!(self.unit.resolve(&ty)?.kind, TypeKind::Void) {
                return Err(Error::new(
                    item.span.start,
                    "an object cannot have void type",
                ));
            }
            if let Some(previous_index) = self
                .unit
                .declarations
                .iter()
                .position(|previous| previous.name == name)
            {
                let previous = &self.unit.declarations[previous_index];
                if previous.kind != kind || !self.compatible(&previous.ty, &ty)? {
                    return Err(Error::new(
                        item.span.start,
                        format!("conflicting declaration of `{name}`"),
                    ));
                }
                // A composite type retains all available bounds and prototypes,
                // including those nested inside pointers and function parameters.
                ty = self.composite_type(&previous.ty, &ty, 0)?;
                let previous = &mut self.unit.declarations[previous_index];
                previous.ty = ty.clone();
                // Repeated compatible prototypes need only one binding.
                if !definition {
                    if previous.link_name.is_none() {
                        previous.link_name = declarator_attributes.link_name;
                    }
                    continue;
                }
            }
            self.unit.declarations.push(Declaration {
                name,
                ty,
                kind,
                link_name: declarator_attributes.link_name,
                is_static,
                is_definition: definition || item.node.initializer.is_some(),
            });
        }
        Ok(())
    }

    fn compatible(&self, left: &Type, right: &Type) -> Result<bool, Error> {
        self.compatible_at(left, right, 0)
    }

    fn compatible_at(&self, left: &Type, right: &Type, depth: usize) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "type compatibility nesting exceeds the 128-level limit",
            ));
        }
        if self.unit.qualifiers(left)? != self.unit.qualifiers(right)? {
            return Ok(false);
        }
        let left = self.unit.resolve(left)?;
        let right = self.unit.resolve(right)?;
        match (&left.kind, &right.kind) {
            (TypeKind::Pointer(left), TypeKind::Pointer(right)) => {
                self.compatible_at(left, right, depth + 1)
            }
            (
                TypeKind::Array {
                    element: left,
                    length: a,
                },
                TypeKind::Array {
                    element: right,
                    length: b,
                },
            ) => Ok((a == b || a.is_none() || b.is_none())
                && self.compatible_at(left, right, depth + 1)?),
            (TypeKind::Function(left), TypeKind::Function(right)) => {
                if !self.compatible_at(&left.return_type, &right.return_type, depth + 1)? {
                    return Ok(false);
                }
                if !left.prototype || !right.prototype {
                    let prototype = if left.prototype { left } else { right };
                    if prototype.variadic {
                        return Ok(false);
                    }
                    for parameter in &prototype.parameters {
                        if matches!(
                            self.unit.resolve(&parameter.ty)?.kind,
                            TypeKind::Bool
                                | TypeKind::Integer(
                                    IntegerKind::Char
                                        | IntegerKind::SignedChar
                                        | IntegerKind::UnsignedChar
                                        | IntegerKind::Short
                                        | IntegerKind::UnsignedShort
                                )
                                | TypeKind::Float(FloatKind::Float)
                        ) {
                            return Ok(false);
                        }
                    }
                    return Ok(true);
                }
                if left.variadic != right.variadic
                    || left.parameters.len() != right.parameters.len()
                {
                    return Ok(false);
                }
                for (a, b) in left.parameters.iter().zip(&right.parameters) {
                    // Top-level parameter qualifiers do not participate in the
                    // function type, but pointee qualifiers still do.
                    let mut a = self.unit.resolve(&a.ty)?.clone();
                    a.qualifiers = Qualifiers::default();
                    let mut b = self.unit.resolve(&b.ty)?.clone();
                    b.qualifiers = Qualifiers::default();
                    if !self.compatible_at(&a, &b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(left.kind == right.kind),
        }
    }

    /// Combines compatible declarations without losing nested type information.
    fn composite_type(&self, left: &Type, right: &Type, depth: usize) -> Result<Type, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "composite type nesting exceeds the 128-level limit",
            ));
        }
        if left == right {
            return Ok(left.clone());
        }
        let resolved_left = self.unit.resolve(left)?;
        let resolved_right = self.unit.resolve(right)?;
        let kind = match (&resolved_left.kind, &resolved_right.kind) {
            (TypeKind::Pointer(a), TypeKind::Pointer(b)) => {
                TypeKind::Pointer(Box::new(self.composite_type(a, b, depth + 1)?))
            }
            (
                TypeKind::Array {
                    element: a,
                    length: a_len,
                },
                TypeKind::Array {
                    element: b,
                    length: b_len,
                },
            ) => TypeKind::Array {
                element: Box::new(self.composite_type(a, b, depth + 1)?),
                length: a_len.or(*b_len),
            },
            (TypeKind::Function(a), TypeKind::Function(b)) => {
                let mut function = if a.prototype {
                    (**a).clone()
                } else {
                    (**b).clone()
                };
                function.return_type =
                    self.composite_type(&a.return_type, &b.return_type, depth + 1)?;
                if a.prototype && b.prototype {
                    for ((parameter, a), b) in function
                        .parameters
                        .iter_mut()
                        .zip(&a.parameters)
                        .zip(&b.parameters)
                    {
                        parameter.ty = self.composite_type(&a.ty, &b.ty, depth + 1)?;
                    }
                }
                TypeKind::Function(Box::new(function))
            }
            _ => return Ok(left.clone()),
        };
        if kind == resolved_left.kind {
            return Ok(left.clone());
        }
        Ok(Type {
            kind,
            qualifiers: self.unit.qualifiers(left)?,
        })
    }

    fn specifiers(
        &mut self,
        specifiers: &[Node<ast::DeclarationSpecifier>],
    ) -> Result<(Type, Attributes), Error> {
        let mut types = Vec::new();
        let mut qualifiers = Qualifiers::default();
        let mut attributes = Attributes::default();
        let mut record_attributes = Attributes::default();
        let mut after_tag_definition = false;
        for specifier in specifiers {
            match &specifier.node {
                ast::DeclarationSpecifier::TypeSpecifier(ty) => {
                    after_tag_definition = matches!(&ty.node, ast::TypeSpecifier::Struct(record) if record.node.declarations.is_some())
                        || matches!(&ty.node, ast::TypeSpecifier::Enum(enumeration) if !enumeration.node.enumerators.is_empty());
                    types.push(ty.clone());
                }
                ast::DeclarationSpecifier::TypeQualifier(qualifier) => {
                    add_qualifier(&mut qualifiers, qualifier)?
                }
                ast::DeclarationSpecifier::Extension(extensions) => {
                    if self.record_attributes.contains(&specifier.span.start)
                        || after_tag_definition
                    {
                        self.attributes(extensions, &mut record_attributes)?;
                    } else {
                        self.attributes(extensions, &mut attributes)?;
                    }
                }
                ast::DeclarationSpecifier::Alignment(alignment) => {
                    let value = match &alignment.node {
                        ast::AlignmentSpecifier::Type(ty) => {
                            let ty = self.type_name(&ty.node)?;
                            self.unit.layout(&ty)?.alignment_bytes()
                        }
                        ast::AlignmentSpecifier::Constant(expression) => {
                            self.eval(expression)?.as_u64()?
                        }
                    };
                    set_alignment(&mut attributes, value, alignment.span.start)?;
                }
                _ => {}
            }
        }
        let mut ty = self.base_type(&types)?;
        // Attributes on an object declaration do not change the canonical tag.
        // Normalization records attributes written between `struct` and its tag;
        // attributes following the closing brace also belong to the type.
        let defines_tag = types.iter().any(|ty| {
            matches!(&ty.node, ast::TypeSpecifier::Struct(record) if record.node.declarations.is_some())
                || matches!(&ty.node, ast::TypeSpecifier::Enum(enumeration) if !enumeration.node.enumerators.is_empty())
        });
        let clang_forward = matches!(
            self.unit.target,
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
        ) && matches!(self.unit.resolve(&ty)?.kind, TypeKind::Record(id) if self.unit.records[id].fields.is_none());
        // GCC ignores layout attributes on forward tags; Clang retains them.
        // Both ignore a new attribute applied after the tag is already defined.
        if defines_tag || clang_forward {
            self.apply_record_attributes(&ty, &record_attributes)?;
        }
        ty.qualifiers.is_const |= qualifiers.is_const;
        ty.qualifiers.is_volatile |= qualifiers.is_volatile;
        ty.qualifiers.is_restrict |= qualifiers.is_restrict;
        if let Some(mode) = &attributes.mode {
            ty = self.machine_mode(ty, mode, types.first().map_or(0, |ty| ty.span.start))?;
        }
        Ok((ty, attributes))
    }

    fn specifier_qualifiers(
        &mut self,
        specifiers: &[Node<ast::SpecifierQualifier>],
    ) -> Result<(Type, Attributes), Error> {
        let specifiers = specifiers
            .iter()
            .map(|specifier| {
                Node::new(
                    match &specifier.node {
                        ast::SpecifierQualifier::TypeSpecifier(ty) => {
                            ast::DeclarationSpecifier::TypeSpecifier(ty.clone())
                        }
                        ast::SpecifierQualifier::TypeQualifier(qualifier) => {
                            ast::DeclarationSpecifier::TypeQualifier(qualifier.clone())
                        }
                        ast::SpecifierQualifier::Extension(extensions) => {
                            ast::DeclarationSpecifier::Extension(extensions.clone())
                        }
                    },
                    specifier.span,
                )
            })
            .collect::<Vec<_>>();
        self.specifiers(&specifiers)
    }

    fn base_type(&mut self, types: &[Node<ast::TypeSpecifier>]) -> Result<Type, Error> {
        let offset = types.first().map_or(0, |ty| ty.span.start);
        let mut long = 0;
        let mut short = false;
        let mut signed = false;
        let mut unsigned = false;
        let mut char_ = false;
        let mut float = false;
        let mut double = false;
        let mut int = false;
        let mut special = None;
        for ty in types {
            match &ty.node {
                ast::TypeSpecifier::Long => long += 1,
                ast::TypeSpecifier::Short if !short => short = true,
                ast::TypeSpecifier::Signed if !signed => signed = true,
                ast::TypeSpecifier::Unsigned if !unsigned => unsigned = true,
                ast::TypeSpecifier::Char if !char_ => char_ = true,
                ast::TypeSpecifier::Float if !float => float = true,
                ast::TypeSpecifier::Double if !double => double = true,
                ast::TypeSpecifier::Int if !int => int = true,
                value => {
                    let kind = match value {
                        ast::TypeSpecifier::Void => TypeKind::Void,
                        ast::TypeSpecifier::Bool => TypeKind::Bool,
                        ast::TypeSpecifier::TypedefName(name) => {
                            if !self.unit.typedefs.contains_key(&name.node.name) {
                                return Err(Error::new(
                                    ty.span.start,
                                    format!("unknown typedef `{}`", name.node.name),
                                ));
                            }
                            TypeKind::Typedef(name.node.name.clone())
                        }
                        ast::TypeSpecifier::Struct(record) => {
                            TypeKind::Record(self.record(record)?)
                        }
                        ast::TypeSpecifier::Enum(value) => TypeKind::Enum(self.enum_type(value)?),
                        ast::TypeSpecifier::TypeOf(value) => match &value.node {
                            ast::TypeOf::Type(ty) => return self.type_name(&ty.node),
                            ast::TypeOf::Expression(expression) => {
                                return self.expression_type(expression);
                            }
                        },
                        ast::TypeSpecifier::Atomic(_) => {
                            return Err(Error::new(
                                ty.span.start,
                                "atomic type ABI is unsupported",
                            ));
                        }
                        ast::TypeSpecifier::Complex => {
                            return Err(Error::new(
                                ty.span.start,
                                "complex type ABI is unsupported",
                            ));
                        }
                        ast::TypeSpecifier::TS18661Float(float) => {
                            let name = extended_float_name(float);
                            if self.unit.typedefs.contains_key(&name) {
                                TypeKind::Typedef(name)
                            } else {
                                TypeKind::Float(FloatKind::Extended {
                                    format: match float.format {
                                        ast::TS18661FloatFormat::BinaryInterchange => {
                                            crate::ExtendedFloatFormat::BinaryInterchange
                                        }
                                        ast::TS18661FloatFormat::BinaryExtended => {
                                            crate::ExtendedFloatFormat::BinaryExtended
                                        }
                                        ast::TS18661FloatFormat::DecimalInterchange => {
                                            crate::ExtendedFloatFormat::DecimalInterchange
                                        }
                                        ast::TS18661FloatFormat::DecimalExtended => {
                                            crate::ExtendedFloatFormat::DecimalExtended
                                        }
                                    },
                                    width: float.width,
                                })
                            }
                        }
                        _ => {
                            return Err(Error::new(
                                ty.span.start,
                                "duplicate or invalid type specifier",
                            ));
                        }
                    };
                    if special.replace(kind).is_some() {
                        return Err(Error::new(ty.span.start, "conflicting type specifiers"));
                    }
                }
            }
        }
        if let Some(special) = special {
            if long > 0 || short || signed || unsigned || char_ || float || double || int {
                return Err(Error::new(offset, "invalid modifiers on type"));
            }
            return Ok(Type::new(special));
        }
        if types.is_empty()
            || long > 2
            || (long > 0 && short)
            || (signed && unsigned)
            || (char_ && (long > 0 || short || int || float || double))
            || (float && (double || long > 0 || short || signed || unsigned || int))
            || (double && (long > 1 || short || signed || unsigned || int))
        {
            return Err(Error::new(offset, "invalid C type specifier combination"));
        }
        Ok(Type::new(if float {
            TypeKind::Float(FloatKind::Float)
        } else if double {
            TypeKind::Float(if long == 1 {
                FloatKind::LongDouble
            } else {
                FloatKind::Double
            })
        } else {
            TypeKind::Integer(if char_ {
                if unsigned {
                    IntegerKind::UnsignedChar
                } else if signed {
                    IntegerKind::SignedChar
                } else {
                    IntegerKind::Char
                }
            } else if short {
                if unsigned {
                    IntegerKind::UnsignedShort
                } else {
                    IntegerKind::Short
                }
            } else if long == 1 {
                if unsigned {
                    IntegerKind::UnsignedLong
                } else {
                    IntegerKind::Long
                }
            } else if long == 2 {
                if unsigned {
                    IntegerKind::UnsignedLongLong
                } else {
                    IntegerKind::LongLong
                }
            } else if unsigned {
                IntegerKind::UnsignedInt
            } else {
                IntegerKind::Int
            })
        }))
    }

    pub(crate) fn type_name(&mut self, name: &ast::TypeName) -> Result<Type, Error> {
        let (ty, _) = self.specifier_qualifiers(&name.specifiers)?;
        if let Some(declarator) = &name.declarator {
            Ok(self.declarator(ty, declarator)?.1)
        } else {
            Ok(ty)
        }
    }

    fn declarator(
        &mut self,
        mut ty: Type,
        declaration: &Node<ast::Declarator>,
    ) -> Result<(Option<String>, Type, Attributes), Error> {
        if self.nesting >= 128 {
            return Err(Error::new(
                declaration.span.start,
                "declarator nesting limit exceeded",
            ));
        }
        self.nesting += 1;
        let split = declaration
            .node
            .derived
            .iter()
            .take_while(|derived| matches!(derived.node, ast::DerivedDeclarator::Pointer(_)))
            .count();
        for derived in declaration.node.derived[..split]
            .iter()
            .chain(declaration.node.derived[split..].iter().rev())
        {
            ty = match &derived.node {
                ast::DerivedDeclarator::Pointer(qualifiers) => {
                    let mut pointer = ty.pointer();
                    for qualifier in qualifiers {
                        match &qualifier.node {
                            ast::PointerQualifier::TypeQualifier(qualifier) => {
                                add_qualifier(&mut pointer.qualifiers, qualifier)?
                            }
                            ast::PointerQualifier::Extension(extensions) => {
                                self.attributes(extensions, &mut Attributes::default())?
                            }
                        }
                    }
                    pointer
                }
                ast::DerivedDeclarator::Array(array) => {
                    if matches!(
                        self.unit.resolve(&ty)?.kind,
                        TypeKind::Void | TypeKind::Function(_)
                    ) {
                        return Err(Error::new(derived.span.start, "invalid array element type"));
                    }
                    let length = match &array.node.size {
                        ast::ArraySize::Unknown => None,
                        ast::ArraySize::VariableExpression(expression)
                        | ast::ArraySize::StaticExpression(expression) => {
                            Some(self.eval(expression)?.as_u64()?)
                        }
                        ast::ArraySize::VariableUnknown => {
                            return Err(Error::new(
                                derived.span.start,
                                "variable-length array declarators are unsupported",
                            ));
                        }
                    };
                    Type::new(TypeKind::Array {
                        element: Box::new(ty),
                        length,
                    })
                }
                ast::DerivedDeclarator::Function(function) => {
                    if matches!(
                        self.unit.resolve(&ty)?.kind,
                        TypeKind::Array { .. } | TypeKind::Function(_)
                    ) {
                        return Err(Error::new(
                            derived.span.start,
                            "a function cannot return an array or function",
                        ));
                    }
                    self.prototype_scopes.push(PrototypeScope {
                        parameters: Vec::with_capacity(function.node.parameters.len()),
                        ..PrototypeScope::default()
                    });
                    let prototype = !function.node.parameters.is_empty();
                    for parameter in &function.node.parameters {
                        let (base, _) = self.specifiers(&parameter.node.specifiers)?;
                        let (name, mut parameter_type) =
                            if let Some(declarator) = &parameter.node.declarator {
                                let (name, ty, _) = self.declarator(base, declarator)?;
                                (name, ty)
                            } else {
                                (None, base)
                            };
                        self.attributes(&parameter.node.extensions, &mut Attributes::default())?;
                        let qualifiers = self.unit.qualifiers(&parameter_type)?;
                        parameter_type = match &self.unit.resolve(&parameter_type)?.kind {
                            TypeKind::Array { element, .. } => {
                                // Qualifying an array typedef qualifies its elements.
                                // Parameter adjustment removes only the array layer.
                                let mut element = (**element).clone();
                                element.qualifiers.is_const |= qualifiers.is_const;
                                element.qualifiers.is_volatile |= qualifiers.is_volatile;
                                element.qualifiers.is_restrict |= qualifiers.is_restrict;
                                element.pointer()
                            }
                            TypeKind::Function(_) => parameter_type.pointer(),
                            _ => parameter_type,
                        };
                        let scope = self
                            .prototype_scopes
                            .last_mut()
                            .expect("prototype scope is active");
                        if let Some(name) = &name {
                            if scope
                                .names
                                .insert(name.clone(), Some(scope.parameters.len()))
                                .is_some()
                            {
                                return Err(Error::new(
                                    parameter.span.start,
                                    format!("duplicate parameter `{name}`"),
                                ));
                            }
                            let previous = self.unit.constants.remove(name);
                            scope.constants.push((name.clone(), previous));
                        }
                        scope.parameters.push(Parameter {
                            name,
                            ty: parameter_type,
                        });
                    }
                    let mut parameters = self.leave_prototype();
                    if parameters.len() == 1
                        && parameters[0].name.is_none()
                        && matches!(self.unit.resolve(&parameters[0].ty)?.kind, TypeKind::Void)
                    {
                        parameters.clear();
                    }
                    if parameters.iter().any(|parameter| {
                        self.unit
                            .resolve(&parameter.ty)
                            .is_ok_and(|ty| matches!(ty.kind, TypeKind::Void))
                    }) {
                        return Err(Error::new(
                            derived.span.start,
                            "void must be the only unnamed function parameter",
                        ));
                    }
                    let variadic = function.node.ellipsis == ast::Ellipsis::Some;
                    if variadic && parameters.is_empty() {
                        return Err(Error::new(
                            derived.span.start,
                            "C11 variadic functions require a fixed parameter",
                        ));
                    }
                    Type::new(TypeKind::Function(Box::new(FunctionType {
                        return_type: ty,
                        parameters,
                        variadic,
                        prototype,
                        calling_convention: CallingConvention::C,
                    })))
                }
                ast::DerivedDeclarator::KRFunction(parameters) if parameters.is_empty() => {
                    Type::new(TypeKind::Function(Box::new(FunctionType {
                        return_type: ty,
                        parameters: Vec::new(),
                        variadic: false,
                        prototype: false,
                        calling_convention: CallingConvention::C,
                    })))
                }
                ast::DerivedDeclarator::KRFunction(_) => {
                    return Err(Error::new(
                        derived.span.start,
                        "K&R function declarations are unsupported",
                    ));
                }
                ast::DerivedDeclarator::Block(_) => {
                    return Err(Error::new(
                        derived.span.start,
                        "Clang block pointers are unsupported",
                    ));
                }
            };
        }
        let mut attributes = Attributes::default();
        self.attributes(&declaration.node.extensions, &mut attributes)?;
        if let Some(mode) = &attributes.mode {
            ty = self.machine_mode(ty, mode, declaration.span.start)?;
        }
        let result = match &declaration.node.kind.node {
            ast::DeclaratorKind::Identifier(identifier) => {
                (Some(identifier.node.name.clone()), ty, attributes)
            }
            ast::DeclaratorKind::Abstract => (None, ty, attributes),
            ast::DeclaratorKind::Declarator(inner) => {
                let (name, ty, inner_attributes) = self.declarator(ty, inner)?;
                if inner_attributes.packed {
                    attributes.packed = true;
                }
                if inner_attributes.alignment.is_some() {
                    attributes.alignment = inner_attributes.alignment;
                }
                if inner_attributes.link_name.is_some() {
                    attributes.link_name = inner_attributes.link_name;
                }
                (name, ty, attributes)
            }
        };
        self.nesting -= 1;
        Ok(result)
    }

    fn record(&mut self, declaration: &Node<ast::StructType>) -> Result<usize, Error> {
        let name = declaration
            .node
            .identifier
            .as_ref()
            .map(|name| name.node.name.clone());
        let kind = if declaration.node.kind.node == ast::StructKind::Struct {
            RecordKind::Struct
        } else {
            RecordKind::Union
        };
        let binding = name
            .as_ref()
            .and_then(|name| self.tags.get(name))
            .filter(|binding| {
                declaration.node.declarations.is_none()
                    || binding.depth == self.prototype_scopes.len()
            });
        let id = if let Some(binding) = binding {
            let Tag::Record(id) = binding.tag else {
                return Err(Error::new(
                    declaration.span.start,
                    "tag used as both record and enum",
                ));
            };
            if self.unit.records[id].kind != kind {
                return Err(Error::new(
                    declaration.span.start,
                    "tag used as both struct and union",
                ));
            }
            id
        } else {
            let id = self.unit.records.len();
            let pack = self
                .packs
                .iter()
                .take_while(|(offset, _)| *offset <= declaration.span.start)
                .last()
                .and_then(|(_, pack)| *pack);
            self.unit.records.push(Record {
                name: name.clone(),
                scope: self.scope(),
                kind,
                fields: None,
                packed: false,
                alignment: None,
                pack,
            });
            if let Some(name) = name {
                self.bind_tag(name, Tag::Record(id));
            }
            id
        };
        if let Some(declarations) = &declaration.node.declarations {
            if self.unit.records[id].fields.is_some() {
                return Err(Error::new(
                    declaration.span.start,
                    "record is defined more than once",
                ));
            }
            let mut fields = Vec::new();
            for declaration in declarations {
                match &declaration.node {
                    ast::StructDeclaration::StaticAssert(assertion) => {
                        self.static_assert(assertion)?
                    }
                    ast::StructDeclaration::Field(field) => {
                        let (base, attributes) =
                            self.specifier_qualifiers(&field.node.specifiers)?;
                        if field.node.declarators.is_empty() {
                            if !matches!(self.unit.resolve(&base)?.kind, TypeKind::Record(_)) {
                                return Err(Error::new(
                                    field.span.start,
                                    "anonymous field must be a struct or union",
                                ));
                            }
                            fields.push(Field {
                                name: None,
                                ty: base,
                                bit_width: None,
                                alignment: attributes.alignment,
                                packed: attributes.packed,
                            });
                        } else {
                            for declarator in &field.node.declarators {
                                let (name, ty, extra) =
                                    if let Some(declarator) = &declarator.node.declarator {
                                        self.declarator(base.clone(), declarator)?
                                    } else {
                                        (None, base.clone(), Attributes::default())
                                    };
                                if name.as_ref().is_some_and(|name| {
                                    fields.iter().any(|field| field.name.as_ref() == Some(name))
                                }) {
                                    return Err(Error::new(
                                        declarator.span.start,
                                        "duplicate field name",
                                    ));
                                }
                                let bit_width = declarator
                                    .node
                                    .bit_width
                                    .as_ref()
                                    .map(|expression| self.eval(expression)?.as_u64())
                                    .transpose()?;
                                if let Some(width) = bit_width {
                                    if !matches!(
                                        self.unit.resolve(&ty)?.kind,
                                        TypeKind::Integer(_) | TypeKind::Bool | TypeKind::Enum(_)
                                    ) {
                                        return Err(Error::new(
                                            declarator.span.start,
                                            "bitfield requires an integer type",
                                        ));
                                    }
                                    if width == 0 && name.is_some() {
                                        return Err(Error::new(
                                            declarator.span.start,
                                            "zero-width bitfield must be unnamed",
                                        ));
                                    }
                                    if width > self.unit.layout(&ty)?.size_bits {
                                        return Err(Error::new(
                                            declarator.span.start,
                                            "bitfield is wider than its type",
                                        ));
                                    }
                                }
                                fields.push(Field {
                                    name,
                                    ty,
                                    bit_width,
                                    alignment: extra.alignment.or(attributes.alignment),
                                    packed: extra.packed || attributes.packed,
                                });
                            }
                        }
                    }
                }
            }
            for (index, field) in fields.iter().enumerate() {
                if matches!(
                    self.unit.resolve(&field.ty)?.kind,
                    TypeKind::Array { length: None, .. }
                ) && (kind == RecordKind::Union || index + 1 != fields.len() || index == 0)
                {
                    return Err(Error::new(
                        declaration.span.start,
                        "flexible array must be the final member after a named member",
                    ));
                }
                if let TypeKind::Record(field_id) = self.unit.resolve(&field.ty)?.kind
                    && (field_id == id || self.unit.records[field_id].fields.is_none())
                {
                    return Err(Error::new(
                        declaration.span.start,
                        "field has incomplete record type",
                    ));
                }
            }
            self.unit.records[id].pack = self
                .packs
                .iter()
                .take_while(|(offset, _)| *offset <= declaration.span.start)
                .last()
                .and_then(|(_, pack)| *pack);
            self.unit.records[id].fields = Some(fields);
        }
        Ok(id)
    }

    fn enum_type(&mut self, declaration: &Node<ast::EnumType>) -> Result<usize, Error> {
        let name = declaration
            .node
            .identifier
            .as_ref()
            .map(|name| name.node.name.clone());
        let binding = name
            .as_ref()
            .and_then(|name| self.tags.get(name))
            .filter(|binding| {
                declaration.node.enumerators.is_empty()
                    || binding.depth == self.prototype_scopes.len()
            });
        let id = if let Some(binding) = binding {
            let Tag::Enum(id) = binding.tag else {
                return Err(Error::new(
                    declaration.span.start,
                    "tag used as both record and enum",
                ));
            };
            id
        } else {
            let id = self.unit.enums.len();
            self.unit.enums.push(Enum {
                name: name.clone(),
                scope: self.scope(),
                variants: Vec::new(),
            });
            if let Some(name) = name {
                self.bind_tag(name, Tag::Enum(id));
            }
            id
        };
        if !declaration.node.enumerators.is_empty() && !self.unit.enums[id].variants.is_empty() {
            return Err(Error::new(
                declaration.span.start,
                "enum is defined more than once",
            ));
        }
        let mut previous: Option<IntegerValue> = None;
        for enumerator in &declaration.node.enumerators {
            self.attributes(&enumerator.node.extensions, &mut Attributes::default())?;
            let value = if let Some(expression) = &enumerator.node.expression {
                self.eval(expression)?
            } else if let Some(previous) = previous {
                self.integer_add_one(previous, enumerator.span.start)?
            } else {
                IntegerValue::int(0)
            };
            // C11 enumerator identifiers have type int when their values fit,
            // irrespective of the suffix/type used in the defining expression.
            let value = if (value.signed && i32::try_from(value.signed_value()).is_ok())
                || (!value.signed && value.value <= i32::MAX as u128)
            {
                IntegerValue::int(value.signed_value())
            } else {
                value
            };
            let name = enumerator.node.identifier.node.name.clone();
            let duplicate = if let Some(scope) = self.prototype_scopes.last_mut() {
                scope.names.insert(name.clone(), None).is_some()
            } else {
                self.unit.constants.contains_key(&name)
                    || self.unit.typedefs.contains_key(&name)
                    || self
                        .unit
                        .declarations
                        .iter()
                        .any(|declaration| declaration.name == name)
            };
            if duplicate {
                return Err(Error::new(
                    enumerator.span.start,
                    format!("duplicate enumerator `{name}`"),
                ));
            }
            let previous_binding = self.unit.constants.insert(name.clone(), value);
            if let Some(scope) = self.prototype_scopes.last_mut() {
                scope.constants.push((name.clone(), previous_binding));
            }
            self.unit.enums[id]
                .variants
                .push(EnumVariant { name, value });
            previous = Some(value);
        }
        Ok(id)
    }

    fn apply_record_attributes(&mut self, ty: &Type, attributes: &Attributes) -> Result<(), Error> {
        if !attributes.packed && attributes.alignment.is_none() {
            return Ok(());
        }
        if let TypeKind::Record(id) = self.unit.resolve(ty)?.kind {
            let record = &mut self.unit.records[id];
            record.packed |= attributes.packed;
            if attributes.alignment.is_some() {
                record.alignment = attributes.alignment;
            }
        } else if attributes.packed || attributes.alignment.is_some() {
            return Err(Error::new(
                0,
                "layout attributes on a non-record type are unsupported",
            ));
        }
        Ok(())
    }

    fn attributes(
        &mut self,
        extensions: &[Node<ast::Extension>],
        result: &mut Attributes,
    ) -> Result<(), Error> {
        for extension in extensions {
            match &extension.node {
                ast::Extension::AsmLabel(label) => {
                    result.link_name = Some(decode_strings(&label.node, extension.span.start)?);
                }
                ast::Extension::AvailabilityAttribute(_) => {}
                ast::Extension::Attribute(attribute) => {
                    let name = attribute.name.node.trim_matches('_');
                    match name {
                        "mode" => {
                            let [argument] = attribute.arguments.as_slice() else {
                                return Err(Error::new(
                                    extension.span.start,
                                    "mode requires one machine mode name",
                                ));
                            };
                            let ast::Expression::Identifier(identifier) = &argument.node else {
                                return Err(Error::new(
                                    extension.span.start,
                                    "mode argument must be an identifier",
                                ));
                            };
                            result.mode = Some(identifier.node.name.trim_matches('_').to_owned());
                        }
                        "packed" => result.packed = true,
                        "aligned" => {
                            if attribute.arguments.len() != 1 {
                                return Err(Error::new(
                                    extension.span.start,
                                    "aligned attributes require an explicit alignment",
                                ));
                            }
                            let value = self.eval(&attribute.arguments[0])?.as_u64()?;
                            set_alignment(result, value, extension.span.start)?;
                        }
                        "cdecl" => {}
                        // These attributes do not alter C representation or calling convention.
                        "nothrow"
                        | "leaf"
                        | "nonnull"
                        | "format"
                        | "format_arg"
                        | "warn_unused_result"
                        | "malloc"
                        | "alloc_size"
                        | "alloc_align"
                        | "access"
                        | "deprecated"
                        | "pure"
                        | "const"
                        | "visibility"
                        | "sentinel"
                        | "always_inline"
                        | "gnu_inline"
                        | "noinline"
                        | "unused"
                        | "used"
                        | "artificial"
                        | "returns_nonnull"
                        | "cold"
                        | "hot"
                        | "noreturn"
                        | "may_alias"
                        | "noclone"
                        | "no_sanitize"
                        | "no_sanitize_address"
                        | "no_sanitize_thread"
                        | "no_sanitize_undefined"
                        | "fallthrough"
                        | "diagnose_if"
                        | "enable_if"
                        | "warn_unused"
                        | "externally_visible"
                        | "nonnull_all"
                        | "warn_if_not_aligned" => {}
                        _ => {
                            return Err(Error::new(
                                extension.span.start,
                                format!(
                                    "unsupported C attribute `{name}`; its ABI effect is not assumed"
                                ),
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn static_assert(&mut self, assertion: &Node<ast::StaticAssert>) -> Result<(), Error> {
        if !self.eval(&assertion.node.expression)?.truth() {
            return Err(Error::new(
                assertion.span.start,
                format!(
                    "static assertion failed: {}",
                    decode_strings(&assertion.node.message.node, assertion.span.start)?
                ),
            ));
        }
        Ok(())
    }

    fn machine_mode(&self, mut ty: Type, mode: &str, offset: usize) -> Result<Type, Error> {
        let resolved = self.unit.resolve(&ty)?;
        if !matches!(resolved.kind, TypeKind::Integer(_)) {
            return Err(Error::new(
                offset,
                "machine modes on noninteger types are unsupported",
            ));
        }
        let signed = self.integer_type(resolved, offset)?.signed;
        let width = match mode {
            "QI" | "byte" => 8,
            "HI" => 16,
            "SI" => 32,
            "DI" => 64,
            "TI" => 128,
            "word" | "pointer" => self.unit.target.pointer_width(),
            _ => {
                return Err(Error::new(
                    offset,
                    format!("unsupported integer machine mode `{mode}`"),
                ));
            }
        };
        ty.kind = TypeKind::Integer(match (width, signed) {
            (8, true) => IntegerKind::SignedChar,
            (8, false) => IntegerKind::UnsignedChar,
            (16, true) => IntegerKind::Short,
            (16, false) => IntegerKind::UnsignedShort,
            (32, true) => IntegerKind::Int,
            (32, false) => IntegerKind::UnsignedInt,
            (64, true) if self.unit.target.long_width() == 64 => IntegerKind::Long,
            (64, false) if self.unit.target.long_width() == 64 => IntegerKind::UnsignedLong,
            (64, true) => IntegerKind::LongLong,
            (64, false) => IntegerKind::UnsignedLongLong,
            (128, true) => IntegerKind::Int128,
            (128, false) => IntegerKind::UnsignedInt128,
            _ => return Err(Error::new(offset, "unsupported machine mode width")),
        });
        self.unit
            .layout(&ty)
            .map_err(|error| Error::new(offset, error.message))?;
        Ok(ty)
    }
}

fn extended_float_name(float: &ast::TS18661FloatType) -> String {
    let prefix = match float.format {
        ast::TS18661FloatFormat::BinaryInterchange | ast::TS18661FloatFormat::BinaryExtended => {
            "_Float"
        }
        _ => "_Decimal",
    };
    let suffix = if matches!(
        float.format,
        ast::TS18661FloatFormat::BinaryExtended | ast::TS18661FloatFormat::DecimalExtended
    ) {
        "x"
    } else {
        ""
    };
    format!("{prefix}{}{suffix}", float.width)
}

fn add_qualifier(
    result: &mut Qualifiers,
    qualifier: &Node<ast::TypeQualifier>,
) -> Result<(), Error> {
    match qualifier.node {
        ast::TypeQualifier::Const => result.is_const = true,
        ast::TypeQualifier::Volatile => result.is_volatile = true,
        ast::TypeQualifier::Restrict => result.is_restrict = true,
        ast::TypeQualifier::Atomic => {
            return Err(Error::new(
                qualifier.span.start,
                "atomic qualifiers require unsupported ABI handling",
            ));
        }
        ast::TypeQualifier::Nonnull
        | ast::TypeQualifier::NullUnspecified
        | ast::TypeQualifier::Nullable => {}
    }
    Ok(())
}

fn set_alignment(attributes: &mut Attributes, value: u64, offset: usize) -> Result<(), Error> {
    if value != 0 {
        if !value.is_power_of_two() || value > (1 << 28) {
            return Err(Error::new(
                offset,
                "alignment must be a supported power of two",
            ));
        }
        attributes.alignment = Some(attributes.alignment.unwrap_or(1).max(value));
    }
    Ok(())
}

pub(crate) fn decode_strings(strings: &[String], offset: usize) -> Result<String, Error> {
    let mut result = String::new();
    for string in strings {
        let Some(string) = string
            .strip_prefix('"')
            .and_then(|string| string.strip_suffix('"'))
        else {
            return Err(Error::new(offset, "wide string is unsupported here"));
        };
        let mut chars = string.chars();
        while let Some(ch) = chars.next() {
            if ch != '\\' {
                result.push(ch);
                continue;
            }
            result.push(match chars.next() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some('t') => '\t',
                Some('\\') => '\\',
                Some('"') => '"',
                Some('0') => '\0',
                _ => return Err(Error::new(offset, "unsupported string escape")),
            });
        }
    }
    Ok(result)
}

/// Removes line directives while preserving parser byte positions, and records
/// the effective pack value at each pragma rather than discarding ABI state.
fn prepare_source(source: &str) -> Result<(String, PackEvents), Error> {
    let mut result = String::with_capacity(source.len());
    let mut packs = Vec::new();
    let mut current = None;
    let mut stack = Vec::<(Option<String>, Option<u64>)>::new();
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if let Some(directive) = trimmed.strip_prefix('#') {
            let directive = directive.trim_start();
            if let Some(pragma) = directive.strip_prefix("pragma") {
                let pragma = pragma.trim();
                if let Some(pack) = pragma.strip_prefix("pack") {
                    let Some(arguments) = pack
                        .trim()
                        .strip_prefix('(')
                        .and_then(|value| value.strip_suffix(')'))
                    else {
                        return Err(Error::new(offset, "malformed pack pragma"));
                    };
                    let parts = arguments.split(',').map(str::trim).collect::<Vec<_>>();
                    let parse_value = |text: &str| -> Result<Option<u64>, Error> {
                        let value = text
                            .parse::<u64>()
                            .map_err(|_| Error::new(offset, "invalid pack alignment"))?;
                        if value == 0 {
                            Ok(None)
                        } else if matches!(value, 1 | 2 | 4 | 8 | 16) {
                            Ok(Some(value))
                        } else {
                            Err(Error::new(offset, "unsupported pack alignment"))
                        }
                    };
                    match parts[0] {
                        "" if parts.len() == 1 => current = None,
                        "push" => {
                            let mut label = None;
                            let mut value = None;
                            for part in &parts[1..] {
                                if part.chars().all(|ch| ch.is_ascii_digit()) {
                                    value = Some(parse_value(part)?);
                                } else if label.is_none() {
                                    label = Some((*part).to_owned());
                                } else {
                                    return Err(Error::new(offset, "invalid pack push"));
                                }
                            }
                            stack.push((label, current));
                            if let Some(value) = value {
                                current = value;
                            }
                        }
                        "pop" => {
                            let label = parts
                                .get(1)
                                .filter(|value| !value.chars().all(|ch| ch.is_ascii_digit()));
                            if let Some(label) = label {
                                let index = stack
                                    .iter()
                                    .rposition(|(name, _)| name.as_deref() == Some(*label))
                                    .ok_or_else(|| {
                                        Error::new(offset, "unknown pack stack label")
                                    })?;
                                current = stack[index].1;
                                stack.truncate(index);
                            } else {
                                current = stack
                                    .pop()
                                    .ok_or_else(|| {
                                        Error::new(offset, "pack pop without matching push")
                                    })?
                                    .1;
                            }
                            for part in &parts[1..] {
                                if part.chars().all(|ch| ch.is_ascii_digit()) {
                                    current = parse_value(part)?;
                                }
                            }
                        }
                        value if parts.len() == 1 => current = parse_value(value)?,
                        _ => return Err(Error::new(offset, "invalid pack pragma")),
                    }
                    packs.push((offset, current));
                } else if !(pragma.starts_with("GCC diagnostic")
                    || pragma.starts_with("clang diagnostic")
                    || pragma == "once"
                    || pragma.starts_with("GCC visibility"))
                {
                    return Err(Error::new(offset, format!("unsupported pragma `{pragma}`")));
                }
            } else if !(directive.starts_with("line ")
                || directive
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_digit())
                || directive.is_empty())
            {
                return Err(Error::new(
                    offset,
                    "input contains an unprocessed directive",
                ));
            }
            for byte in line.bytes() {
                result.push(if byte == b'\n' { '\n' } else { ' ' });
            }
        } else {
            result.push_str(line);
        }
        offset += line.len();
    }
    Ok((result, packs))
}

/// Bounds recursive parser work before constructing the external parser's AST.
/// This scanner treats quoted strings and comments as indivisible tokens.
fn check_parse_limits(source: &str) -> Result<(), Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let bytes = source.as_bytes();
    let mut index = 0;
    let mut nesting = 0usize;
    let mut operators = 0usize;
    let mut prefix_run = 0usize;
    while index < bytes.len() {
        if matches!(bytes[index], b'+' | b'-' | b'!' | b'~' | b'*' | b'&') {
            prefix_run += 1;
            if prefix_run > 16 {
                return Err(Error::new(
                    index,
                    "consecutive prefix operators exceed the 16-operator limit",
                ));
            }
        } else if !bytes[index].is_ascii_whitespace() {
            prefix_run = 0;
        }
        match bytes[index] {
            b'\'' | b'"' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index += 1;
            }
            b'(' | b'[' | b'{' => {
                nesting += 1;
                if nesting > 128 {
                    return Err(Error::new(
                        index,
                        "syntactic nesting exceeds the 128-level limit",
                    ));
                }
            }
            b')' | b']' | b'}' => {
                nesting = nesting.saturating_sub(1);
            }
            b';' => operators = 0,
            b'*' | b'!' | b'~' | b'+' | b'-' | b'/' | b'%' | b'&' | b'|' | b'^' | b'?' | b'<'
            | b'>' | b'=' => {
                operators += 1;
                if operators > 256 {
                    return Err(Error::new(
                        index,
                        "expression or declarator exceeds the operator limit",
                    ));
                }
            }
            _ => {}
        }
        index += 1;
    }
    Ok(())
}

/// Adapts legal GNU attribute spelling to lang-c's grammar. It requires adjacent
/// double parentheses and attributes before the `struct` keyword. Replacements
/// preserve total byte length, so diagnostics outside an attribute remain stable.
fn normalize_attributes(source: &str) -> (String, HashSet<usize>) {
    let mut record_attributes = HashSet::new();
    let mut bytes = source.as_bytes().to_vec();
    let mut index = 0;
    let mut previous_word: Option<(usize, usize)> = None;
    while index < bytes.len() {
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            index += 2;
            while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/') {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if matches!(bytes[index], b'\'' | b'"') {
            let quote = bytes[index];
            index += 1;
            while index < bytes.len() && bytes[index] != quote {
                if bytes[index] == b'\\' {
                    index += 1;
                }
                index += 1;
            }
            index += 1;
            previous_word = None;
            continue;
        }
        if bytes[index].is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if !(bytes[index].is_ascii_alphabetic() || bytes[index] == b'_') {
            index += 1;
            previous_word = None;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        if &bytes[start..index] != b"__attribute__" && &bytes[start..index] != b"__attribute" {
            previous_word = Some((start, index));
            continue;
        }
        let mut first = index;
        while bytes.get(first).is_some_and(u8::is_ascii_whitespace) {
            first += 1;
        }
        if bytes.get(first) != Some(&b'(') {
            previous_word = None;
            continue;
        }
        let mut second = first + 1;
        while bytes.get(second).is_some_and(u8::is_ascii_whitespace) {
            second += 1;
        }
        if bytes.get(second) != Some(&b'(') {
            previous_word = None;
            continue;
        }
        let mut cursor = second + 1;
        let mut depth = 2;
        let mut penultimate = second;
        while cursor < bytes.len() && depth > 0 {
            match bytes[cursor] {
                b'\'' | b'"' => {
                    let quote = bytes[cursor];
                    cursor += 1;
                    while cursor < bytes.len() && bytes[cursor] != quote {
                        if bytes[cursor] == b'\\' {
                            cursor += 1;
                        }
                        cursor += 1;
                    }
                }
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 1 {
                        penultimate = cursor;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        if depth != 0 {
            previous_word = None;
            continue;
        }
        let last = cursor - 1;
        if bytes[penultimate + 1..last]
            .iter()
            .all(u8::is_ascii_whitespace)
        {
            bytes[first..=second].fill(b' ');
            bytes[first] = b'(';
            bytes[first + 1] = b'(';
            bytes[penultimate..=last].fill(b' ');
            bytes[penultimate] = b')';
            bytes[penultimate + 1] = b')';
        }
        if let Some((keyword_start, keyword_end)) = previous_word
            && matches!(
                &bytes[keyword_start..keyword_end],
                b"struct" | b"union" | b"enum"
            )
            && bytes[keyword_end..start]
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            record_attributes.insert(keyword_start);
            let keyword = bytes[keyword_start..keyword_end].to_vec();
            let attribute = bytes[start..cursor].to_vec();
            let separator = start - keyword_end;
            bytes[keyword_start..keyword_start + attribute.len()].copy_from_slice(&attribute);
            let keyword_position = keyword_start + attribute.len() + separator;
            bytes[keyword_start + attribute.len()..keyword_position].fill(b' ');
            bytes[keyword_position..cursor].copy_from_slice(&keyword);
        }
        index = cursor;
        previous_word = None;
    }
    // Reordering and replacing ASCII bytes leaves every multibyte character intact.
    (
        String::from_utf8(bytes).expect("attribute normalization preserves UTF-8"),
        record_attributes,
    )
}
