use std::collections::{BTreeMap, HashMap, HashSet};

use lang_c::{ast, driver, span::Node};
use toucan_target::Target;

use crate::checked::{
    Builder as CodeBuilder, CheckedCode, EntityKind, Limits as CodeLimits, LocalDeclaration,
    OccurrenceKind, ScopeKind, Storage, declarator_name_span,
};

use crate::{
    CallingConvention, Declaration, DeclarationKind, Enum, EnumVariant, Error, Field, FloatKind,
    FunctionType, IntegerKind, IntegerValue, Parameter, Qualifiers, Record, RecordKind, Scope,
    TranslationUnit, Type, TypeKind,
};

type PackEvents = Vec<(usize, Option<u64>)>;

/// Parses and checks preprocessed C declarations and bodies without a subprocess.
///
/// Pack pragmas are interpreted before parsing. Definition markers let binding
/// generators omit inline functions after their bodies have been checked.
pub fn analyze(source: &str, target: Target) -> Result<TranslationUnit, Error> {
    analyze_inner(source, target, None).map(|(unit, _)| unit)
}

pub(crate) fn analyze_inner(
    source: &str,
    target: Target,
    retention: Option<CodeLimits>,
) -> Result<(TranslationUnit, Option<CheckedCode>), Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let (source, packs) = prepare_source(source)?;
    let parsed = parse(&source, 0)?;
    let packs = packs
        .into_iter()
        .map(|(offset, pack)| (parsed.offsets.pragma_offset(offset), pack))
        .collect();
    let mut analyzer = Analyzer::new(target, packs);
    analyzer.record_attributes = parsed.record_attributes;
    analyzer.character_literals = parsed.character_literals;
    analyzer.string_literals = parsed.string_literals;
    analyzer.empty_initializers = parsed.empty_initializers;
    analyzer.int128_specifiers = parsed.int128_specifiers;
    let result = (|| {
        if let Some(limits) = retention {
            analyzer.checked = Some(Box::new(CodeBuilder::new(
                &parsed.unit,
                source.len(),
                limits,
            )?));
        }
        for external in parsed.unit.0 {
            match external.node {
                ast::ExternalDeclaration::Declaration(declaration) => {
                    analyzer.declaration(&declaration, false)?
                }
                ast::ExternalDeclaration::StaticAssert(assertion) => {
                    analyzer.static_assert(&assertion)?
                }
                ast::ExternalDeclaration::FunctionDefinition(definition) => {
                    analyzer.function_definition(&definition)?;
                }
            }
        }
        analyzer.finish_tentative_definitions()?;
        analyzer.validate_block_externs()?;
        let checked = analyzer
            .checked
            .take()
            .map(|builder| builder.finish(&parsed.offsets))
            .transpose()?;
        Ok((analyzer.unit, checked))
    })();
    result.map_err(|mut error: Error| {
        error.offset = parsed.offsets.original_offset(error.offset);
        error
    })
}

/// Evaluates an integer constant expression in the translation unit's type and
/// enumerator environment, using the target's C integer conversion rules.
pub fn evaluate_integer(unit: &TranslationUnit, expression: &str) -> Result<IntegerValue, Error> {
    evaluate_expression(unit, expression, |analyzer, expression| {
        analyzer.eval(expression)
    })
}

/// Evaluate a supported arithmetic constant in the unit's type and enumerator
/// environment. Floating operations round to nearest, ties to even in the target
/// format. This is a separate query from C integer-constant-expression checking.
pub fn evaluate_arithmetic(
    unit: &TranslationUnit,
    expression: &str,
) -> Result<crate::ArithmeticConstant, Error> {
    evaluate_expression(unit, expression, |analyzer, expression| {
        analyzer
            .eval_arithmetic(expression)?
            .into_constant(analyzer.unit.target, expression.span.start)
    })
}

fn evaluate_expression<Value>(
    unit: &TranslationUnit,
    expression: &str,
    evaluate: impl FnOnce(&mut Analyzer, &Node<ast::Expression>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let identifiers = validate_expression_source(expression)?;
    for value in unit.constants.values() {
        value.validate()?;
    }
    // Only typedef names occurring in the expression matter to the parser. The
    // semantic environment below retains every real type, including dependencies
    // of these typedefs, without reparsing unrelated names for every macro.
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
            if !identifiers.contains(name.as_str()) {
                continue;
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
    let parsed = parse(&source, expression_offset).map_err(|mut error| {
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
    analyzer.character_literals = parsed.character_literals;
    analyzer.string_literals = parsed.string_literals;
    analyzer.empty_initializers = parsed.empty_initializers;
    analyzer.int128_specifiers = parsed.int128_specifiers;
    evaluate(&mut analyzer, expression).map_err(|mut error| {
        error.offset = parsed
            .offsets
            .original_offset(error.offset)
            .saturating_sub(expression_offset);
        error
    })
}

struct Parsed {
    unit: ast::TranslationUnit,
    record_attributes: HashSet<usize>,
    character_literals: HashMap<usize, String>,
    string_literals: HashMap<usize, Vec<String>>,
    offsets: crate::parser_extensions::SourceMap,
    empty_initializers: HashSet<usize>,
    int128_specifiers: HashSet<usize>,
}

fn parse(source: &str, diagnostic_offset: usize) -> Result<Parsed, Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let original = source;
    let source = strip_comments(source)?;
    check_parse_limits(&source)?;
    let adapted = crate::parser_extensions::adapt(&source)?;
    let source = adapted.source;
    let config = driver::Config {
        cpp_command: String::new(),
        cpp_options: Vec::new(),
        flavor: driver::Flavor::ClangC11,
    };
    let (source, record_attributes) = normalize_attributes(&source);
    let (source, literal_spellings) = crate::literals::normalize_literal_escapes(source);
    let parsed = driver::parse_preprocessed(&config, source).map_err(|mut error| {
        error.offset = adapted.offsets.original_offset(error.offset);
        error.source = original.to_owned();
        error.line = original[..error.offset]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        let line_start = original[..error.offset]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        error.column = original[line_start..error.offset].chars().count() + 1;
        let offset = error.offset;
        if diagnostic_offset != 0 && offset >= diagnostic_offset {
            // Macro parse wrappers must not leak into the displayed line/column.
            // Keep the AST offset until the caller adjusts it alongside semantic
            // errors, but format the parser's message relative to the expression.
            error.source.drain(..diagnostic_offset);
            error.offset -= diagnostic_offset;
            let line_start = error.source[..error.offset]
                .rfind('\n')
                .map_or(0, |newline| newline + 1);
            error.column = error.source[line_start..error.offset].chars().count() + 1;
        }
        Error::new(offset, format!("C syntax error: {error}"))
    })?;
    Ok(Parsed {
        unit: parsed.unit,
        record_attributes,
        character_literals: literal_spellings.characters,
        string_literals: literal_spellings.strings,
        offsets: adapted.offsets,
        empty_initializers: adapted.empty_initializers,
        int128_specifiers: adapted.int128_specifiers,
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
/// independently of the parser, including comments and quoted literals. Returns
/// identifier tokens so the parser can recognize referenced typedef names.
fn validate_expression_source(expression: &str) -> Result<HashSet<&str>, Error> {
    let bytes = expression.as_bytes();
    let mut index = 0;
    let mut delimiters = Vec::new();
    let mut identifiers = HashSet::new();
    while index < bytes.len() {
        match bytes[index] {
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = index;
                while bytes
                    .get(index + 1)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                {
                    index += 1;
                }
                let identifier = &expression[start..=index];
                // A literal's encoding prefix is not an identifier token.
                if !(matches!(identifier, "L" | "u" | "U" | "u8")
                    && matches!(bytes.get(index + 1), Some(b'\'' | b'"')))
                {
                    identifiers.insert(identifier);
                }
            }
            b'0'..=b'9' => {
                // Consume preprocessing numbers together, including suffixes and
                // exponent signs, rather than treating their letters as names.
                while bytes.get(index + 1).is_some_and(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b'_' | b'.')
                        || (matches!(byte, b'+' | b'-')
                            && matches!(bytes[index], b'e' | b'E' | b'p' | b'P'))
                }) {
                    index += 1;
                }
            }
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
            b'(' | b'[' | b'{' => delimiters.push(bytes[index]),
            b')' | b']' | b'}' => {
                let expected = match bytes[index] {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if delimiters.pop() != Some(expected) {
                    return Err(Error::new(
                        index,
                        "unbalanced delimiter in integer expression",
                    ));
                }
            }
            b';' | b'#' => {
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
    Ok(identifiers)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Attributes {
    packed: bool,
    alignment: Option<u64>,
    link_name: Option<String>,
    mode: Option<String>,
    calling_convention: Option<CallingConvention>,
    alias_base: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum Tag {
    Record(usize),
    Enum(usize),
}

#[derive(Clone, Copy)]
pub(crate) struct TagBinding {
    pub(crate) tag: Tag,
    pub(crate) depth: usize,
}

/// Scope frames retain only new bindings; file-scope maps remain shared.
#[derive(Default)]
pub(crate) struct LexicalScope {
    pub(crate) is_block: bool,
    pub(crate) is_definition_parameters: bool,
    pub(crate) variably_modified: Option<usize>,
    pub(crate) record_ids: Vec<usize>,
    pub(crate) enum_ids: Vec<usize>,
    pub(crate) typedefs: HashMap<String, Type>,
    pub(crate) static_storage: HashSet<String>,
    pub(crate) flexible_array_storage: HashMap<String, crate::FlexibleArrayStorage>,
    pub(crate) linked: HashSet<String>,
    pub(crate) register: HashSet<String>,
    pub(crate) tags: Vec<(String, Option<TagBinding>)>,
    pub(crate) constants: Vec<(String, Option<IntegerValue>)>,
    /// A parameter index, or None for an enumerator in the ordinary namespace.
    pub(crate) names: HashMap<String, Option<usize>>,
    pub(crate) parameters: Vec<Parameter>,
}

#[derive(Default)]
struct StorageSpecifiers {
    class: Option<ast::StorageClassSpecifier>,
    thread_local: bool,
}

/// C11 permits one storage class, with `_Thread_local` additionally allowed
/// beside `static` or `extern`.
fn storage_specifiers(
    specifiers: &[Node<ast::DeclarationSpecifier>],
) -> Result<StorageSpecifiers, Error> {
    let mut storage = StorageSpecifiers::default();
    for specifier in specifiers {
        let ast::DeclarationSpecifier::StorageClass(class) = &specifier.node else {
            continue;
        };
        if class.node == ast::StorageClassSpecifier::ThreadLocal {
            if storage.thread_local {
                return Err(Error::new(
                    class.span.start,
                    "duplicate thread-local storage specifier",
                ));
            }
            storage.thread_local = true;
        } else if storage.class.replace(class.node.clone()).is_some() {
            return Err(Error::new(
                class.span.start,
                "multiple storage-class specifiers",
            ));
        }
    }
    if storage.thread_local
        && !matches!(
            storage.class,
            None | Some(ast::StorageClassSpecifier::Static | ast::StorageClassSpecifier::Extern)
        )
    {
        return Err(Error::new(
            specifiers[0].span.start,
            "thread-local storage can only combine with static or extern",
        ));
    }
    Ok(storage)
}

/// Finds the derivation applied last, including parenthesized declarators.
fn outermost_derived(
    mut declarator: &Node<ast::Declarator>,
) -> Option<&Node<ast::DerivedDeclarator>> {
    let mut outermost = None;
    loop {
        let derived = &declarator.node.derived;
        if !derived.is_empty() {
            outermost = derived
                .iter()
                .find(|derived| !matches!(derived.node, ast::DerivedDeclarator::Pointer(_)))
                .or_else(|| derived.last());
        }
        if let ast::DeclaratorKind::Declarator(inner) = &declarator.node.kind.node {
            declarator = inner;
        } else {
            return outermost;
        }
    }
}

pub(crate) struct Analyzer {
    pub(crate) checked: Option<Box<CodeBuilder>>,
    pub(crate) unit: TranslationUnit,
    pub(crate) tags: HashMap<String, TagBinding>,
    pub(crate) lexical_scopes: Vec<LexicalScope>,
    defining_enums: HashSet<usize>,
    tentative_definitions: BTreeMap<usize, usize>,
    packs: PackEvents,
    record_attributes: HashSet<usize>,
    pub(crate) character_literals: HashMap<usize, String>,
    pub(crate) string_literals: HashMap<usize, Vec<String>>,
    pub(crate) empty_initializers: HashSet<usize>,
    int128_specifiers: HashSet<usize>,
    nesting: usize,
    pub(crate) capture_function_scope: bool,
    definition_parameters: Option<usize>,
    pub(crate) variably_modified_parents: Vec<Option<usize>>,
    pub(crate) function_scope: Option<crate::statement::FunctionScope>,
    pub(crate) current_function: Option<crate::statement::FunctionContext>,
    pub(crate) block_externs: HashMap<String, Type>,
    type_names: HashMap<(usize, usize), Type>,
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

    pub(crate) fn from_unit(unit: TranslationUnit) -> Self {
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
            checked: None,
            unit,
            tags,
            lexical_scopes: Vec::new(),
            defining_enums: HashSet::new(),
            tentative_definitions: BTreeMap::new(),
            packs: Vec::new(),
            record_attributes: HashSet::new(),
            character_literals: HashMap::new(),
            string_literals: HashMap::new(),
            empty_initializers: HashSet::new(),
            int128_specifiers: HashSet::new(),
            nesting: 0,
            capture_function_scope: false,
            definition_parameters: None,
            variably_modified_parents: Vec::new(),
            function_scope: None,
            current_function: None,
            block_externs: HashMap::new(),
            type_names: HashMap::new(),
        }
    }

    fn scope(&self) -> Scope {
        match self.lexical_scopes.last() {
            Some(scope) if scope.is_block => Scope::Block,
            Some(_) => Scope::Prototype,
            None => Scope::File,
        }
    }

    pub(crate) fn bind_tag(&mut self, name: String, tag: Tag) {
        let previous = self.tags.insert(
            name.clone(),
            TagBinding {
                tag,
                depth: self.lexical_scopes.len(),
            },
        );
        if let Some(scope) = self.lexical_scopes.last_mut() {
            scope.tags.push((name, previous));
        }
    }

    pub(crate) fn parameter_type(&self, name: &str) -> Option<&Type> {
        self.lexical_scopes
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

    pub(crate) fn leave_prototype(&mut self) -> Vec<Parameter> {
        let scope = self
            .lexical_scopes
            .pop()
            .expect("prototype scope is active");
        let checked_scope = self.checked.as_mut().map(|checked| checked.leave_scope());
        if self.capture_function_scope && !scope.is_block {
            self.function_scope = Some(crate::statement::FunctionScope {
                checked_scope,
                record_ids: scope.record_ids.clone(),
                enum_ids: scope.enum_ids.clone(),
                tags: scope
                    .tags
                    .iter()
                    .filter_map(|(name, _)| {
                        self.tags
                            .get(name)
                            .map(|binding| (name.clone(), binding.tag))
                    })
                    .collect(),
                constants: scope
                    .constants
                    .iter()
                    .filter_map(|(name, _)| {
                        self.unit
                            .constants
                            .get(name)
                            .map(|value| (name.clone(), *value))
                    })
                    .collect(),
                parameters: scope.parameters.clone(),
                register: scope.register.clone(),
            });
        }
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

    pub(crate) fn declaration(
        &mut self,
        declaration: &Node<ast::Declaration>,
        definition: bool,
    ) -> Result<(), Error> {
        let storage = storage_specifiers(&declaration.node.specifiers)?;
        if matches!(
            storage.class,
            Some(ast::StorageClassSpecifier::Auto | ast::StorageClassSpecifier::Register)
        ) {
            return Err(Error::new(
                declaration.span.start,
                "auto and register are not permitted at file scope",
            ));
        }
        let is_typedef = storage.class == Some(ast::StorageClassSpecifier::Typedef);
        // glibc defines the TS spellings as typedefs for older compiler profiles.
        // lang-c recognizes their spelling as a type specifier even in this
        // declaration position, so recover the explicit typedef name here.
        if declaration.node.declarators.is_empty()
            && is_typedef
            && let Some(Node {
                node:
                    ast::DeclarationSpecifier::TypeSpecifier(Node {
                        node: ast::TypeSpecifier::TS18661Float(float),
                        span,
                    }),
                ..
            }) = declaration.node.specifiers.last()
            && !self.int128_specifiers.contains(&span.start)
        {
            let name = extended_float_name(float);
            let (ty, attributes) = self.specifiers(
                &declaration.node.specifiers[..declaration.node.specifiers.len() - 1],
            )?;
            if attributes.packed || attributes.alignment.is_some() || attributes.mode.is_some() {
                return Err(Error::new(
                    declaration.span.start,
                    "attributes on extended float compatibility typedefs are unsupported",
                ));
            }
            if self
                .unit
                .typedefs
                .insert(name.clone(), ty.clone())
                .is_some()
            {
                return Err(Error::new(
                    declaration.span.start,
                    "duplicate extended float typedef",
                ));
            }
            self.unit.declarations.push(Declaration {
                name,
                ty,
                kind: DeclarationKind::Typedef,
                link_name: None,
                is_static: false,
                is_definition: false,
                flexible_array_storage: None,
            });
            if let Some(checked) = &mut self.checked {
                let index = self.unit.declarations.len() - 1;
                checked.file_declaration(
                    declaration,
                    OccurrenceKind::Declaration,
                    &self.unit.declarations[index],
                    index,
                    false,
                    declaration
                        .node
                        .specifiers
                        .last()
                        .map(|specifier| specifier.span),
                )?;
            }
            return Ok(());
        }
        let (base, attributes) = self.specifiers(&declaration.node.specifiers)?;
        for item in &declaration.node.declarators {
            let mut is_static = storage.class == Some(ast::StorageClassSpecifier::Static);
            let previous_parameters = self.definition_parameters;
            if definition {
                self.definition_parameters = outermost_derived(&item.node.declarator)
                    .filter(|derived| matches!(derived.node, ast::DerivedDeclarator::Function(_)))
                    .map(|derived| derived.span.start);
            }
            let declarator = self.declarator(base.clone(), &item.node.declarator, &attributes);
            self.definition_parameters = previous_parameters;
            let (name, mut ty, mut declarator_attributes) = declarator?;
            if self.unit.is_variably_modified(&ty)? {
                return Err(Error::new(
                    item.span.start,
                    "variably modified identifiers require block or prototype scope",
                ));
            }
            let name =
                name.ok_or_else(|| Error::new(item.span.start, "declaration has no name"))?;
            if self.unit.constants.contains_key(&name) {
                return Err(Error::new(
                    item.span.start,
                    format!("declaration conflicts with enumerator `{name}`"),
                ));
            }
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
                    if !self.same_type(previous, &ty, 0)? {
                        return Err(Error::new(
                            item.span.start,
                            format!("conflicting typedef `{name}`"),
                        ));
                    }
                    ty = self.composite_type(previous, &ty, 0)?;
                    self.unit.typedefs.insert(name.clone(), ty.clone());
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
            if storage.thread_local {
                return Err(Error::new(
                    item.span.start,
                    if kind == DeclarationKind::Variable {
                        "thread-local objects require unsupported Rust TLS bindings"
                    } else {
                        "thread-local storage requires an object declaration"
                    },
                ));
            }
            if item.node.initializer.is_some() && kind != DeclarationKind::Variable {
                return Err(Error::new(
                    item.span.start,
                    "only objects can have initializers",
                ));
            }
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
            let is_definition = definition || item.node.initializer.is_some();
            let declaration_index = if let Some(previous_index) = self
                .unit
                .declarations
                .iter()
                .position(|previous| previous.name == name)
            {
                let previous = &self.unit.declarations[previous_index];
                if kind == DeclarationKind::Function {
                    ty = self.inherit_calling_convention(ty, &previous.ty)?;
                }
                if previous.kind != kind || !self.compatible(&previous.ty, &ty)? {
                    return Err(Error::new(
                        item.span.start,
                        format!("conflicting declaration of `{name}`"),
                    ));
                }
                if kind != DeclarationKind::Typedef {
                    // Extern declarations inherit visible linkage. A function
                    // declaration without storage behaves as an extern declaration.
                    if storage.class == Some(ast::StorageClassSpecifier::Extern)
                        || (storage.class.is_none() && kind == DeclarationKind::Function)
                    {
                        is_static = previous.is_static;
                    }
                    if previous.is_static != is_static {
                        return Err(Error::new(
                            item.span.start,
                            format!("conflicting linkage for `{name}`"),
                        ));
                    }
                    if is_definition && previous.is_definition {
                        return Err(Error::new(
                            item.span.start,
                            format!("multiple definitions of `{name}`"),
                        ));
                    }
                }
                // A composite type retains all available bounds and prototypes,
                // including those nested inside pointers and function parameters.
                ty = if definition {
                    // Definition parameter names belong to its body; names in an
                    // earlier prototype have no bearing on those declarations.
                    self.composite_type(&ty, &previous.ty, 0)?
                } else {
                    self.composite_type(&previous.ty, &ty, 0)?
                };
                let previous = &mut self.unit.declarations[previous_index];
                previous.ty = ty;
                previous.is_definition |= is_definition;
                if previous.link_name.is_none() {
                    previous.link_name = declarator_attributes.link_name;
                }
                previous_index
            } else {
                let index = self.unit.declarations.len();
                self.unit.declarations.push(Declaration {
                    name,
                    ty,
                    kind,
                    link_name: declarator_attributes.link_name,
                    is_static,
                    is_definition,
                    flexible_array_storage: None,
                });
                index
            };
            let checked_site = if let Some(checked) = &mut self.checked {
                checked.file_declaration(
                    item,
                    OccurrenceKind::InitDeclarator,
                    &self.unit.declarations[declaration_index],
                    declaration_index,
                    is_definition,
                    declarator_name_span(&item.node.declarator),
                )?
            } else {
                None
            };
            if kind == DeclarationKind::Variable
                && !is_definition
                && storage.class != Some(ast::StorageClassSpecifier::Extern)
            {
                self.tentative_definitions
                    .entry(declaration_index)
                    .or_insert(item.span.start);
            }
            if let Some(initializer) = &item.node.initializer {
                // Earlier declarations contribute bounds to the object being
                // initialized, including when this declarator omits its bound.
                let initializer_type = self.unit.declarations[declaration_index].ty.clone();
                self.initialize_declaration(declaration_index, &initializer_type, initializer)?;
            }
            if let (Some(checked), Some(site)) = (&mut self.checked, checked_site) {
                let declaration = &self.unit.declarations[declaration_index];
                checked.complete_declaration(
                    site,
                    &declaration.ty,
                    declaration.flexible_array_storage.as_ref(),
                )?;
                if let Some(initializer) = &item.node.initializer {
                    checked.attach_initializer(site, initializer)?;
                }
            }
        }
        Ok(())
    }

    /// C11 6.9.2 completes tentative definitions after every declaration is known.
    fn finish_tentative_definitions(&mut self) -> Result<(), Error> {
        for (&index, &offset) in &self.tentative_definitions {
            let declaration = &self.unit.declarations[index];
            if declaration.is_definition {
                continue;
            }
            let mut ty = declaration.ty.clone();
            if let TypeKind::Array {
                element,
                length: None,
            } = &self.unit.resolve(&ty)?.kind
            {
                ty = Type {
                    kind: TypeKind::Array {
                        element: element.clone(),
                        length: Some(1),
                    },
                    qualifiers: self.unit.qualifiers(&ty)?,
                };
            }
            if !self.is_complete_object(&ty, 0)? {
                return Err(Error::new(
                    offset,
                    format!(
                        "tentative definition of `{}` has incomplete type",
                        declaration.name
                    ),
                ));
            }
            self.unit.declarations[index].ty = ty;
            self.unit.declarations[index].is_definition = true;
        }
        Ok(())
    }

    pub(crate) fn compatible(&self, left: &Type, right: &Type) -> Result<bool, Error> {
        self.compatible_at(left, right, 0)
    }

    /// Typedef redeclarations require the same type, rather than a compatible
    /// incomplete/complete pair. Parameter names and equivalent ABI spellings
    /// do not create distinct function types.
    pub(crate) fn same_type(&self, left: &Type, right: &Type, depth: usize) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "type identity nesting exceeds the 128-level limit",
            ));
        }
        if self.unit.qualifiers(left)? != self.unit.qualifiers(right)? {
            return Ok(false);
        }
        let left = self.unit.resolve(left)?;
        let right = self.unit.resolve(right)?;
        Ok(match (&left.kind, &right.kind) {
            (TypeKind::Pointer(a), TypeKind::Pointer(b)) => self.same_type(a, b, depth + 1)?,
            (
                TypeKind::Array {
                    element: a,
                    length: al,
                },
                TypeKind::Array {
                    element: b,
                    length: bl,
                },
            ) => al == bl && self.same_type(a, b, depth + 1)?,
            (TypeKind::VariableArray { element: a }, TypeKind::VariableArray { element: b }) => {
                self.same_type(a, b, depth + 1)?
            }
            (TypeKind::Function(a), TypeKind::Function(b)) => {
                if a.prototype != b.prototype
                    || a.variadic != b.variadic
                    || a.parameters.len() != b.parameters.len()
                    || a.calling_convention.for_target(self.unit.target)?
                        != b.calling_convention.for_target(self.unit.target)?
                    || !self.same_type(&a.return_type, &b.return_type, depth + 1)?
                {
                    return Ok(false);
                }
                for (a, b) in a.parameters.iter().zip(&b.parameters) {
                    let mut a = self.unit.resolve(&a.ty)?.clone();
                    let mut b = self.unit.resolve(&b.ty)?.clone();
                    a.qualifiers = Qualifiers::default();
                    b.qualifiers = Qualifiers::default();
                    if !self.same_type(&a, &b, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => left.kind == right.kind,
        })
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
            (TypeKind::Enum(_), TypeKind::Integer(_))
            | (TypeKind::Integer(_), TypeKind::Enum(_)) => {
                // C11 6.7.2.2 makes an enum compatible with its selected integer
                // type, including inside pointers and function declarations.
                // Distinct enum tags remain distinct types.
                Ok(self.integer_type(left, 0)? == self.integer_type(right, 0)?)
            }
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
            (
                TypeKind::VariableArray { element: left },
                TypeKind::VariableArray { element: right },
            )
            | (TypeKind::VariableArray { element: left }, TypeKind::Array { element: right, .. })
            | (TypeKind::Array { element: left, .. }, TypeKind::VariableArray { element: right }) => {
                self.compatible_at(left, right, depth + 1)
            }
            (TypeKind::Function(left), TypeKind::Function(right)) => {
                if left.calling_convention.for_target(self.unit.target)?
                    != right.calling_convention.for_target(self.unit.target)?
                {
                    return Ok(false);
                }
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
    pub(crate) fn composite_type(
        &self,
        left: &Type,
        right: &Type,
        depth: usize,
    ) -> Result<Type, Error> {
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
            (
                TypeKind::Array {
                    element: a,
                    length: Some(length),
                },
                TypeKind::VariableArray { element: b },
            )
            | (
                TypeKind::VariableArray { element: a },
                TypeKind::Array {
                    element: b,
                    length: Some(length),
                },
            ) => TypeKind::Array {
                element: Box::new(self.composite_type(a, b, depth + 1)?),
                length: Some(*length),
            },
            (TypeKind::VariableArray { element: a }, TypeKind::VariableArray { element: b })
            | (
                TypeKind::VariableArray { element: a },
                TypeKind::Array {
                    element: b,
                    length: None,
                },
            )
            | (
                TypeKind::Array {
                    element: a,
                    length: None,
                },
                TypeKind::VariableArray { element: b },
            ) => TypeKind::VariableArray {
                element: Box::new(self.composite_type(a, b, depth + 1)?),
            },
            (TypeKind::Function(a), TypeKind::Function(b)) => {
                let mut function = if a.prototype {
                    (**a).clone()
                } else {
                    (**b).clone()
                };
                if function.calling_convention == CallingConvention::C {
                    function.calling_convention = if a.calling_convention != CallingConvention::C {
                        a.calling_convention
                    } else {
                        b.calling_convention
                    };
                }
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

    pub(crate) fn specifiers(
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
        if let Some(checked) = &mut self.checked {
            for specifier in &types {
                if let ast::TypeSpecifier::TypedefName(name) = &specifier.node {
                    checked.typedef_reference(name)?;
                }
            }
        }
        attributes.alias_base = matches!(
            types.as_slice(),
            [Node {
                node: ast::TypeSpecifier::TypedefName(_),
                ..
            }]
        ) && matches!(
            self.unit.resolve(&ty)?.kind,
            TypeKind::Pointer(_) | TypeKind::Array { .. }
        );
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
        self.check_restrict(&ty, specifiers.first().map_or(0, |item| item.span.start))?;
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
        if let [
            Node {
                node: ast::TypeSpecifier::TypedefName(name),
                ..
            },
        ] = types
            && let Some(ty) = self.local_typedef(&name.node.name)
        {
            return Ok(ty.clone());
        }
        let offset = types.first().map_or(0, |ty| ty.span.start);
        let mut long = 0;
        let mut short = false;
        let mut signed = false;
        let mut unsigned = false;
        let mut char_ = false;
        let mut float = false;
        let mut double = false;
        let mut int = false;
        let mut int128 = false;
        let mut special = None;
        for ty in types {
            if self.int128_specifiers.contains(&ty.span.start) {
                if std::mem::replace(&mut int128, true) {
                    return Err(Error::new(
                        ty.span.start,
                        "duplicate __int128 type specifier",
                    ));
                }
                continue;
            }
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
            if long > 0 || short || signed || unsigned || char_ || float || double || int || int128
            {
                return Err(Error::new(offset, "invalid modifiers on type"));
            }
            return Ok(Type::new(special));
        }
        if types.is_empty()
            || long > 2
            || (long > 0 && short)
            || (signed && unsigned)
            || (int128 && (long > 0 || short || char_ || float || double || int))
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
            TypeKind::Integer(if int128 {
                if unsigned {
                    IntegerKind::UnsignedInt128
                } else {
                    IntegerKind::Int128
                }
            } else if char_ {
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

    /// Resolves each written type name once, so typing an unevaluated operand and
    /// subsequently evaluating it cannot redeclare tags defined inside that type.
    pub(crate) fn type_name(&mut self, name: &ast::TypeName) -> Result<Type, Error> {
        let start = name.specifiers.first().map_or(0, |node| node.span.start);
        let end = name.declarator.as_ref().map_or_else(
            || name.specifiers.last().map_or(start, |node| node.span.end),
            |node| node.span.end,
        );
        let key = (start, end);
        if let Some(ty) = self.type_names.get(&key) {
            return Ok(ty.clone());
        }
        let (ty, attributes) = self.specifier_qualifiers(&name.specifiers)?;
        let ty = if let Some(declarator) = &name.declarator {
            self.declarator(ty, declarator, &attributes)?.1
        } else {
            self.apply_calling_convention(ty, &attributes, start)?
        };
        self.type_names.insert(key, ty.clone());
        Ok(ty)
    }

    pub(crate) fn declarator(
        &mut self,
        ty: Type,
        declaration: &Node<ast::Declarator>,
        attributes: &Attributes,
    ) -> Result<(Option<String>, Type, Attributes), Error> {
        let alias_base = attributes.alias_base && !has_function_derivation(declaration);
        let (name, ty, extra) = self.declarator_at(ty, declaration, None, alias_base)?;
        if attributes.calling_convention.is_none() {
            return Ok((name, ty, extra));
        }
        let mut attributes = attributes.clone();
        attributes.alias_base = alias_base;
        if attributes.alias_base
            && attributes.calling_convention.is_some()
            && extra.calling_convention.is_some()
            && attributes.calling_convention != extra.calling_convention
        {
            return Err(Error::new(
                declaration.span.start,
                "conflicting calling convention attributes",
            ));
        }
        let ty = self.apply_calling_convention(ty, &attributes, declaration.span.start)?;
        Ok((name, ty, extra))
    }

    /// `parameter_array` identifies the outermost array adjusted to a pointer.
    fn declarator_at(
        &mut self,
        mut ty: Type,
        declaration: &Node<ast::Declarator>,
        parameter_array: Option<usize>,
        alias_base: bool,
    ) -> Result<(Option<String>, Type, Attributes), Error> {
        if self.nesting >= 128 {
            return Err(Error::new(
                declaration.span.start,
                "declarator nesting limit exceeded",
            ));
        }
        self.nesting += 1;
        let mut alias_convention = None;
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
                                let mut attributes = Attributes {
                                    alias_base,
                                    ..Attributes::default()
                                };
                                self.attributes(extensions, &mut attributes)?;
                                if alias_base {
                                    merge_convention(
                                        &mut alias_convention,
                                        attributes.calling_convention,
                                        qualifier.span.start,
                                    )?;
                                }
                                pointer = self.apply_calling_convention(
                                    pointer,
                                    &attributes,
                                    qualifier.span.start,
                                )?;
                                if attributes.packed
                                    || attributes.alignment.is_some()
                                    || attributes.mode.is_some()
                                    || attributes.link_name.is_some()
                                {
                                    return Err(Error::new(
                                        qualifier.span.start,
                                        "attributes that change a nested type's representation are unsupported",
                                    ));
                                }
                            }
                        }
                    }
                    self.check_restrict(&pointer, derived.span.start)?;
                    pointer
                }
                ast::DerivedDeclarator::Array(array) => {
                    if (!array.node.qualifiers.is_empty()
                        || matches!(array.node.size, ast::ArraySize::StaticExpression(_)))
                        && parameter_array != Some(derived.span.start)
                    {
                        return Err(Error::new(
                            derived.span.start,
                            "array qualifiers and static require an outermost parameter array",
                        ));
                    }
                    if !self.is_complete_object(&ty, 0)? {
                        return Err(Error::new(
                            derived.span.start,
                            "array element must have complete object type",
                        ));
                    }
                    let kind = match &array.node.size {
                        ast::ArraySize::Unknown => TypeKind::Array {
                            element: Box::new(ty),
                            length: None,
                        },
                        ast::ArraySize::VariableExpression(expression)
                        | ast::ArraySize::StaticExpression(expression) => {
                            let bound = self.value_expression_type(expression)?;
                            self.integer_type(&bound, expression.span.start)?;
                            let constant = if self.is_integer_constant_expression(expression, 0)? {
                                // Undefined arithmetic does not form an ICE. Such an
                                // expression remains a runtime bound; executing it is UB.
                                self.eval(expression).ok()
                            } else {
                                None
                            };
                            if let Some(constant) = constant {
                                let length = constant.as_u64()?;
                                TypeKind::Array {
                                    element: Box::new(ty),
                                    length: Some(length),
                                }
                            } else {
                                TypeKind::VariableArray {
                                    element: Box::new(ty),
                                }
                            }
                        }
                        ast::ArraySize::VariableUnknown => {
                            if self.lexical_scopes.last().is_none_or(|scope| {
                                scope.is_block || scope.is_definition_parameters
                            }) {
                                return Err(Error::new(
                                    derived.span.start,
                                    "star array bounds require function prototype scope",
                                ));
                            }
                            TypeKind::VariableArray {
                                element: Box::new(ty),
                            }
                        }
                    };
                    Type::new(kind)
                }
                ast::DerivedDeclarator::Function(function) => {
                    if matches!(
                        self.unit.resolve(&ty)?.kind,
                        TypeKind::Array { .. }
                            | TypeKind::VariableArray { .. }
                            | TypeKind::Function(_)
                    ) {
                        return Err(Error::new(
                            derived.span.start,
                            "a function cannot return an array or function",
                        ));
                    }
                    if let Some(checked) = &mut self.checked {
                        checked.enter_scope(ScopeKind::Prototype, derived.span, None)?;
                    }
                    self.lexical_scopes.push(LexicalScope {
                        is_definition_parameters: self.definition_parameters
                            == Some(derived.span.start),
                        parameters: Vec::with_capacity(function.node.parameters.len()),
                        ..LexicalScope::default()
                    });
                    let prototype = !function.node.parameters.is_empty();
                    for parameter in &function.node.parameters {
                        let storage = storage_specifiers(&parameter.node.specifiers)?;
                        if storage.thread_local
                            || !matches!(
                                storage.class,
                                None | Some(ast::StorageClassSpecifier::Register)
                            )
                        {
                            return Err(Error::new(
                                parameter.span.start,
                                "only register storage is permitted for a parameter",
                            ));
                        }
                        if parameter.node.specifiers.iter().any(|specifier| {
                            matches!(specifier.node, ast::DeclarationSpecifier::Alignment(_))
                        }) {
                            return Err(Error::new(
                                parameter.span.start,
                                "alignment is not permitted on a parameter",
                            ));
                        }
                        let (base, mut attributes) = self.specifiers(&parameter.node.specifiers)?;
                        attributes.alias_base = attributes.alias_base
                            && !parameter
                                .node
                                .declarator
                                .as_ref()
                                .is_some_and(has_function_derivation);
                        let mut array_qualifiers = Qualifiers::default();
                        let (name, mut parameter_type) =
                            if let Some(declarator) = &parameter.node.declarator {
                                let array = outermost_derived(declarator).and_then(|derived| {
                                    if let ast::DerivedDeclarator::Array(array) = &derived.node {
                                        Some((derived.span.start, array))
                                    } else {
                                        None
                                    }
                                });
                                if let Some((_, array)) = array {
                                    for qualifier in &array.node.qualifiers {
                                        add_qualifier(&mut array_qualifiers, qualifier)?;
                                    }
                                }
                                let (name, ty, _) = self.declarator_at(
                                    base,
                                    declarator,
                                    array.map(|(offset, _)| offset),
                                    attributes.alias_base,
                                )?;
                                (name, ty)
                            } else {
                                (None, base)
                            };
                        parameter_type = self.apply_calling_convention(
                            parameter_type,
                            &attributes,
                            parameter.span.start,
                        )?;
                        let mut extra = Attributes::default();
                        self.attributes(&parameter.node.extensions, &mut extra)?;
                        parameter_type = self.apply_calling_convention(
                            parameter_type,
                            &extra,
                            parameter.span.start,
                        )?;
                        let qualifiers = self.unit.qualifiers(&parameter_type)?;
                        if matches!(self.unit.resolve(&parameter_type)?.kind, TypeKind::Void)
                            && (qualifiers != Qualifiers::default()
                                || (storage.class.is_some()
                                    && matches!(
                                        self.unit.target,
                                        Target::X86_64UnknownLinuxGnu
                                            | Target::Aarch64UnknownLinuxGnu
                                    )))
                        {
                            return Err(Error::new(
                                parameter.span.start,
                                "void parameter must be unqualified",
                            ));
                        }
                        parameter_type = match &self.unit.resolve(&parameter_type)?.kind {
                            TypeKind::Array { element, .. }
                            | TypeKind::VariableArray { element } => {
                                // Qualifying an array typedef qualifies its elements.
                                // Parameter adjustment removes only the array layer.
                                let mut element = (**element).clone();
                                element.qualifiers.is_const |= qualifiers.is_const;
                                element.qualifiers.is_volatile |= qualifiers.is_volatile;
                                element.qualifiers.is_restrict |= qualifiers.is_restrict;
                                let mut pointer = element.pointer();
                                pointer.qualifiers = array_qualifiers;
                                pointer
                            }
                            TypeKind::Function(_) => parameter_type.pointer(),
                            _ => parameter_type,
                        };
                        if let Some(checked) = &mut self.checked
                            && (name.is_some()
                                || !matches!(
                                    self.unit.resolve(&parameter_type)?.kind,
                                    TypeKind::Void
                                ))
                        {
                            checked.local_declaration(
                                parameter,
                                OccurrenceKind::Parameter,
                                LocalDeclaration {
                                    name: name.as_deref(),
                                    name_span: parameter
                                        .node
                                        .declarator
                                        .as_ref()
                                        .and_then(declarator_name_span),
                                    ty: &parameter_type,
                                    kind: EntityKind::Parameter,
                                    storage: Storage::Automatic,
                                    linked: false,
                                    register: storage.class
                                        == Some(ast::StorageClassSpecifier::Register),
                                    definition: false,
                                    allocation: None,
                                },
                            )?;
                        }
                        let scope = self
                            .lexical_scopes
                            .last_mut()
                            .expect("prototype scope is active");
                        if let Some(name) = &name {
                            if parameter.node.specifiers.iter().any(|specifier| matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(storage) if storage.node == ast::StorageClassSpecifier::Register)) {
                                scope.register.insert(name.clone());
                            }
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
        let calling_convention = attributes.calling_convention;
        let (name, ty, mut attributes) = match &declaration.node.kind.node {
            ast::DeclaratorKind::Identifier(identifier) => {
                (Some(identifier.node.name.clone()), ty, attributes)
            }
            ast::DeclaratorKind::Abstract => (None, ty, attributes),
            ast::DeclaratorKind::Declarator(inner) => {
                let (name, ty, inner_attributes) =
                    self.declarator_at(ty, inner, parameter_array, alias_base)?;
                if alias_base {
                    merge_convention(
                        &mut alias_convention,
                        inner_attributes.calling_convention,
                        declaration.span.start,
                    )?;
                }
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
        let ty = self.apply_calling_convention(
            ty,
            &Attributes {
                calling_convention,
                alias_base,
                ..Attributes::default()
            },
            declaration.span.start,
        )?;
        if alias_base {
            merge_convention(
                &mut alias_convention,
                attributes.calling_convention,
                declaration.span.start,
            )?;
            attributes.calling_convention = alias_convention;
        }
        self.nesting -= 1;
        Ok((name, ty, attributes))
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
                    || binding.depth == self.lexical_scopes.len()
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
            if let Some(scope) = self.lexical_scopes.last_mut() {
                scope.record_ids.push(id);
            }
            if let Some(name) = name {
                self.bind_tag(name, Tag::Record(id));
            }
            id
        };
        if let Some(checked) = &mut self.checked {
            checked.tag(
                declaration,
                OccurrenceKind::Record,
                self.unit.records[id].name.as_deref(),
                Type::new(TypeKind::Record(id)),
                declaration.node.declarations.is_some(),
                declaration.node.identifier.as_ref().map(|name| name.span),
            )?;
        }
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
                            // GNU and Clang accept declarations without members,
                            // including nested tag definitions. Only a directly
                            // written unnamed record declares an anonymous member.
                            if !matches!(base.kind, TypeKind::Record(id) if self.unit.records[id].name.is_none())
                            {
                                continue;
                            }
                            let member = Field {
                                name: None,
                                ty: base,
                                bit_width: None,
                                alignment: attributes.alignment,
                                packed: attributes.packed,
                            };
                            if let Some(checked) = &mut self.checked {
                                checked.member_declaration(
                                    field,
                                    crate::checked::OccurrenceKind::Field,
                                    id,
                                    fields.len(),
                                    &member,
                                    None,
                                )?;
                            }
                            fields.push(member);
                        } else {
                            for declarator in &field.node.declarators {
                                let (name, ty, extra) =
                                    if let Some(declarator) = &declarator.node.declarator {
                                        self.declarator(base.clone(), declarator, &attributes)?
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
                                let member = Field {
                                    name,
                                    ty,
                                    bit_width,
                                    alignment: extra.alignment.or(attributes.alignment),
                                    packed: extra.packed || attributes.packed,
                                };
                                if let Some(checked) = &mut self.checked {
                                    checked.member_declaration(
                                        declarator,
                                        crate::checked::OccurrenceKind::StructDeclarator,
                                        id,
                                        fields.len(),
                                        &member,
                                        crate::checked::references::member_name_span(declarator),
                                    )?;
                                }
                                fields.push(member);
                            }
                        }
                    }
                }
            }
            let mut member_names = HashSet::new();
            let mut has_named_member = false;
            for (index, field) in fields.iter().enumerate() {
                if self.unit.is_variably_modified(&field.ty)? {
                    return Err(Error::new(
                        declaration.span.start,
                        "record members cannot have variably modified type",
                    ));
                }
                if matches!(
                    self.unit.resolve(&field.ty)?.kind,
                    TypeKind::Array { length: None, .. }
                ) {
                    if kind == RecordKind::Union || index + 1 != fields.len() || !has_named_member {
                        return Err(Error::new(
                            declaration.span.start,
                            "flexible array must be the final member after a named member",
                        ));
                    }
                } else if !self.is_complete_object(&field.ty, 0)? {
                    return Err(Error::new(
                        declaration.span.start,
                        "field must have complete object type",
                    ));
                }
                self.check_member_names(
                    std::slice::from_ref(field),
                    &mut member_names,
                    declaration.span.start,
                    0,
                )?;
                // GCC also counts anonymous records containing only unnamed bitfields.
                has_named_member |= !member_names.is_empty()
                    || (matches!(
                        self.unit.target,
                        Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
                    ) && field.name.is_none()
                        && field.bit_width.is_none());
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

    /// Anonymous members share their containing record's member namespace.
    fn check_member_names<'a>(
        &'a self,
        fields: &'a [Field],
        names: &mut HashSet<&'a str>,
        offset: usize,
        depth: usize,
    ) -> Result<(), Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "anonymous member nesting exceeds the 128-level limit",
            ));
        }
        for field in fields {
            if let Some(name) = &field.name {
                if !names.insert(name) {
                    return Err(Error::new(offset, format!("duplicate field name `{name}`")));
                }
            } else if field.bit_width.is_none()
                && let TypeKind::Record(id) = self.unit.resolve(&field.ty)?.kind
            {
                let record = self
                    .unit
                    .records
                    .get(id)
                    .ok_or_else(|| Error::new(offset, "invalid record identity"))?;
                if let Some(fields) = &record.fields {
                    self.check_member_names(fields, names, offset, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    /// Checks completeness without requiring a supported target layout.
    pub(crate) fn is_complete_object(&self, ty: &Type, depth: usize) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "object type nesting exceeds the 128-level limit",
            ));
        }
        Ok(match &self.unit.resolve(ty)?.kind {
            TypeKind::Void | TypeKind::Function(_) => false,
            TypeKind::Record(id) => self
                .unit
                .records
                .get(*id)
                .ok_or_else(|| Error::new(0, "invalid record identity"))?
                .fields
                .is_some(),
            TypeKind::Enum(id) => {
                self.unit
                    .enums
                    .get(*id)
                    .ok_or_else(|| Error::new(0, "invalid enum identity"))?
                    .complete
            }
            TypeKind::Array { element, length } => {
                length.is_some() && self.is_complete_object(element, depth + 1)?
            }
            TypeKind::VariableArray { element } => self.is_complete_object(element, depth + 1)?,
            _ => true,
        })
    }

    /// C11 6.7.3 permits `restrict` only on pointers to object or incomplete types.
    fn check_restrict(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if !self.unit.qualifiers(ty)?.is_restrict {
            return Ok(());
        }
        let mut resolved = self.unit.resolve(ty)?;
        // GNU C propagates qualifiers on array typedefs to their element type.
        if matches!(
            self.unit.target,
            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
        ) {
            for _ in 0..128 {
                let (TypeKind::Array { element, .. } | TypeKind::VariableArray { element }) =
                    &resolved.kind
                else {
                    break;
                };
                resolved = self.unit.resolve(element)?;
            }
        }
        if let TypeKind::Pointer(pointee) = &resolved.kind
            && !matches!(self.unit.resolve(pointee)?.kind, TypeKind::Function(_))
        {
            return Ok(());
        }
        Err(Error::new(
            offset,
            "restrict requires a pointer to an object or incomplete type",
        ))
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
                    || binding.depth == self.lexical_scopes.len()
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
                complete: false,
                variants: Vec::new(),
            });
            if let Some(scope) = self.lexical_scopes.last_mut() {
                scope.enum_ids.push(id);
            }
            if let Some(name) = name {
                self.bind_tag(name, Tag::Enum(id));
            }
            id
        };
        if let Some(checked) = &mut self.checked {
            checked.tag(
                declaration,
                OccurrenceKind::Enum,
                self.unit.enums[id].name.as_deref(),
                Type::new(TypeKind::Enum(id)),
                !declaration.node.enumerators.is_empty(),
                declaration.node.identifier.as_ref().map(|name| name.span),
            )?;
        }
        if !declaration.node.enumerators.is_empty()
            && (self.unit.enums[id].complete || !self.defining_enums.insert(id))
        {
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
            let value = if value.fits_int() {
                IntegerValue::int(value.signed_value())
            } else {
                value
            };
            let name = enumerator.node.identifier.node.name.clone();
            let duplicate = if let Some(scope) = self.lexical_scopes.last_mut() {
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
            if let Some(scope) = self.lexical_scopes.last_mut() {
                scope.constants.push((name.clone(), previous_binding));
            }
            if let Some(checked) = &mut self.checked {
                checked.enumerator(
                    enumerator,
                    id,
                    self.unit.enums[id].variants.len(),
                    &crate::integer::integer_to_type(value),
                )?;
            }
            self.unit.enums[id]
                .variants
                .push(EnumVariant { name, value });
            previous = Some(value);
        }
        if !declaration.node.enumerators.is_empty() {
            self.unit.enums[id].complete = true;
            self.finish_enum(id, declaration.span.start)?;
            self.defining_enums.remove(&id);
        }
        Ok(id)
    }

    /// Clang lets a later unannotated declaration inherit an established ABI.
    pub(crate) fn inherit_calling_convention(
        &self,
        ty: Type,
        previous: &Type,
    ) -> Result<Type, Error> {
        if !matches!(
            self.unit.target,
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin | Target::X86_64PcWindowsMsvc
        ) {
            return Ok(ty);
        }
        let TypeKind::Function(function) = &self.unit.resolve(&ty)?.kind else {
            return Ok(ty);
        };
        let TypeKind::Function(previous) = &self.unit.resolve(previous)?.kind else {
            return Ok(ty);
        };
        if function.calling_convention != CallingConvention::C
            || previous.calling_convention == CallingConvention::C
        {
            return Ok(ty);
        }
        let mut ty = self.unit.resolve(&ty)?.clone();
        let TypeKind::Function(function) = &mut ty.kind else {
            unreachable!()
        };
        function.calling_convention = previous.calling_convention;
        Ok(ty)
    }

    /// Applies an ABI attribute at the declaration's type boundary. GCC accepts
    /// a function or one pointer to a function; Clang also traverses arrays and
    /// additional pointers, and can replace conventions inside pointer aliases.
    pub(crate) fn apply_calling_convention(
        &self,
        ty: Type,
        attributes: &Attributes,
        offset: usize,
    ) -> Result<Type, Error> {
        let Some(convention) = attributes.calling_convention else {
            return Ok(ty);
        };
        self.apply_convention_at(ty, convention, offset, 0, attributes.alias_base)
    }

    fn apply_convention_at(
        &self,
        ty: Type,
        convention: CallingConvention,
        offset: usize,
        depth: usize,
        alias_base: bool,
    ) -> Result<Type, Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "calling convention type nesting exceeds the 128-level limit",
            ));
        }
        let qualifiers = self.unit.qualifiers(&ty)?;
        let mut resolved = self.unit.resolve(&ty)?.clone();
        let clang = matches!(
            self.unit.target,
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin | Target::X86_64PcWindowsMsvc
        );
        let alias_base = alias_base
            || (matches!(ty.kind, TypeKind::Typedef(_))
                && matches!(resolved.kind, TypeKind::Pointer(_) | TypeKind::Array { .. }));
        resolved.qualifiers = qualifiers;
        match &mut resolved.kind {
            TypeKind::Function(function) => {
                let effective = convention
                    .for_target(self.unit.target)
                    .map_err(|mut error| {
                        error.offset = offset;
                        error
                    })?;
                if !(clang && alias_base)
                    && function.calling_convention != CallingConvention::C
                    && function.calling_convention.for_target(self.unit.target)? != effective
                {
                    return Err(Error::new(
                        offset,
                        "conflicting calling convention attributes",
                    ));
                }
                function.calling_convention = convention;
            }
            TypeKind::Pointer(element) => {
                if !clang && !matches!(self.unit.resolve(element)?.kind, TypeKind::Function(_)) {
                    return Ok(ty);
                }
                **element = self.apply_convention_at(
                    (**element).clone(),
                    convention,
                    offset,
                    depth + 1,
                    alias_base,
                )?;
            }
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element } => {
                if !clang {
                    return Ok(ty);
                }
                **element = self.apply_convention_at(
                    (**element).clone(),
                    convention,
                    offset,
                    depth + 1,
                    alias_base,
                )?;
            }
            _ => return Ok(ty),
        }
        Ok(resolved)
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
                    result.link_name = Some(decode_strings(
                        self.decode_string_literal(label, extension.span.start)?,
                        extension.span.start,
                    )?);
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
                        "cdecl" | "stdcall" | "fastcall" | "thiscall" | "ms_abi" | "sysv_abi" => {
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    "calling convention attributes take no arguments",
                                ));
                            }
                            let convention = match name {
                                "ms_abi" => Some(CallingConvention::Win64),
                                "sysv_abi" => Some(CallingConvention::SysV64),
                                // GNU ignores these x86-32 conventions on its
                                // 64-bit targets. Clang retains explicit cdecl.
                                "cdecl"
                                    if matches!(self.unit.target, Target::X86_64AppleDarwin) =>
                                {
                                    Some(CallingConvention::SysV64)
                                }
                                "cdecl"
                                    if matches!(self.unit.target, Target::X86_64PcWindowsMsvc) =>
                                {
                                    Some(CallingConvention::Win64)
                                }
                                _ => None,
                            };
                            if let Some(convention) = convention {
                                if result
                                    .calling_convention
                                    .is_some_and(|old| old != convention)
                                {
                                    return Err(Error::new(
                                        extension.span.start,
                                        "conflicting calling convention attributes",
                                    ));
                                }
                                result.calling_convention = Some(convention);
                            }
                        }
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

    pub(crate) fn static_assert(
        &mut self,
        assertion: &Node<ast::StaticAssert>,
    ) -> Result<(), Error> {
        let message =
            self.decode_string_literal(&assertion.node.message, assertion.node.message.span.start)?;
        let value = self.eval(&assertion.node.expression)?;
        if !value.truth() {
            let message = message
                .to_bytes()
                .and_then(|mut bytes| {
                    bytes.pop();
                    String::from_utf8(bytes).ok()
                })
                .unwrap_or_else(|| {
                    self.string_literal_tokens(&assertion.node.message)
                        .join(" ")
                });
            return Err(Error::new(
                assertion.span.start,
                format!("static assertion failed: {message}"),
            ));
        }
        if self.checked.is_some() {
            self.retain_static_assertion(assertion, value, message)?;
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

fn merge_convention(
    current: &mut Option<CallingConvention>,
    next: Option<CallingConvention>,
    offset: usize,
) -> Result<(), Error> {
    if let Some(next) = next {
        if current.is_some_and(|current| current != next) {
            return Err(Error::new(
                offset,
                "conflicting calling convention attributes",
            ));
        }
        *current = Some(next);
    }
    Ok(())
}

fn has_function_derivation(mut declaration: &Node<ast::Declarator>) -> bool {
    loop {
        if declaration.node.derived.iter().any(|derived| {
            matches!(
                derived.node,
                ast::DerivedDeclarator::Function(_) | ast::DerivedDeclarator::KRFunction(_)
            )
        }) {
            return true;
        }
        if let ast::DeclaratorKind::Declarator(inner) = &declaration.node.kind.node {
            declaration = inner;
        } else {
            return false;
        }
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

fn decode_strings(decoded: crate::DecodedString, offset: usize) -> Result<String, Error> {
    let mut bytes = decoded
        .to_bytes()
        .ok_or_else(|| Error::new(offset, "text context requires an ordinary or UTF-8 string"))?;
    bytes.pop();
    String::from_utf8(bytes)
        .map_err(|_| Error::new(offset, "text context requires valid UTF-8 bytes"))
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
    let mut grouping = 0usize;
    let mut pending_colons = vec![0usize];
    let mut active_colons = 0usize;
    // lang-c recursively parses labels and unbraced control flow before our
    // semantic depth checks run. Count introducers across a whole outer brace
    // region, including siblings: semicolons do not end dangling-else chains.
    let mut control_tokens = 0usize;
    #[derive(Default)]
    struct ExpressionDepth {
        operators: usize,
        child: usize,
        sibling: usize,
    }
    impl ExpressionDepth {
        fn depth(&self) -> usize {
            self.sibling.max(self.operators + self.child)
        }
        fn next_expression(&mut self) {
            self.sibling = self.depth();
            self.operators = 0;
            self.child = 0;
        }
    }
    let mut expressions = vec![ExpressionDepth::default()];
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
                if bytes[index] == b'{' {
                    pending_colons.push(0);
                } else {
                    grouping += 1;
                }
                expressions.push(ExpressionDepth::default());
                nesting += 1;
                if nesting > 128 {
                    return Err(Error::new(
                        index,
                        "syntactic nesting exceeds the 128-level limit",
                    ));
                }
            }
            b')' | b']' | b'}' => {
                if bytes[index] == b'}' {
                    if pending_colons.len() > 1 {
                        active_colons -= pending_colons.pop().expect("brace region");
                    }
                    if pending_colons.len() == 1 {
                        control_tokens = 0;
                    }
                } else {
                    grouping = grouping.saturating_sub(1);
                }
                nesting = nesting.saturating_sub(1);
                if expressions.len() > 1 {
                    let depth = expressions.pop().expect("nested expression").depth();
                    let parent = expressions.last_mut().expect("root expression");
                    parent.child = parent.child.max(depth);
                }
            }
            b':' => {
                *pending_colons.last_mut().expect("root region") += 1;
                active_colons += 1;
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                let start = index;
                while bytes
                    .get(index + 1)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                {
                    index += 1;
                }
                if matches!(
                    &source[start..=index],
                    "if" | "else" | "for" | "while" | "do" | "switch"
                ) {
                    control_tokens += 1;
                }
            }
            b';' | b',' => {
                if bytes[index] == b';' && grouping == 0 {
                    // A completed statement ends its label chain. Keep labels
                    // in parent braces and across for-header semicolons active.
                    let current = pending_colons.last_mut().expect("root region");
                    active_colons -= *current;
                    *current = 0;
                }
                expressions
                    .last_mut()
                    .expect("expression frame")
                    .next_expression();
            }
            b'*' | b'!' | b'~' | b'+' | b'-' | b'/' | b'%' | b'&' | b'|' | b'^' | b'?' | b'<'
            | b'>' | b'=' => {
                expressions.last_mut().expect("expression frame").operators += 1;
                let current = expressions.last().expect("expression frame");
                let ancestors: usize = expressions[..expressions.len() - 1]
                    .iter()
                    .map(|frame| frame.operators)
                    .sum();
                if ancestors + current.depth() > 256 {
                    return Err(Error::new(
                        index,
                        "expression or declarator exceeds the operator limit",
                    ));
                }
            }
            _ => {}
        }
        if control_tokens + active_colons > 1024 {
            return Err(Error::new(
                index,
                "control-flow introducers and pending colons exceed the 1024-token limit within an outer brace region",
            ));
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
