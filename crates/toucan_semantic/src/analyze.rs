use std::collections::BTreeMap;

use lang_c::{ast, driver, span::Node};
use rustc_hash::{FxHashMap, FxHashSet};
use toucan_target::{Compiler, CompilerProfile, Target};

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

/// Bounds branching type traversals as well as their separately checked depth.
/// Shared callback typedefs can otherwise expand into exponentially many visits.
pub(crate) struct TypeComparisonBudget(usize);

impl TypeComparisonBudget {
    pub(crate) fn new() -> Self {
        Self(1_000_000)
    }

    pub(crate) fn step(&mut self) -> Result<(), Error> {
        self.0 = self
            .0
            .checked_sub(1)
            .ok_or_else(|| Error::new(0, "type comparison work limit exceeded"))?;
        Ok(())
    }
}

/// Parses and checks preprocessed C declarations and bodies without a subprocess.
///
/// Pack pragmas are interpreted before parsing. Definition markers let binding
/// generators omit inline functions after their bodies have been checked.
pub fn analyze(source: &str, target: Target) -> Result<TranslationUnit, Error> {
    analyze_inner(source, target, None).map(|(unit, _)| unit)
}

/// Checks preprocessed C and optionally retains its owned semantic graph.
///
/// A successful retained result has complete expression, statement, and initializer
/// coverage, excluding attribute metadata. Unsupported
/// retention returns a source-positioned diagnostic rather than an incomplete graph.
pub fn analyze_with_options(
    source: &str,
    target: Target,
    options: &crate::AnalysisOptions,
) -> Result<crate::Analysis, Error> {
    analyze_with_profile(source, CompilerProfile::default_for(target), options)
}

/// Checks C under an explicitly validated compiler and physical target, retaining
/// code when requested. Constant queries later reuse this choice from the unit.
pub fn analyze_with_profile(
    source: &str,
    profile: CompilerProfile,
    options: &crate::AnalysisOptions,
) -> Result<crate::Analysis, Error> {
    let (
        unit,
        checked,
        declaration_origins,
        object_values,
        documentation_origins,
        parameter_type_dependencies,
    ) = crate::with_parser_stack(|| {
        analyze_on_parser_stack(
            source,
            profile,
            options.retain_code.then_some(options.limits),
            options.retain_declaration_origins,
            options.retain_object_values,
            options.retain_documentation_origins,
            options.retain_parameter_type_dependencies,
        )
    })??;
    Ok(crate::Analysis {
        unit,
        checked,
        declaration_origins,
        object_values,
        documentation_origins,
        parameter_type_dependencies,
    })
}

pub(crate) fn analyze_inner(
    source: &str,
    target: Target,
    retention: Option<CodeLimits>,
) -> Result<(TranslationUnit, Option<CheckedCode>), Error> {
    crate::with_parser_stack(|| {
        analyze_on_parser_stack(
            source,
            CompilerProfile::default_for(target),
            retention,
            false,
            false,
            false,
            false,
        )
        .map(|(unit, checked, _, _, _, _)| (unit, checked))
    })?
}

type AnalysisParts = (
    TranslationUnit,
    Option<CheckedCode>,
    Option<Box<crate::DeclarationOrigins>>,
    Option<Box<crate::ObjectValues>>,
    Option<Box<crate::DocumentationDeclarations>>,
    Option<Box<crate::ParameterTypeDependencies>>,
);

fn analyze_on_parser_stack(
    source: &str,
    profile: CompilerProfile,
    retention: Option<CodeLimits>,
    retain_declaration_origins: bool,
    retain_object_values: bool,
    retain_documentation_origins: bool,
    retain_parameter_type_dependencies: bool,
) -> Result<AnalysisParts, Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let (source, packs) = prepare_source(source)?;
    let parsed = parse(
        &source,
        profile.target(),
        profile.compiler(),
        profile.language_mode(),
    )?;
    let mut analyzer = Analyzer::new(profile, packs);
    analyzer.declaration_origins =
        retain_declaration_origins.then(|| Box::new(crate::declaration_origins::Builder::new()));
    analyzer.object_values =
        retain_object_values.then(|| Box::new(crate::object_values::Builder::new()));
    analyzer.documentation_origins = retain_documentation_origins
        .then(|| Box::new(crate::documentation_origins::Builder::new()));
    analyzer.parameter_type_dependencies = retain_parameter_type_dependencies
        .then(|| Box::new(crate::parameter_dependencies::Builder::new()));
    analyzer.prepare_dll_storage(&source);
    analyzer.prepare_typedef_alignments(Syntax::Unit(&parsed), &source)?;
    analyzer.prepare_array_identities(Syntax::Unit(&parsed), &source)?;
    analyzer.prepare_late_function_targets(Syntax::Unit(&parsed), &source)?;
    analyzer.prepare_inline_definitions(&parsed, &source)?;
    if let Some(limits) = retention {
        analyzer.checked = Some(Box::new(CodeBuilder::new(&parsed, source.len(), limits)?));
    }
    for external in parsed.0 {
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
    analyzer.validate_sve_features()?;
    analyzer.finish_tentative_definitions()?;
    analyzer.validate_block_externs()?;
    analyzer.validate_weak_symbol_aliases()?;
    analyzer.validate_returns_twice_aliases()?;
    analyzer.finish_inline_targets()?;
    analyzer.finish_inline_definitions()?;
    analyzer.finish_dll_inline_definitions();
    if analyzer.needs_tag_discovery {
        // Keep the ordinary analysis path free to drop each parsed declaration.
        // Only this rare compatibility case needs a second, bounded syntax walk.
        let syntax = parse(
            &source,
            profile.target(),
            profile.compiler(),
            profile.language_mode(),
        )?;
        analyzer.unit.tag_discovery = crate::tag_discovery::discover(&analyzer.unit, &syntax)?;
    }
    let checked = analyzer
        .checked
        .take()
        .map(|builder| builder.finish())
        .transpose()?;
    let declaration_origins = analyzer
        .declaration_origins
        .take()
        .map(|builder| Box::new(builder.finish()));
    let object_values = analyzer
        .object_values
        .take()
        .map(|values| Box::new(values.finish()));
    let documentation_origins = analyzer
        .documentation_origins
        .take()
        .map(|builder| builder.finish(source.len()).map(Box::new))
        .transpose()?;
    let parameter_type_dependencies = analyzer
        .parameter_type_dependencies
        .take()
        .map(|builder| Box::new(builder.finish()));
    Ok((
        analyzer.unit,
        checked,
        declaration_origins,
        object_values,
        documentation_origins,
        parameter_type_dependencies,
    ))
}

/// Evaluates a supported integer fold in the translation unit's type and
/// enumerator environment, using the target's C integer conversion rules.
/// A successful query may use known code-generation facts such as an object
/// extent; it does not certify C integer-constant-expression admissibility.
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

/// Folds a supported fixed-vector expression in the unit's type and enumerator
/// environment. Numeric conversions round each lane in its target format.
/// This value query does not certify static-initializer or integer-constant-
/// expression admissibility; for example, Clang 18 rejects a static initializer
/// using `__builtin_convertvector` even when its lanes can be folded.
pub fn evaluate_vector(
    unit: &TranslationUnit,
    expression: &str,
) -> Result<crate::VectorConstant, Error> {
    evaluate_expression(unit, expression, |analyzer, expression| {
        analyzer
            .eval_vector(expression, false)?
            .into_constant(&analyzer.unit, expression.span.start)
    })
}

fn evaluate_expression<Value: Send>(
    unit: &TranslationUnit,
    expression: &str,
    evaluate: impl FnOnce(&mut Analyzer, &Node<ast::Expression>) -> Result<Value, Error> + Send,
) -> Result<Value, Error> {
    crate::with_parser_stack(|| evaluate_on_parser_stack(unit, expression, evaluate))?
}

fn evaluate_on_parser_stack<Value>(
    unit: &TranslationUnit,
    expression: &str,
    evaluate: impl FnOnce(&mut Analyzer, &Node<ast::Expression>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let profile = unit.profile()?;
    unit.validate_function_options()?;
    for value in unit.constants.values() {
        value.validate()?;
    }
    for name in unit.typedefs.keys() {
        if name != "__builtin_va_list"
            && (!name
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(*byte, b'_' | b'$'))
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$')))
        {
            return Err(Error::new(
                0,
                "invalid typedef identifier in evaluation environment",
            ));
        }
    }
    let parsed = parse_source(
        expression,
        unit.target,
        unit.compiler,
        unit.language_mode,
        |config, source| {
            driver::parse_expression(config, source, |name| unit.typedefs.contains_key(name))
                .map(|parsed| parsed.expression)
        },
    )?;
    let source = expression;
    let expression = &parsed;
    unit.validate_parameter_contracts()?;
    // Literals and known enumerator values do not need declaration identities.
    // Keep validating the public environment above, and copy only the values
    // referenced by a proven expression. Each query still owns its analyzer.
    let mut constants = BTreeMap::new();
    let mut analyzer = if value_expression(
        &expression.node,
        &unit.constants,
        &mut constants,
        0,
        &mut 4096,
    ) {
        let mut analyzer = Analyzer::new(profile, Vec::new());
        analyzer.unit.constants = constants
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect();
        analyzer
    } else {
        Analyzer::from_unit(unit.clone())
    };
    analyzer.prepare_dll_storage(source);
    analyzer.evaluation = crate::evaluation::Context::constant_query();
    let syntax = Syntax::Expression(expression);
    analyzer.prepare_array_identities(syntax, source)?;
    analyzer.prepare_typedef_alignments(syntax, source)?;
    analyzer.prepare_late_function_targets(syntax, source)?;
    let value = evaluate(&mut analyzer, expression)?;
    analyzer.validate_sve_features()?;
    Ok(value)
}

/// Collects the owner-independent values referenced by an expression that cannot
/// inspect or introduce types or objects. Unknown names and exhausted traversal
/// budgets keep the ordinary evaluation path and its limits.
fn value_expression<'a>(
    expression: &'a ast::Expression,
    constants: &BTreeMap<String, IntegerValue>,
    referenced: &mut BTreeMap<&'a str, IntegerValue>,
    depth: u8,
    remaining: &mut usize,
) -> bool {
    if depth >= 128 || *remaining == 0 {
        return false;
    }
    *remaining -= 1;
    let mut operand = |expression: &'a Node<ast::Expression>| {
        value_expression(
            &expression.node,
            constants,
            referenced,
            depth + 1,
            remaining,
        )
    };
    match expression {
        ast::Expression::Constant(_) => true,
        ast::Expression::Identifier(identifier) => {
            let name = identifier.node.name.as_str();
            if let Some(&value) = constants.get(name) {
                referenced.insert(name, value);
                true
            } else {
                false
            }
        }
        ast::Expression::UnaryOperator(unary) => {
            matches!(
                unary.node.operator.node,
                ast::UnaryOperator::Plus
                    | ast::UnaryOperator::Minus
                    | ast::UnaryOperator::Complement
                    | ast::UnaryOperator::Negate
            ) && operand(&unary.node.operand)
        }
        ast::Expression::BinaryOperator(binary) => {
            matches!(
                binary.node.operator.node,
                ast::BinaryOperator::Multiply
                    | ast::BinaryOperator::Divide
                    | ast::BinaryOperator::Modulo
                    | ast::BinaryOperator::Plus
                    | ast::BinaryOperator::Minus
                    | ast::BinaryOperator::ShiftLeft
                    | ast::BinaryOperator::ShiftRight
                    | ast::BinaryOperator::Less
                    | ast::BinaryOperator::Greater
                    | ast::BinaryOperator::LessOrEqual
                    | ast::BinaryOperator::GreaterOrEqual
                    | ast::BinaryOperator::Equals
                    | ast::BinaryOperator::NotEquals
                    | ast::BinaryOperator::BitwiseAnd
                    | ast::BinaryOperator::BitwiseXor
                    | ast::BinaryOperator::BitwiseOr
                    | ast::BinaryOperator::LogicalAnd
                    | ast::BinaryOperator::LogicalOr
            ) && operand(&binary.node.lhs)
                && operand(&binary.node.rhs)
        }
        ast::Expression::Conditional(conditional) => {
            operand(&conditional.node.condition)
                && conditional
                    .node
                    .then_expression
                    .as_ref()
                    .is_none_or(|value| operand(value))
                && operand(&conditional.node.else_expression)
        }
        _ => false,
    }
}

/// Syntax roots accepted by the preparatory semantic visitors.
#[derive(Clone, Copy)]
pub(crate) enum Syntax<'a> {
    Unit(&'a ast::TranslationUnit),
    Expression(&'a Node<ast::Expression>),
}

impl<'a> Syntax<'a> {
    pub(crate) fn visit(self, visitor: &mut impl lang_c::visit::Visit<'a>) {
        match self {
            Self::Unit(unit) => visitor.visit_translation_unit(unit),
            Self::Expression(expression) => {
                visitor.visit_expression(&expression.node, &expression.span)
            }
        }
    }
}

fn parse(
    source: &str,
    target: Target,
    compiler: Compiler,
    language_mode: toucan_target::LanguageMode,
) -> Result<ast::TranslationUnit, Error> {
    parse_source(source, target, compiler, language_mode, |config, source| {
        driver::parse_preprocessed(config, source).map(|parsed| parsed.unit)
    })
}

fn parse_source<T>(
    source: &str,
    target: Target,
    compiler: Compiler,
    language_mode: toucan_target::LanguageMode,
    parse: impl FnOnce(&driver::Config, String) -> Result<T, driver::SyntaxError>,
) -> Result<T, Error> {
    if source.len() > 16 * 1024 * 1024 {
        return Err(Error::new(0, "preprocessed input exceeds the 16 MiB limit"));
    }
    let original = source;
    let source = strip_comments(source, compiler, language_mode)?;
    let config = driver::Config {
        cpp_command: String::new(),
        cpp_options: Vec::new(),
        gnu_keywords: language_mode.is_gnu(),
        standard: match language_mode {
            toucan_target::LanguageMode::C90 | toucan_target::LanguageMode::Gnu90 => {
                driver::Standard::C90
            }
            toucan_target::LanguageMode::C99 | toucan_target::LanguageMode::Gnu99 => {
                driver::Standard::C99
            }
            toucan_target::LanguageMode::C11 | toucan_target::LanguageMode::Gnu11 => {
                driver::Standard::C11
            }
            toucan_target::LanguageMode::C17 | toucan_target::LanguageMode::Gnu17 => {
                driver::Standard::C17
            }
        },
        extensions_msvc: target.is_windows(),
        flavor: match compiler {
            Compiler::Gnu => driver::Flavor::GnuC11WithClangExtensions,
            Compiler::Clang => driver::Flavor::ClangC11,
        },
    };
    parse(&config, source).map_err(|mut error| {
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
        Error::new(error.offset, format!("C syntax error: {error}"))
    })
}

/// Comment compatibility for callers supplying source directly to the semantic
/// library. The facade has already completed translation phase three.
struct SourceComments {
    enabled: bool,
    clang_extension: bool,
}

impl SourceComments {
    fn new(compiler: Compiler, mode: toucan_target::LanguageMode) -> Self {
        Self {
            enabled: mode != toucan_target::LanguageMode::C90,
            clang_extension: mode == toucan_target::LanguageMode::C90
                && compiler == Compiler::Clang,
        }
    }

    fn starts(&mut self, bytes: &[u8], index: usize) -> bool {
        if bytes.get(index + 1) != Some(&b'/') {
            return false;
        }
        if self.clang_extension && bytes.get(index + 2) != Some(&b'*') {
            self.enabled = true;
        }
        self.enabled
    }
}

/// The parser expects comments to have been replaced in translation phase three.
fn strip_comments(
    source: &str,
    compiler: Compiler,
    mode: toucan_target::LanguageMode,
) -> Result<String, Error> {
    let mut bytes = source.as_bytes().to_vec();
    let mut comments = SourceComments::new(compiler, mode);
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
            b'/' if comments.starts(&bytes, index) => {
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

#[derive(Clone, Debug, Default)]
pub(crate) struct Attributes {
    pub(crate) dll_storage: Option<Box<crate::dll_storage::ParsedStorage>>,
    pub(crate) target_attributes: Vec<crate::target_features::ParsedTarget>,
    pub(crate) minimum_vector_width: Vec<crate::target_features::ParsedMinimumVectorWidth>,
    pub(crate) always_inline: Option<(lang_c::span::Span, bool)>,
    pub(crate) gnu_inline: Option<(lang_c::span::Span, bool)>,
    /// Written keyword and whether its compiler rejects non-function subjects.
    pub(crate) inline_specifier: Option<(lang_c::span::Span, bool)>,
    pub(crate) no_inline: Option<(lang_c::span::Span, bool)>,
    pub(crate) noescape: Vec<(lang_c::span::Span, bool)>,
    pub(crate) type_noreturn: bool,
    target_type_name: bool,
    nodebug_arguments: Option<usize>,
    pub(crate) transparent_union: Option<lang_c::span::Span>,
    pub(crate) weak: Option<lang_c::span::Span>,
    pub(crate) returns_twice: Option<lang_c::span::Span>,
    pub(crate) noreturn: Option<lang_c::span::Span>,
    pub(crate) c11_noreturn: Option<lang_c::span::Span>,
    pub(crate) diagnostic_attributes: Vec<crate::checked::attributes::ParsedDiagnosticAttribute>,
    pub(crate) type_use: Option<crate::checked::bounds::TypeUseId>,
    type_name_use: bool,
    packed: bool,
    pub(crate) alignment: Option<u64>,
    pub(crate) msvc_alignment: Option<u64>,
    pub(crate) c11_alignment: Option<u64>,
    pub(crate) link_name: Option<String>,
    mode: Option<String>,
    vector_size: Option<u64>,
    calling_convention: Option<CallingConvention>,
    alias_base: bool,
    pub(crate) typedef_base: bool,
    pub(crate) unknown_typedef_origin: bool,
}

impl Attributes {
    /// The strongest vendor alignment, retaining its spelling in separate fields.
    pub(crate) fn vendor_alignment(&self) -> Option<u64> {
        self.alignment.max(self.msvc_alignment)
    }

    /// Clang checks arity only after recognizing a supported declaration subject.
    pub(crate) fn check_nodebug_subject(&self) -> Result<(), Error> {
        if let Some(offset) = self.nodebug_arguments {
            return Err(Error::new(offset, "nodebug takes no arguments"));
        }
        Ok(())
    }

    pub(crate) fn require_no_transparent_union(&self) -> Result<(), Error> {
        if let Some(span) = self.transparent_union {
            return Err(Error::new(
                span.start,
                "transparent_union requires a union definition or typedef",
            ));
        }
        Ok(())
    }
    pub(crate) fn require_no_weak(&self) -> Result<(), Error> {
        if let Some(span) = self.weak {
            return Err(Error::new(
                span.start,
                "weak requires an external function or object declaration",
            ));
        }
        Ok(())
    }
    pub(crate) fn require_function_attributes(&self, function: bool) -> Result<(), Error> {
        if !function && let Some((span, true)) = self.inline_specifier {
            return Err(Error::new(
                span.start,
                "inline requires a function declaration",
            ));
        }
        if function && let Some((span, true)) = self.gnu_inline {
            return Err(Error::new(span.start, "gnu_inline takes no arguments"));
        }
        if !function && let Some(attribute) = self.minimum_vector_width.first() {
            return Err(Error::new(
                attribute.span.start,
                "min_vector_width requires a function declaration",
            ));
        }
        if !function
            && !self.target_type_name
            && let Some(attribute) = self
                .target_attributes
                .iter()
                .find(|attribute| attribute.clang)
        {
            return Err(Error::new(
                attribute.span.start,
                "target attribute requires a function declaration",
            ));
        }
        if !function && let Some(span) = self.c11_noreturn {
            return Err(Error::new(
                span.start,
                "_Noreturn requires a function declaration",
            ));
        }
        if !function && let Some(span) = self.returns_twice {
            return Err(Error::new(
                span.start,
                "returns_twice requires a function declaration",
            ));
        }
        if !function && let Some(attribute) = self.diagnostic_attributes.first() {
            return Err(Error::new(
                attribute.span.start,
                "diagnostic attributes require a function declaration",
            ));
        }
        Ok(())
    }
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

/// A linked block declaration, also checked against later file declarations.
pub(crate) struct BlockExtern {
    pub(crate) noreturn: bool,
    pub(crate) alignment: crate::DeclarationAlignment,
    pub(crate) ty: Type,
    pub(crate) thread_local: bool,
    pub(crate) is_static: bool,
}

/// Scope frames retain only new bindings; file-scope maps remain shared.
#[derive(Default)]
pub(crate) struct LexicalScope {
    /// GNU attribute consistency follows each binding scope, including attributes
    /// inherited by its first declaration. Most scopes need no inline state.
    #[allow(clippy::box_collection)]
    pub(crate) gnu_inline: Option<Box<FxHashMap<String, crate::inline::GnuDeclaration>>>,
    // Most scopes have no alignment annotations; keep their inline state one pointer.
    #[allow(clippy::box_collection)]
    pub(crate) alignments: Option<Box<FxHashMap<String, crate::DeclarationAlignment>>>,
    pub(crate) is_block: bool,
    pub(crate) is_definition_parameters: bool,
    pub(crate) variably_modified: Option<usize>,
    pub(crate) record_ids: Vec<usize>,
    pub(crate) enum_ids: Vec<usize>,
    pub(crate) typedefs: FxHashMap<String, Type>,
    pub(crate) static_storage: FxHashSet<String>,
    pub(crate) flexible_array_storage: FxHashMap<String, crate::FlexibleArrayStorage>,
    pub(crate) linked: FxHashSet<String>,
    pub(crate) register: FxHashSet<String>,
    pub(crate) tags: Vec<(String, Option<TagBinding>)>,
    pub(crate) constants: Vec<(String, Option<IntegerValue>)>,
    /// A parameter index, or None for an enumerator in the ordinary namespace.
    pub(crate) names: FxHashMap<String, Option<usize>>,
    pub(crate) parameters: Vec<Parameter>,
}

#[derive(Default)]
pub(crate) struct StorageSpecifiers {
    pub(crate) class: Option<ast::StorageClassSpecifier>,
    pub(crate) thread_local: bool,
}

/// C11 permits one storage class, with `_Thread_local` additionally allowed
/// beside `static` or `extern`.
pub(crate) fn storage_specifiers(
    specifiers: &[Node<ast::DeclarationSpecifier>],
    compiler: Compiler,
) -> Result<StorageSpecifiers, Error> {
    let mut storage = StorageSpecifiers::default();
    let mut gnu_thread_local = false;
    for specifier in specifiers {
        let ast::DeclarationSpecifier::StorageClass(class) = &specifier.node else {
            continue;
        };
        if matches!(
            class.node,
            ast::StorageClassSpecifier::ThreadLocal | ast::StorageClassSpecifier::GnuThreadLocal
        ) {
            if storage.thread_local {
                return Err(Error::new(
                    class.span.start,
                    "duplicate thread-local storage specifier",
                ));
            }
            storage.thread_local = true;
            gnu_thread_local = class.node == ast::StorageClassSpecifier::GnuThreadLocal;
        } else if gnu_thread_local
            && compiler == Compiler::Gnu
            && matches!(
                class.node,
                ast::StorageClassSpecifier::Static | ast::StorageClassSpecifier::Extern
            )
        {
            return Err(Error::new(
                class.span.start,
                "GNU __thread must follow static or extern",
            ));
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
pub(crate) fn outermost_derived(
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

#[derive(Clone)]
pub(crate) struct PreparedSpecifiers {
    types: Vec<Node<ast::TypeSpecifier>>,
    qualifiers: Qualifiers,
    atomic: bool,
    attributes: Attributes,
    record_attributes: Attributes,
}

#[derive(Clone, Copy)]
struct DeclaratorContext<'a> {
    parameter_array: Option<usize>,
    parenthesized: bool,
    alias_base: bool,
    base_use: Option<crate::checked::TypeUseId>,
    type_name: bool,
    definition: Option<&'a Node<ast::FunctionDefinition>>,
}

pub(crate) struct Analyzer {
    pub(crate) const_objects: Option<Box<crate::const_objects::Values>>,
    pub(crate) evaluation: crate::evaluation::Context,
    pub(crate) object_values: Option<Box<crate::object_values::Builder>>,
    pub(crate) inline_registry: Option<Box<crate::inline::Registry>>,
    pub(crate) dll_registry: Option<Box<crate::dll_storage::Registry>>,
    pub(crate) allocation_uses: u8,
    pub(crate) allocation_evaluation: crate::allocation::Evaluation,
    pub(crate) allocation_symbols: Option<Box<crate::allocation::Symbols>>,
    pub(crate) noreturn_registry: Option<Box<crate::noreturn::Registry>>,
    pub(crate) alignment_registry: Option<Box<crate::type_alignment::Registry>>,
    pub(crate) has_type_noreturn: bool,
    pub(crate) parameter_contract_index: Option<Box<crate::noescape::ContractIndex>>,
    pub(crate) shuffle_vectors: BTreeMap<(usize, usize), crate::vector::ShuffleVectorSignature>,
    pub(crate) old_style_definitions: crate::old_style::Definitions,
    pub(crate) lexical_function_options: BTreeMap<usize, BTreeMap<String, crate::FunctionOptions>>,
    pub(crate) definition_options: Option<(crate::FunctionOptions, Option<usize>)>,
    pub(crate) function_options: BTreeMap<String, crate::FunctionOptions>,
    pub(crate) late_target_names: std::collections::BTreeSet<String>,
    pub(crate) array_identities: crate::array_identity::Registry,
    // Completed query checks prevent nested constant folding from replaying operand typing.
    pub(crate) checked_overflow_predicates: FxHashMap<(usize, usize), (u8, bool)>,
    pub(crate) checked_atomic_queries: FxHashSet<(usize, usize)>,
    pub(crate) transparent_variant_bytes: usize,
    pub(crate) has_variadic_packs: bool,
    pub(crate) generic_selections: FxHashMap<(usize, usize), usize>,
    pub(crate) pending_auto_types: Vec<(String, usize)>,
    pub(crate) choose_selections: FxHashMap<(usize, usize), bool>,
    pub(crate) type_compatibility_results: FxHashMap<(usize, usize), bool>,
    pub(crate) weak_symbols: BTreeMap<String, lang_c::span::Span>,
    pub(crate) function_effects: BTreeMap<String, crate::returns_twice::FunctionEffects>,
    pub(crate) diagnostic_kinds: FxHashMap<String, u8>,
    pub(crate) checked: Option<Box<CodeBuilder>>,
    declaration_origins: Option<Box<crate::declaration_origins::Builder>>,
    pub(crate) parameter_type_dependencies: Option<Box<crate::parameter_dependencies::Builder>>,
    documentation_origins: Option<Box<crate::documentation_origins::Builder>>,
    pub(crate) unit: TranslationUnit,
    pub(crate) tags: FxHashMap<String, TagBinding>,
    pub(crate) lexical_scopes: Vec<LexicalScope>,
    pub(crate) lexical_record: Option<usize>,
    pub(crate) needs_tag_discovery: bool,
    defining_enums: FxHashSet<usize>,
    tentative_definitions: BTreeMap<usize, usize>,
    packs: PackEvents,
    nesting: usize,
    pub(crate) capture_function_scope: bool,
    definition_parameters: Option<usize>,
    pub(crate) variably_modified_parents: Vec<Option<usize>>,
    pub(crate) function_scope: Option<crate::statement::FunctionScope>,
    pub(crate) current_function: Option<crate::statement::FunctionContext>,
    pub(crate) sve_feature_uses: Vec<crate::target_features::FeatureUse>,
    pub(crate) sve_feature_labels: usize,
    pub(crate) block_externs: FxHashMap<String, BlockExtern>,
    pub(crate) alignment_queries: crate::alignof::AlignmentQueries,
    type_names: FxHashMap<(usize, usize), Type>,
}

impl Analyzer {
    pub(crate) fn enter_expression(&mut self, offset: usize) -> Result<(), Error> {
        if self.nesting >= 128 {
            return Err(Error::new(offset, "expression nesting limit exceeded"));
        }
        if let Some(dependencies) = &mut self.parameter_type_dependencies {
            dependencies.enter_expression(self.nesting);
        }
        self.nesting += 1;
        Ok(())
    }

    pub(crate) fn leave_expression(&mut self) {
        self.nesting -= 1;
        if let Some(dependencies) = &mut self.parameter_type_dependencies {
            dependencies.leave_expression(self.nesting);
        }
    }

    fn new(profile: CompilerProfile, packs: PackEvents) -> Self {
        let mut analyzer = Self::from_unit(TranslationUnit {
            target: profile.target(),
            compiler: profile.compiler(),
            language_mode: profile.language_mode(),
            declarations: Vec::new(),
            function_options: BTreeMap::new(),
            parameter_contracts: Vec::new(),
            alignment_origins: Vec::new(),
            records: Vec::new(),
            record_origins: BTreeMap::new(),
            lexical_tags: crate::TagLexicalOrigins::default(),
            tag_discovery: None,
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
            const_objects: None,
            evaluation: crate::evaluation::Context::default(),
            object_values: None,
            inline_registry: None,
            dll_registry: None,
            allocation_uses: 0,
            allocation_evaluation: Default::default(),
            allocation_symbols: None,
            noreturn_registry: None,
            alignment_registry: None,
            has_type_noreturn: crate::noescape::has_type_noreturn(&unit),
            parameter_contract_index: None,
            shuffle_vectors: BTreeMap::new(),
            old_style_definitions: crate::old_style::Definitions::default(),
            lexical_function_options: BTreeMap::new(),
            definition_options: None,
            function_options: Self::inherited_function_options(&unit),
            late_target_names: std::collections::BTreeSet::new(),
            array_identities: crate::array_identity::Registry::default(),
            diagnostic_kinds: FxHashMap::default(),
            checked: None,
            declaration_origins: None,
            parameter_type_dependencies: None,
            documentation_origins: None,
            unit,
            tags,
            lexical_scopes: Vec::new(),
            lexical_record: None,
            needs_tag_discovery: false,
            defining_enums: FxHashSet::default(),
            tentative_definitions: BTreeMap::new(),
            packs: Vec::new(),
            nesting: 0,
            capture_function_scope: false,
            definition_parameters: None,
            variably_modified_parents: Vec::new(),
            function_scope: None,
            current_function: None,
            sve_feature_uses: Vec::new(),
            sve_feature_labels: 0,
            block_externs: FxHashMap::default(),
            weak_symbols: BTreeMap::new(),
            function_effects: BTreeMap::new(),
            transparent_variant_bytes: 0,
            has_variadic_packs: false,
            generic_selections: FxHashMap::default(),
            choose_selections: FxHashMap::default(),
            pending_auto_types: Vec::new(),
            type_compatibility_results: FxHashMap::default(),
            checked_atomic_queries: FxHashSet::default(),
            checked_overflow_predicates: FxHashMap::default(),
            alignment_queries: crate::alignof::AlignmentQueries::default(),
            type_names: FxHashMap::default(),
        }
    }

    pub(crate) fn scope(&self) -> Scope {
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
        self.leave_noreturn_scope();
        self.leave_allocation_scope();
        self.leave_dll_scope();
        self.lexical_function_options
            .remove(&self.lexical_scopes.len());
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
                alignments: scope.alignments.clone(),
                old_style: None,
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
            "x86_64-unknown-linux-gnu" | "x86_64-unknown-linux-musl" | "x86_64-apple-darwin" => (
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
            "aarch64-unknown-linux-gnu" | "aarch64-unknown-linux-musl" => (
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
            transparent_union: false,
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
        self.declaration_with_definition(declaration, definition, None)
    }

    pub(crate) fn declaration_with_definition(
        &mut self,
        declaration: &Node<ast::Declaration>,
        definition: bool,
        syntax: Option<&Node<ast::FunctionDefinition>>,
    ) -> Result<(), Error> {
        let storage = storage_specifiers(&declaration.node.specifiers, self.unit.compiler)?;
        if matches!(
            storage.class,
            Some(ast::StorageClassSpecifier::Auto | ast::StorageClassSpecifier::Register)
        ) {
            return Err(Error::new(
                declaration.span.start,
                "auto and register are not permitted at file scope",
            ));
        }
        if let Some(deps) = &mut self.parameter_type_dependencies {
            deps.begin(declaration.span, false)?;
        }
        let is_typedef = storage.class == Some(ast::StorageClassSpecifier::Typedef);
        let mut auto = self.auto_declaration(declaration)?;
        let explicit = if auto.is_none() {
            Some(self.declaration_specifiers(&declaration.node)?)
        } else {
            None
        };
        if declaration.node.declarators.is_empty()
            && let Some((_, attributes)) = &explicit
        {
            attributes.require_function_attributes(false)?;
            attributes.require_no_weak()?;
            attributes.require_no_transparent_union()?;
        }
        for item in &declaration.node.declarators {
            if let Some(deps) = &mut self.parameter_type_dependencies {
                deps.begin(
                    declarator_name_span(&item.node.declarator).unwrap_or(item.span),
                    true,
                )?;
            }
            let inferred = auto
                .as_mut()
                .map(|group| self.auto_item(declaration, item, group))
                .transpose()?;
            let (base, attributes) = match &inferred {
                Some((base, attributes, _)) => (base, attributes),
                None => {
                    let (base, attributes) =
                        explicit.as_ref().expect("explicit declaration specifiers");
                    (base, attributes)
                }
            };
            let inference = inferred.as_ref().map(|(_, _, inference)| inference);
            let mut prechecked_initializer = None;
            let mut is_static = storage.class == Some(ast::StorageClassSpecifier::Static);
            let previous_parameters = self.definition_parameters;
            if definition {
                self.definition_parameters = outermost_derived(&item.node.declarator)
                    .filter(|derived| {
                        matches!(
                            derived.node,
                            ast::DerivedDeclarator::Function(_)
                                | ast::DerivedDeclarator::KRFunction(_)
                        )
                    })
                    .map(|derived| derived.span.start);
            }
            let declarator = self.declarator_with_definition(
                base.clone(),
                &item.node.declarator,
                attributes,
                syntax,
            );
            self.definition_parameters = previous_parameters;
            let (name, mut ty, mut declarator_attributes) = declarator?;
            declarator_attributes.check_nodebug_subject()?;
            if let Some(inference) = inference {
                self.check_auto_declarator(inference, &item.node.declarator, &ty)?;
            }
            if self.unit.is_variably_modified(&ty)? {
                return Err(Error::new(
                    item.span.start,
                    "variably modified identifiers require block or prototype scope",
                ));
            }
            let name =
                name.ok_or_else(|| Error::new(item.span.start, "declaration has no name"))?;
            if let Some(builtin) =
                crate::arm::builtin_type(&name, self.unit.target, self.unit.compiler)
            {
                if !is_typedef {
                    return Err(Error::new(
                        item.span.start,
                        format!("declaration conflicts with predefined Arm typedef `{name}`"),
                    ));
                }
                if !self.same_type(&builtin, &ty, 0)? {
                    let message = if self.gnu_vector_profile() {
                        "replacing a predefined Arm vector typedef is unsupported"
                    } else {
                        "conflicting predefined Arm vector typedef"
                    };
                    return Err(Error::new(item.span.start, message));
                }
            }
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
                self.align_typedef(
                    &mut ty,
                    &declaration.node.specifiers,
                    attributes,
                    &declarator_attributes,
                    item.span.start,
                    &name,
                )?;
                self.apply_transparent_typedef(&mut ty, attributes, &declarator_attributes)?;
                if let Some(previous) = self.unit.typedefs.get(&name) {
                    if !self.same_type(previous, &ty, 0)? {
                        return Err(Error::new(
                            item.span.start,
                            format!("conflicting typedef `{name}`"),
                        ));
                    }
                    let alignment = ty.alignment;
                    ty = crate::noescape::composite_type!(self, previous, &ty, 0)?;
                    ty.alignment = alignment;
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
            if kind == DeclarationKind::Variable && self.unit.is_sizeless(&ty)? {
                return Err(Error::new(
                    item.span.start,
                    "objects with static or thread storage cannot have sizeless SVE type",
                ));
            }
            if kind != DeclarationKind::Typedef {
                declarator_attributes.require_no_transparent_union()?;
            }
            declarator_attributes.require_function_attributes(kind == DeclarationKind::Function)?;
            self.check_diagnostic_attributes(&name, &declarator_attributes.diagnostic_attributes)?;
            let previous_index = if definition {
                self.definition_options.as_ref().map(|(_, index)| *index)
            } else {
                None
            }
            .unwrap_or_else(|| {
                self.unit
                    .declarations
                    .iter()
                    .position(|previous| previous.name == name)
            });
            // Microsoft C retains earlier external linkage when a later object
            // or function declaration is written `static`, in every C mode.
            if is_static
                && kind != DeclarationKind::Typedef
                && self.unit.target.is_windows()
                && (previous_index.is_some_and(|index| {
                    let previous = &self.unit.declarations[index];
                    previous.kind == kind && !previous.is_static
                }) || self
                    .block_externs
                    .get(&name)
                    .is_some_and(|previous| !previous.is_static))
            {
                is_static = false;
            }
            let mut link_name_is_literal = declarator_attributes.link_name.is_some();
            if !is_typedef {
                self.require_linked_float_name(&name, item.span.start)?;
                let external = !is_static
                    && !previous_index.is_some_and(|index| self.unit.declarations[index].is_static)
                    && !self
                        .block_externs
                        .get(&name)
                        .is_some_and(|prior| prior.is_static);
                let builtin = self.builtin_function_declaration(
                    &name,
                    &mut ty,
                    external,
                    definition,
                    &mut declarator_attributes.link_name,
                    item.span.start,
                )?;
                if builtin {
                    link_name_is_literal = self.unit.compiler == Compiler::Clang
                        && (link_name_is_literal
                            || previous_index.is_some_and(|index| {
                                self.unit.declarations[index].link_name_is_literal
                            }));
                }
                if builtin
                    && self.unit.compiler == Compiler::Gnu
                    && matches!(
                        name.as_str(),
                        "__builtin_malloc" | "__builtin_calloc" | "__builtin_realloc"
                    )
                    && declarator_attributes.c11_noreturn.is_none()
                {
                    declarator_attributes.noreturn = None;
                }
            }
            let function_options = if kind == DeclarationKind::Function {
                Some(self.check_function_options(&name, &declarator_attributes, previous_index)?)
            } else {
                None
            };
            let noreturn = kind == DeclarationKind::Function
                && self.declaration_noreturn(
                    &name,
                    &ty,
                    declarator_attributes.noreturn.is_some(),
                    previous_index,
                )?;
            let returns_twice = if kind == DeclarationKind::Function {
                self.check_returns_twice(&name, &declarator_attributes)?
            } else {
                false
            };
            if returns_twice
                && matches!(&self.unit.resolve(&ty)?.kind, TypeKind::Function(f) if f.noreturn)
            {
                return Err(Error::new(
                    item.span.start,
                    "combining returns_twice and noreturn is unsupported",
                ));
            }
            if storage.thread_local && kind != DeclarationKind::Variable {
                return Err(Error::new(
                    item.span.start,
                    "thread-local storage requires an object declaration",
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
            let is_extern = storage.class == Some(ast::StorageClassSpecifier::Extern)
                || (storage.class.is_none()
                    && !is_typedef
                    && crate::dll_storage::implicit_extern(
                        declarator_attributes.dll_storage.as_deref(),
                    ));
            let dll_storage_class = self.dll_declaration(crate::dll_storage::Declaration {
                name: &name,
                kind,
                attributes: declarator_attributes.dll_storage.as_deref(),
                specifiers: &declaration.node.specifiers,
                external: !is_static
                    && !previous_index.is_some_and(|index| self.unit.declarations[index].is_static),
                definition: is_definition,
                tentative: kind == DeclarationKind::Variable && !is_extern,
                thread_local: storage.thread_local,
                previous_definition: previous_index
                    .is_some_and(|index| self.unit.declarations[index].is_definition),
                block: false,
                offset: item.span.start,
            })?;
            if kind == DeclarationKind::Function {
                if definition {
                    self.prepare_old_style_definition(
                        &name,
                        &mut ty,
                        previous_index,
                        item.span.start,
                    )?;
                }
                if let Some(index) = previous_index {
                    self.check_old_style_redeclaration(index, &ty, item.span.start)?;
                }
            }
            let repeated_inline_body = if kind == DeclarationKind::Function {
                self.record_inline_declaration(
                    &name,
                    crate::inline::DeclarationFacts {
                        offset: item.span.start,
                        site: None,
                        inline_source: declarator_attributes.inline_specifier.map(|(span, _)| span),
                        gnu_source: declarator_attributes.gnu_inline.map(|(span, _)| span),
                        file_scope: true,
                        written_inline: declarator_attributes.inline_specifier.is_some(),
                        written_extern: storage.class == Some(ast::StorageClassSpecifier::Extern),
                        written_gnu_inline: declarator_attributes.inline_specifier.is_some()
                            && declarator_attributes.gnu_inline.is_some(),
                        body: definition,
                        internal: is_static
                            || ((storage.class.is_none()
                                || storage.class == Some(ast::StorageClassSpecifier::Extern))
                                && previous_index
                                    .is_some_and(|index| self.unit.declarations[index].is_static)),
                        inlined: false,
                        gnu_inline: false,
                    },
                )?
            } else {
                false
            };
            let written_alignment = if is_typedef {
                crate::DeclarationAlignment::default()
            } else {
                self.check_declaration_alignment(
                    &ty,
                    attributes,
                    &declarator_attributes,
                    if kind == DeclarationKind::Function {
                        crate::object_alignment::AlignmentSubject::Function
                    } else {
                        crate::object_alignment::AlignmentSubject::Object { register: false }
                    },
                    item.span.start,
                )?
            };
            let alignment_definition =
                is_definition || (kind == DeclarationKind::Variable && !is_extern);
            let mut alignment = written_alignment;
            if !is_typedef
                && (self.unit.compiler == Compiler::Gnu || previous_index.is_none())
                && let Some(previous) = self.block_externs.get(&name)
            {
                alignment = self.merge_declaration_alignment(
                    &ty,
                    previous.alignment,
                    alignment,
                    false,
                    alignment_definition,
                    item.span.start,
                )?;
            }
            let written_object_type = if kind == DeclarationKind::Variable
                && let Some(values) = &mut self.object_values
                && previous_index.is_some_and(|index| self.unit.declarations[index].ty != ty)
            {
                Some(values.copy_written_type(&ty, item.span.start)?)
            } else {
                None
            };
            let (dependency_prototype, dependency_previous_prototype) = if self
                .parameter_type_dependencies
                .is_some()
            {
                (
                    matches!(&self.unit.resolve(&ty)?.kind, TypeKind::Function(function) if function.prototype),
                    previous_index.is_some_and(|index| self.unit.resolve(&self.unit.declarations[index].ty).is_ok_and(|ty| matches!(&ty.kind, TypeKind::Function(function) if function.prototype))),
                )
            } else {
                (false, false)
            };
            let declaration_index = if let Some(previous_index) = previous_index {
                let previous = &self.unit.declarations[previous_index];
                alignment = alignment.combined(self.merge_declaration_alignment(
                    &ty,
                    previous.alignment,
                    written_alignment,
                    previous.is_definition,
                    alignment_definition,
                    item.span.start,
                )?);
                if kind == DeclarationKind::Function {
                    ty = self.inherit_calling_convention(ty, &previous.ty)?;
                    if let (TypeKind::Function(prior), TypeKind::Function(current)) = (
                        &self.unit.resolve(&previous.ty)?.kind,
                        &self.unit.resolve(&ty)?.kind,
                    ) && ((definition
                        && self
                            .function_scope
                            .as_ref()
                            .is_none_or(|scope| scope.old_style.is_none())
                        && !current.prototype
                        && !prior.parameters.is_empty())
                        || (previous.is_definition
                            && !self.has_old_style_definition(previous_index)
                            && !prior.prototype
                            && !current.parameters.is_empty()))
                    {
                        // An empty list in a definition means zero parameters;
                        // an empty list in a declaration leaves them unspecified.
                        return Err(Error::new(
                            item.span.start,
                            format!(
                                "empty parameter definition of `{name}` conflicts with its prototype"
                            ),
                        ));
                    }
                }
                if previous.is_thread_local != storage.thread_local {
                    return Err(Error::new(
                        item.span.start,
                        format!("conflicting thread-local storage for `{name}`"),
                    ));
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
                    if is_extern || (storage.class.is_none() && kind == DeclarationKind::Function) {
                        is_static = previous.is_static;
                    }
                    if previous.is_static != is_static {
                        return Err(Error::new(
                            item.span.start,
                            format!("conflicting linkage for `{name}`"),
                        ));
                    }
                    if is_definition && previous.is_definition && !repeated_inline_body {
                        return Err(Error::new(
                            item.span.start,
                            format!("multiple definitions of `{name}`"),
                        ));
                    }
                }
                self.check_transparent_definition_merge(
                    &previous.ty,
                    &ty,
                    previous.is_definition,
                    definition,
                    item.span.start,
                )?;
                // A composite type retains all available bounds and prototypes,
                // including those nested inside pointers and function parameters.
                let mut composite = if definition {
                    // Definition parameter names belong to its body; names in an
                    // earlier prototype have no bearing on those declarations.
                    crate::noescape::composite_type!(self, &ty, &previous.ty, 0)?
                } else {
                    crate::noescape::composite_type!(self, &previous.ty, &ty, 0)?
                };
                if kind != DeclarationKind::Function {
                    self.object_alignment_sugar(&mut composite, &ty)?;
                } else if definition {
                    self.object_alignment_sugar(&mut composite, &previous.ty)?;
                }
                ty = composite;
                let previous_definition = previous.is_definition;
                let symbol_binding = self.check_symbol_binding(
                    &name,
                    self.inline_weak_attribute(
                        &name,
                        declarator_attributes.weak,
                        kind == DeclarationKind::Function,
                        declarator_attributes.inline_specifier.is_some(),
                        previous_definition,
                    ),
                    kind != DeclarationKind::Typedef && !is_static,
                    previous_definition,
                )?;
                if inference.is_some()
                    && let Some(initializer) = &item.node.initializer
                {
                    prechecked_initializer =
                        Some(self.check_object_initializer(&ty, initializer, true)?);
                }
                let previous = &mut self.unit.declarations[previous_index];
                previous.dll_storage_class = dll_storage_class;
                previous.alignment = alignment;
                previous.returns_twice = returns_twice;
                previous.noreturn = noreturn;
                previous.symbol_binding = symbol_binding;
                previous.ty = ty;
                previous.is_definition |= is_definition;
                if previous.link_name.is_none()
                    || (!is_static && crate::BuiltinFunction::from_name(&previous.name).is_some())
                {
                    previous.link_name = declarator_attributes.link_name;
                    previous.link_name_is_literal = link_name_is_literal;
                }
                previous_index
            } else {
                let symbol_binding = self.check_symbol_binding(
                    &name,
                    self.inline_weak_attribute(
                        &name,
                        declarator_attributes.weak,
                        kind == DeclarationKind::Function,
                        declarator_attributes.inline_specifier.is_some(),
                        false,
                    ),
                    kind != DeclarationKind::Typedef && !is_static,
                    false,
                )?;
                if inference.is_some()
                    && let Some(initializer) = &item.node.initializer
                {
                    prechecked_initializer =
                        Some(self.check_object_initializer(&ty, initializer, true)?);
                }
                let index = self.unit.declarations.len();
                self.unit.declarations.push(Declaration {
                    function_definition_kind: None,
                    inline_facts: None,
                    dll_storage_class,
                    alignment,
                    returns_twice,
                    noreturn,
                    symbol_binding,
                    name,
                    ty,
                    kind,
                    link_name: declarator_attributes.link_name,
                    link_name_is_literal,
                    is_static,
                    is_thread_local: storage.thread_local,
                    is_definition,
                    flexible_array_storage: None,
                });
                index
            };
            if let Some(deps) = &mut self.parameter_type_dependencies {
                deps.declaration(
                    declaration_index,
                    &self.unit.declarations[declaration_index].name,
                    kind,
                    dependency_prototype,
                    previous_index.is_some(),
                    dependency_previous_prototype,
                )?;
            }
            if kind == DeclarationKind::Typedef {
                self.note_typedef_lexical_tag(declaration_index, &declaration.node.specifiers)?;
            }
            if noreturn {
                let name = self.unit.declarations[declaration_index].name.clone();
                self.record_noreturn(&name, true, true, item.span.start)?;
            }
            if definition {
                self.save_old_style_definition(declaration_index, item.span.start)?;
            }
            if let Some(options) = &function_options
                && !options.is_default()
            {
                self.unit
                    .function_options
                    .insert(declaration_index, options.clone());
            }
            let origin_inline = self.declaration_origins.is_some()
                && kind == DeclarationKind::Function
                && self.origin_function_inline(&self.unit.declarations[declaration_index].name);
            if let Some(origins) = &mut self.declaration_origins {
                origins.push(
                    crate::DeclarationTarget::Declaration(declaration_index),
                    declarator_name_span(&item.node.declarator).unwrap_or(item.span),
                    is_definition,
                    kind != DeclarationKind::Typedef && !is_static,
                    false,
                    origin_inline,
                )?;
            }
            if let Some(origins) = &mut self.documentation_origins {
                origins.push(
                    crate::DocumentationTarget::Declaration(declaration_index),
                    declaration.span.start,
                    declarator_name_span(&item.node.declarator)
                        .unwrap_or(item.span)
                        .start,
                    false,
                )?;
            }
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
            if kind == DeclarationKind::Function && self.inline_registry.is_some() {
                self.inline_file_declaration(declaration_index);
                let name = self.unit.declarations[declaration_index].name.clone();
                self.inline_declaration_site(&name, checked_site);
            }
            if kind == DeclarationKind::Variable && !is_definition && !is_extern {
                self.tentative_definitions
                    .entry(declaration_index)
                    .or_insert(item.span.start);
            }
            if let Some(initializer) = &item.node.initializer {
                // Earlier declarations contribute bounds to the object being
                // initialized, including when this declarator omits its bound.
                let initializer_type = self.unit.declarations[declaration_index].ty.clone();
                self.initialize_declaration(
                    declaration_index,
                    &initializer_type,
                    initializer,
                    prechecked_initializer,
                )?;
            }
            if kind == DeclarationKind::Variable && self.object_values.is_some() {
                self.retain_object_value(
                    declaration_index,
                    declarator_name_span(&item.node.declarator)
                        .unwrap_or(item.span)
                        .start,
                    item.node.initializer.as_ref(),
                    written_object_type,
                )?;
            }
            if let Some(initializer) = &item.node.initializer {
                // The current initializer must not see its own completed value
                // during optional capture (for example, constant_p(object)).
                self.note_const_object(declaration_index, initializer)?;
            }
            if let (Some(checked), Some(site)) = (&mut self.checked, checked_site) {
                if let Some(inference) = &inference {
                    checked.retain_type_inference(site, inference)?;
                }
                checked.attach_dll_storage(
                    site,
                    dll_storage_class,
                    declarator_attributes.dll_storage.as_deref(),
                )?;
                checked.attach_alignment(site, written_alignment, alignment)?;
                checked.attach_function_options(
                    site,
                    function_options.as_ref(),
                    (
                        &declarator_attributes.target_attributes,
                        &declarator_attributes.minimum_vector_width,
                    ),
                    declarator_attributes.always_inline,
                    declarator_attributes.no_inline,
                    true,
                )?;
                checked.attach_diagnostic_attributes(
                    site,
                    &declarator_attributes.diagnostic_attributes,
                )?;
                checked.attach_returns_twice(
                    site,
                    returns_twice,
                    declarator_attributes.returns_twice,
                )?;
                checked.attach_noreturn(site, noreturn, declarator_attributes.noreturn)?;
                checked.attach_symbol_binding(
                    site,
                    self.unit.declarations[declaration_index].symbol_binding,
                    declarator_attributes.weak,
                )?;
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
        if let Some(deps) = &mut self.parameter_type_dependencies {
            deps.discard();
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
                    alignment: self.unit.typedef_alignment_metadata(&ty)?,
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
        self.compatible_at(left, right, 0, &mut TypeComparisonBudget::new())
    }

    /// Typedef redeclarations require the same type, rather than a compatible
    /// incomplete/complete pair. Parameter names and equivalent ABI spellings
    /// do not create distinct function types.
    pub(crate) fn same_type(&self, left: &Type, right: &Type, depth: usize) -> Result<bool, Error> {
        self.same_type_at::<false, false>(left, right, depth, &mut TypeComparisonBudget::new())
    }

    pub(crate) fn same_deduced_type(&self, left: &Type, right: &Type) -> Result<bool, Error> {
        self.same_type_at::<true, false>(left, right, 0, &mut TypeComparisonBudget::new())
    }

    /// Array qualification belongs to the element type for both identity and
    /// compatibility. Exact deduction additionally compares VLA identity.
    fn same_type_at<const EXACT: bool, const ARRAY_ELEMENT: bool>(
        &self,
        left: &Type,
        right: &Type,
        depth: usize,
        budget: &mut TypeComparisonBudget,
    ) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "type identity nesting exceeds the 128-level limit",
            ));
        }
        budget.step()?;
        if !ARRAY_ELEMENT
            && self.identity_qualifiers(left, depth)? != self.identity_qualifiers(right, depth)?
        {
            return Ok(false);
        }
        let left = self.unit.resolve(left)?;
        let right = self.unit.resolve(right)?;
        Ok(match (&left.kind, &right.kind) {
            (TypeKind::Pointer(a), TypeKind::Pointer(b))
            | (TypeKind::Atomic(a), TypeKind::Atomic(b)) => {
                self.same_type_at::<EXACT, false>(a, b, depth + 1, budget)?
            }
            (
                TypeKind::Array {
                    element: a,
                    length: al,
                },
                TypeKind::Array {
                    element: b,
                    length: bl,
                },
            ) => al == bl && self.same_type_at::<EXACT, true>(a, b, depth + 1, budget)?,
            (
                TypeKind::VariableArray {
                    element: a,
                    identity: ai,
                },
                TypeKind::VariableArray {
                    element: b,
                    identity: bi,
                },
            ) => {
                (!EXACT || ai == bi) && self.same_type_at::<EXACT, true>(a, b, depth + 1, budget)?
            }
            (TypeKind::Function(a), TypeKind::Function(b)) => {
                if a.noreturn != b.noreturn
                    || a.prototype != b.prototype
                    || !self
                        .unit
                        .same_parameter_contracts(a.parameter_contracts, b.parameter_contracts)?
                    || a.variadic != b.variadic
                    || a.parameters.len() != b.parameters.len()
                    || a.calling_convention.for_target(self.unit.target)?
                        != b.calling_convention.for_target(self.unit.target)?
                {
                    return Ok(false);
                }
                // GCC ignores ordinary return qualifiers for function identity
                // as well as compatibility. Pointees and atomic wrappers remain.
                let same_return = if self.unit.compiler == Compiler::Gnu {
                    self.same_type_at::<EXACT, false>(
                        &self.unqualified(&a.return_type)?,
                        &self.unqualified(&b.return_type)?,
                        depth + 1,
                        budget,
                    )?
                } else {
                    self.same_type_at::<EXACT, false>(
                        &a.return_type,
                        &b.return_type,
                        depth + 1,
                        budget,
                    )?
                };
                if !same_return {
                    return Ok(false);
                }
                for (a, b) in a.parameters.iter().zip(&b.parameters) {
                    let mut a = self.unit.resolve(&a.ty)?.clone();
                    let mut b = self.unit.resolve(&b.ty)?.clone();
                    a.qualifiers.is_const = false;
                    a.qualifiers.is_volatile = false;
                    a.qualifiers.is_restrict = false;
                    a.qualifiers.set_unaligned(false);
                    b.qualifiers.is_const = false;
                    b.qualifiers.is_volatile = false;
                    b.qualifiers.is_restrict = false;
                    b.qualifiers.set_unaligned(false);
                    if !self.same_type_at::<EXACT, false>(&a, &b, depth + 1, budget)? {
                        return Ok(false);
                    }
                }
                true
            }
            _ => left.kind == right.kind,
        })
    }

    /// Collects qualification across an array chain without changing its stored
    /// typedefs. Pointer pointees begin a separate qualification boundary.
    fn identity_qualifiers<'a>(
        &'a self,
        mut ty: &'a Type,
        depth: usize,
    ) -> Result<Qualifiers, Error> {
        let mut result = self.unit.qualifiers(ty)?;
        let mut levels = depth;
        while let TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } =
            &self.unit.resolve(ty)?.kind
        {
            levels += 1;
            if levels >= 128 {
                return Err(Error::new(
                    0,
                    "type identity nesting exceeds the 128-level limit",
                ));
            }
            ty = element;
            let inner = self.unit.qualifiers(ty)?;
            result.is_const |= inner.is_const;
            result.is_volatile |= inner.is_volatile;
            result.is_restrict |= inner.is_restrict;
            result.set_unaligned(result.is_unaligned() || inner.is_unaligned());
        }
        Ok(result)
    }

    pub(crate) fn compatible_at(
        &self,
        left: &Type,
        right: &Type,
        depth: usize,
        budget: &mut TypeComparisonBudget,
    ) -> Result<bool, Error> {
        self.compatible_array_at(left, right, depth, budget, None)
    }

    /// Compares an array chain's qualifiers once, at its outermost layer.
    fn compatible_array_at(
        &self,
        left: &Type,
        right: &Type,
        depth: usize,
        budget: &mut TypeComparisonBudget,
        array_qualifiers: Option<Qualifiers>,
    ) -> Result<bool, Error> {
        if depth >= 128 {
            return Err(Error::new(
                0,
                "type compatibility nesting exceeds the 128-level limit",
            ));
        }
        budget.step()?;
        let qualifiers = match array_qualifiers {
            Some(qualifiers) => qualifiers,
            None => {
                let qualifiers = self.identity_qualifiers(left, depth)?;
                if qualifiers != self.identity_qualifiers(right, depth)? {
                    return Ok(false);
                }
                qualifiers
            }
        };
        let resolved_left = self.unit.resolve(left)?;
        let resolved_right = self.unit.resolve(right)?;
        match (&resolved_left.kind, &resolved_right.kind) {
            (TypeKind::Enum(id), TypeKind::Integer(kind))
            | (TypeKind::Integer(kind), TypeKind::Enum(id)) => {
                // C11 6.7.2.2 makes an enum compatible with its selected integer
                // type, including inside pointers and function declarations.
                // Distinct enum tags remain distinct types.
                if qualifiers != Qualifiers::default() {
                    return Ok(false);
                }
                if !self.unit.enums[*id].complete {
                    // Only the Microsoft ABI fixes a forward enum's integer
                    // type before its definition.
                    return Ok(self.unit.target.is_windows() && *kind == IntegerKind::Int);
                }
                // Plain char is distinct from equally sized signed/unsigned char.
                Ok(self.unit.enum_integer_kind(*id)? == *kind)
            }
            (TypeKind::Pointer(left), TypeKind::Pointer(right)) => {
                self.compatible_at(left, right, depth + 1, budget)
            }
            (TypeKind::Atomic(left), TypeKind::Atomic(right)) => {
                // GCC treats atomic as enum qualification; Clang compares the
                // contained types. Qualification does not reach through pointers.
                if self.gnu_sync_profile()
                    && matches!(
                        (
                            &self.unit.resolve(left)?.kind,
                            &self.unit.resolve(right)?.kind
                        ),
                        (TypeKind::Enum(_), TypeKind::Integer(_))
                            | (TypeKind::Integer(_), TypeKind::Enum(_))
                    )
                {
                    return Ok(false);
                }
                self.compatible_at(left, right, depth + 1, budget)
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
                && self.compatible_array_at(left, right, depth + 1, budget, Some(qualifiers))?),
            (
                TypeKind::VariableArray { element: left, .. },
                TypeKind::VariableArray { element: right, .. },
            )
            | (
                TypeKind::VariableArray { element: left, .. },
                TypeKind::Array { element: right, .. },
            )
            | (
                TypeKind::Array { element: left, .. },
                TypeKind::VariableArray { element: right, .. },
            ) => self.compatible_array_at(left, right, depth + 1, budget, Some(qualifiers)),
            (TypeKind::Function(left), TypeKind::Function(right)) => {
                if left.calling_convention.for_target(self.unit.target)?
                    != right.calling_convention.for_target(self.unit.target)?
                {
                    return Ok(false);
                }
                if !self.compatible_return_type(
                    &left.return_type,
                    &right.return_type,
                    depth + 1,
                    budget,
                )? {
                    return Ok(false);
                }
                if !left.prototype || !right.prototype {
                    let prototype = if left.prototype { left } else { right };
                    if prototype.variadic {
                        return Ok(false);
                    }
                    for parameter in &prototype.parameters {
                        let parameter_type = self.unit.resolve(&parameter.ty)?;
                        // GCC promotes an atomic parameter's contained value in
                        // a call without a prototype; Clang preserves its type.
                        let parameter_type = match &parameter_type.kind {
                            TypeKind::Atomic(value) if self.gnu_sync_profile() => {
                                self.unit.resolve(value)?
                            }
                            _ => parameter_type,
                        };
                        if (matches!(parameter_type.kind, TypeKind::Enum(_))
                            && self.integer_type(parameter_type, 0)?.rank < 3)
                            || matches!(
                                parameter_type.kind,
                                TypeKind::Bool
                                    | TypeKind::Integer(
                                        IntegerKind::Char
                                            | IntegerKind::SignedChar
                                            | IntegerKind::UnsignedChar
                                            | IntegerKind::Short
                                            | IntegerKind::UnsignedShort
                                    )
                                    | TypeKind::Float(FloatKind::Float)
                            )
                        {
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
                    a.qualifiers.is_const = false;
                    a.qualifiers.is_volatile = false;
                    a.qualifiers.is_restrict = false;
                    a.qualifiers.set_unaligned(false);
                    let mut b = self.unit.resolve(&b.ty)?.clone();
                    b.qualifiers.is_const = false;
                    b.qualifiers.is_volatile = false;
                    b.qualifiers.is_restrict = false;
                    b.qualifiers.set_unaligned(false);
                    if !self.compatible_parameter_at(&a, &b, depth + 1, budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(resolved_left.kind == resolved_right.kind),
        }
    }

    pub(crate) fn align_typedef(
        &mut self,
        ty: &mut Type,
        specifiers: &[Node<ast::DeclarationSpecifier>],
        attributes: &Attributes,
        extra: &Attributes,
        offset: usize,
        name: &str,
    ) -> Result<(), Error> {
        if specifiers
            .iter()
            .any(|s| matches!(s.node, ast::DeclarationSpecifier::Alignment(_)))
        {
            return Err(Error::new(
                offset,
                "an alignment specifier is not permitted on a typedef",
            ));
        }
        let inherited = self.unit.typedef_alignment_metadata(ty)?;
        let previous = match self.lexical_scopes.last() {
            Some(scope) => scope.typedefs.get(name),
            None => self.unit.typedefs.get(name),
        }
        .map(|ty| self.unit.typedef_alignment_metadata(ty))
        .transpose()?;
        let explicit = attributes.vendor_alignment().max(extra.vendor_alignment());
        ty.alignment = inherited;
        if let Some(alignment) = explicit {
            let non_object = matches!(
                self.unit.resolve(ty)?.kind,
                TypeKind::Void | TypeKind::Function(_)
            );
            if non_object
                && attributes
                    .msvc_alignment
                    .max(extra.msvc_alignment)
                    .is_some()
            {
                return Err(Error::new(
                    offset,
                    "Microsoft alignment on void or function typedefs is unsupported",
                ));
            }
            if !non_object || self.unit.compiler == Compiler::Clang {
                ty.alignment =
                    crate::TypeAlignment::new(u32::try_from(alignment).map_err(|_| {
                        Error::new(offset, "typedef alignment exceeds the supported range")
                    })?)
                    .ok_or_else(|| {
                        Error::new(offset, "typedef alignment exceeds the supported range")
                    })?;
            }
        }
        if let Some(previous) = previous {
            ty.alignment =
                crate::TypeAlignment::from_bytes(previous.bytes().max(ty.alignment.bytes()))?;
        }
        self.retain_typedef_alignment(name, ty, previous, inherited, explicit.is_some(), offset)
    }

    /// Prepares a declaration with its standalone-tag context still available.
    pub(crate) fn declaration_specifiers(
        &mut self,
        declaration: &ast::Declaration,
    ) -> Result<(Type, Attributes), Error> {
        let prepared = self.prepare_specifiers_context(
            &declaration.specifiers,
            false,
            declaration.declarators.is_empty(),
        )?;
        let result = self.complete_specifiers(&declaration.specifiers, prepared, None)?;
        if declaration.declarators.is_empty() {
            self.note_standalone_lexical_tag(&result.0.kind);
        }
        if declaration.declarators.is_empty()
            && (self.declaration_origins.is_some() || self.documentation_origins.is_some())
        {
            for specifier in &declaration.specifiers {
                if let ast::DeclarationSpecifier::TypeSpecifier(ty) = &specifier.node {
                    let identifier = match &ty.node {
                        ast::TypeSpecifier::Struct(tag) if tag.node.declarations.is_none() => {
                            tag.node.identifier.as_ref()
                        }
                        ast::TypeSpecifier::Enum(tag) if tag.node.enumerators.is_empty() => {
                            tag.node.identifier.as_ref()
                        }
                        _ => None,
                    };
                    if let Some(identifier) = identifier {
                        if let Some(origins) = &mut self.declaration_origins {
                            origins.standalone_tag(identifier.span);
                        }
                        if let Some(origins) = &mut self.documentation_origins {
                            origins.standalone_tag(identifier.span.start);
                        }
                    }
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn specifiers(
        &mut self,
        specifiers: &[Node<ast::DeclarationSpecifier>],
    ) -> Result<(Type, Attributes), Error> {
        let prepared = self.prepare_specifiers(specifiers)?;
        self.complete_specifiers(specifiers, prepared, None)
    }

    pub(crate) fn prepare_specifiers(
        &mut self,
        specifiers: &[Node<ast::DeclarationSpecifier>],
    ) -> Result<PreparedSpecifiers, Error> {
        self.prepare_specifiers_context(specifiers, false, false)
    }

    fn prepare_specifiers_context(
        &mut self,
        specifiers: &[Node<ast::DeclarationSpecifier>],
        type_name: bool,
        tag_only: bool,
    ) -> Result<PreparedSpecifiers, Error> {
        let mut types = Vec::new();
        let mut qualifiers = Qualifiers::default();
        let mut atomic = false;
        let mut attributes = Attributes {
            target_type_name: type_name,
            ..Attributes::default()
        };
        let mut record_attributes = Attributes::default();
        let mut after_tag_definition = false;
        let tag_definition = || {
            specifiers
                .iter()
                .find_map(|specifier| match &specifier.node {
                    ast::DeclarationSpecifier::TypeSpecifier(ty) => match &ty.node {
                        ast::TypeSpecifier::Struct(tag) if tag.node.declarations.is_some() => {
                            Some(ty.span.start)
                        }
                        ast::TypeSpecifier::Enum(tag) if !tag.node.enumerators.is_empty() => {
                            Some(ty.span.start)
                        }
                        _ => None,
                    },
                    _ => None,
                })
        };
        for specifier in specifiers {
            match &specifier.node {
                ast::DeclarationSpecifier::TypeSpecifier(ty) => {
                    after_tag_definition = matches!(&ty.node, ast::TypeSpecifier::Struct(record) if record.node.declarations.is_some())
                        || matches!(&ty.node, ast::TypeSpecifier::Enum(enumeration) if !enumeration.node.enumerators.is_empty());
                    match &ty.node {
                        ast::TypeSpecifier::Struct(tag) => {
                            self.attributes(&tag.node.extensions, &mut record_attributes)?
                        }
                        ast::TypeSpecifier::Enum(tag) => {
                            self.attributes(&tag.node.extensions, &mut record_attributes)?
                        }
                        _ => {}
                    }
                    types.push(ty.clone());
                }
                ast::DeclarationSpecifier::TypeQualifier(qualifier) => {
                    add_qualifier(&mut qualifiers, &mut atomic, qualifier)?
                }
                ast::DeclarationSpecifier::Extension(extensions) => {
                    let first_record = self.unit.records.len();
                    let first_enum = self.unit.enums.len();
                    for extension in extensions {
                        if let ast::Extension::Declspec(attribute) = &extension.node {
                            if type_name {
                                return Err(Error::new(
                                    extension.span.start,
                                    "__declspec is not permitted directly in a type name",
                                ));
                            }
                            // Microsoft prefix alignment belongs to a following tag
                            // definition, but the same spelling after `}` belongs to
                            // the declarator. Other prefix attributes stay on the
                            // object or function declared alongside a tag definition.
                            let tag_alignment = attribute.name.node == "align"
                                && (tag_only
                                    || tag_definition()
                                        .is_some_and(|start| extension.span.start < start));
                            if tag_alignment {
                                self.declspec_attribute(
                                    attribute,
                                    extension.span,
                                    &mut record_attributes,
                                    false,
                                )?;
                            } else {
                                self.declspec_attribute(
                                    attribute,
                                    extension.span,
                                    &mut attributes,
                                    tag_only,
                                )?;
                            }
                        } else if after_tag_definition {
                            self.attributes(
                                std::slice::from_ref(extension),
                                &mut record_attributes,
                            )?;
                        } else {
                            self.attributes(std::slice::from_ref(extension), &mut attributes)?;
                        }
                    }
                    // Trailing tag attributes are evaluated before the tag
                    // is built. Their definitions need cursor ownership or
                    // visibility facts without changing actual C scope.
                    if types.last().is_some_and(|ty| {
                        matches!(&ty.node, ast::TypeSpecifier::Struct(record) if record.node.declarations.is_some())
                            || matches!(&ty.node, ast::TypeSpecifier::Enum(enumeration) if !enumeration.node.enumerators.is_empty())
                    }) && (self.unit.enums[first_enum..]
                        .iter()
                        .any(|enumeration| enumeration.scope == Scope::File)
                        || self.unit.records[first_record..]
                            .iter()
                            .any(|record| record.scope == Scope::File))
                    {
                        self.needs_tag_discovery = true;
                    }
                }
                ast::DeclarationSpecifier::Function(specifier)
                    if specifier.node == ast::FunctionSpecifier::Noreturn =>
                {
                    attributes.noreturn = Some(specifier.span);
                    attributes.c11_noreturn = Some(specifier.span);
                }
                ast::DeclarationSpecifier::Function(specifier) => {
                    attributes.inline_specifier =
                        Some((specifier.span, self.unit.compiler == Compiler::Clang));
                }
                ast::DeclarationSpecifier::Alignment(alignment) => {
                    let value = self.alignment_operand(|analyzer| match &alignment.node {
                        ast::AlignmentSpecifier::Type(ty) => {
                            let ty = analyzer.type_name(&ty.node)?;
                            analyzer.unit.alignment(&ty)
                        }
                        ast::AlignmentSpecifier::Constant(expression) => {
                            analyzer.eval(expression)?.as_u64()
                        }
                    })?;
                    let mut checked_alignment = Attributes::default();
                    set_alignment(&mut checked_alignment, value, alignment.span.start)?;
                    attributes.c11_alignment =
                        Some(attributes.c11_alignment.unwrap_or(0).max(value));
                }
                _ => {}
            }
        }
        if let Some(checked) = &mut self.checked {
            checked.begin_specifier_operands(&types)?;
        }
        Ok(PreparedSpecifiers {
            types,
            qualifiers,
            atomic,
            attributes,
            record_attributes,
        })
    }

    pub(crate) fn complete_specifiers(
        &mut self,
        specifiers: &[Node<ast::DeclarationSpecifier>],
        prepared: PreparedSpecifiers,
        inference: Option<&crate::auto_type::AutoInference<'_>>,
    ) -> Result<(Type, Attributes), Error> {
        let PreparedSpecifiers {
            types,
            qualifiers,
            atomic,
            mut attributes,
            record_attributes,
        } = prepared;
        record_attributes.require_function_attributes(false)?;
        record_attributes.require_no_weak()?;
        if record_attributes.vector_size.is_some() {
            return Err(Error::new(
                types.first().map_or(0, |ty| ty.span.start),
                "vector_size attached to a record or enum tag is unsupported",
            ));
        }
        let mut ty = match inference {
            Some(inference) => inference.ty.clone(),
            None => self.base_type(&types)?,
        };
        if let Some(checked) = &mut self.checked {
            for specifier in &types {
                if let ast::TypeSpecifier::TypedefName(name) = &specifier.node {
                    checked.typedef_reference(name)?;
                }
            }
        }
        let origin = super::transparent_union::typedef_origin(&types);
        attributes.typedef_base = origin == Some(true);
        attributes.unknown_typedef_origin = origin.is_none();
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
        let clang_forward = self.unit.compiler == Compiler::Clang
            && (record_attributes.packed
                || record_attributes.vendor_alignment().is_some()
                || record_attributes.transparent_union.is_some())
            && match self.unit.resolve(&ty)?.kind {
                TypeKind::Record(id) => self.unit.records[id].fields.is_none(),
                TypeKind::Enum(id) => !self.unit.enums[id].complete,
                _ => false,
            };
        // GCC ignores layout attributes on forward tags; Clang retains them.
        // Both ignore a new attribute applied after the tag is already defined.
        if defines_tag || clang_forward {
            self.apply_tag_attributes(
                &ty,
                &record_attributes,
                types.first().map_or(0, |ty| ty.span.start),
            )?;
        }
        let atomic_wrapper = atomic && self.unit.atomic_value(&ty)?.is_none();
        if atomic {
            ty = self.atomic_type(ty, false, specifiers.first().map_or(0, |s| s.span.start))?;
        }
        ty.qualifiers.is_const |= qualifiers.is_const;
        ty.qualifiers.is_volatile |= qualifiers.is_volatile;
        ty.qualifiers.is_restrict |= qualifiers.is_restrict;
        ty.qualifiers
            .set_unaligned(ty.qualifiers.is_unaligned() || qualifiers.is_unaligned());
        if let Some(mode) = &attributes.mode {
            ty = self.machine_mode(ty, mode, types.first().map_or(0, |ty| ty.span.start))?;
        }
        if let Some(bytes) = attributes.vector_size {
            ty = self.vector_type(ty, bytes, types.first().map_or(0, |ty| ty.span.start))?;
        }
        let offset = specifiers.first().map_or(0, |item| item.span.start);
        // Clang applies written qualifiers after deduction, including restrict
        // on a non-pointer inferred type. Ordinary C declarations still reject it.
        if inference.is_none() || self.gnu_sync_profile() {
            self.check_restrict(&ty, offset)?;
        }
        if let Some(checked) = &mut self.checked {
            let variably_modified = self.unit.is_variably_modified(&ty)?;
            let definition_parameter = self
                .lexical_scopes
                .last()
                .is_some_and(|scope| scope.is_definition_parameters);
            attributes.type_use = Some(if let Some(id) = inference.and_then(|i| i.type_use) {
                if atomic_wrapper {
                    checked.wrap_type_use(
                        id,
                        &ty,
                        crate::checked::TypeStep::AtomicValue,
                        None,
                        offset,
                    )?
                } else {
                    checked.retype_use(id, &ty, offset)?
                }
            } else {
                checked.base_type_use(
                    &types,
                    &ty,
                    offset,
                    variably_modified,
                    definition_parameter,
                    atomic_wrapper,
                )?
            });
        }
        Ok((ty, attributes))
    }

    fn specifier_qualifiers(
        &mut self,
        specifiers: &[Node<ast::SpecifierQualifier>],
        type_name: bool,
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
                        ast::SpecifierQualifier::Alignment(alignment) => {
                            ast::DeclarationSpecifier::Alignment(alignment.clone())
                        }
                        ast::SpecifierQualifier::Extension(extensions) => {
                            ast::DeclarationSpecifier::Extension(extensions.clone())
                        }
                    },
                    specifier.span,
                )
            })
            .collect::<Vec<_>>();
        let mut prepared = self.prepare_specifiers_context(&specifiers, type_name, false)?;
        // Clang ignores declaration mode attributes in a type name. GNU applies
        // them to the completed abstract declarator.
        if type_name && self.unit.compiler == Compiler::Clang {
            prepared.attributes.mode = None;
        }
        self.complete_specifiers(&specifiers, prepared, None)
    }

    fn base_type(&mut self, types: &[Node<ast::TypeSpecifier>]) -> Result<Type, Error> {
        for ty in types {
            if types.len() != 1 && matches!(ty.node, ast::TypeSpecifier::TypeOf(_)) {
                return Err(Error::new(
                    ty.span.start,
                    "typeof cannot be combined with other type specifiers",
                ));
            }
            if let ast::TypeSpecifier::TypedefName(name) = &ty.node {
                self.check_auto_reference(&name.node.name, name.span.start)?;
            }
            if matches!(ty.node, ast::TypeSpecifier::AutoType) {
                return Err(Error::new(
                    ty.span.start,
                    "__auto_type is only permitted in an initialized object declaration",
                ));
            }
        }
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
        let mut complex = false;
        let mut int = false;
        let mut int128 = false;
        let mut special = None;
        let mut direct_complex_base = false;
        let msvc_short = types
            .iter()
            .any(|ty| matches!(ty.node, ast::TypeSpecifier::MsvcInteger(16)));
        for ty in types {
            match &ty.node {
                ast::TypeSpecifier::Int128 => {
                    if matches!(
                        self.unit.target,
                        Target::I686UnknownLinuxGnu | Target::Armv7UnknownLinuxGnueabihf
                    ) {
                        return Err(Error::new(
                            ty.span.start,
                            if self.unit.target.is_armv7() {
                                "__int128 is unavailable on ARMv7 GNU Linux"
                            } else {
                                "__int128 is unavailable on i686 GNU Linux"
                            },
                        ));
                    }
                    if std::mem::replace(&mut int128, true) {
                        return Err(Error::new(
                            ty.span.start,
                            "duplicate __int128 type specifier",
                        ));
                    }
                }
                ast::TypeSpecifier::MsvcInteger(8) if !char_ => char_ = true,
                ast::TypeSpecifier::MsvcInteger(16) => short = true,
                ast::TypeSpecifier::MsvcInteger(32) if !int => int = true,
                ast::TypeSpecifier::MsvcInteger(64) => long = 2,
                ast::TypeSpecifier::Long => {
                    if long >= 2 {
                        return Err(Error::new(ty.span.start, "too many long type specifiers"));
                    }
                    long += 1;
                }
                ast::TypeSpecifier::Short if !short || msvc_short => short = true,
                ast::TypeSpecifier::Signed if !signed => signed = true,
                ast::TypeSpecifier::Unsigned if !unsigned => unsigned = true,
                ast::TypeSpecifier::Char if !char_ => char_ = true,
                ast::TypeSpecifier::Float if !float => float = true,
                ast::TypeSpecifier::Double if !double => double = true,
                ast::TypeSpecifier::Int if !int => int = true,
                ast::TypeSpecifier::Complex if !complex => complex = true,
                value => {
                    let kind = match value {
                        ast::TypeSpecifier::Void => TypeKind::Void,
                        ast::TypeSpecifier::Bool => TypeKind::Bool,
                        ast::TypeSpecifier::TypedefName(name) => {
                            if !self.unit.typedefs.contains_key(&name.node.name)
                                && let Some(kind) = crate::wide_float::predefined_type(
                                    &name.node.name,
                                    self.unit.target,
                                    self.unit.compiler,
                                )
                            {
                                if types.len() != 1 {
                                    return Err(Error::new(
                                        ty.span.start,
                                        "invalid modifiers on typedef type",
                                    ));
                                }
                                return Ok(Type::new(TypeKind::Float(kind)));
                            }
                            if !self.unit.typedefs.contains_key(&name.node.name)
                                && let Some(builtin) = crate::arm::builtin_type(
                                    &name.node.name,
                                    self.unit.target,
                                    self.unit.compiler,
                                )
                            {
                                self.unit.typedefs.insert(name.node.name.clone(), builtin);
                            }
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
                            ast::TypeOf::Type(ty) => {
                                let checkpoint = self.sve_feature_checkpoint();
                                let allocation_context = self.allocation_context(false);
                                let ty = self.type_name(&ty.node);
                                let mut ty = self.finish_allocation_operand(
                                    allocation_context,
                                    ty,
                                    |analyzer, ty| analyzer.unit.is_variably_modified(ty),
                                )?;
                                if !self.unit.is_variably_modified(&ty)? {
                                    self.discard_sve_feature_uses(checkpoint);
                                }
                                self.retain_typeof_alignment(&mut ty, false, value.span.start)?;
                                return Ok(ty);
                            }
                            ast::TypeOf::Expression(expression) => {
                                let dependency_expression = self.parameter_type_dependencies.as_mut().map(|dependencies| {
                                    dependencies.begin_type_expression(crate::parameter_dependencies::type_expression_identifier(expression))
                                });
                                let checkpoint = self.sve_feature_checkpoint();
                                let allocation_context = self.allocation_context(false);
                                let ty = self.expression_type(expression);
                                let mut ty = self.finish_allocation_operand(
                                    allocation_context,
                                    ty,
                                    |analyzer, ty| analyzer.unit.is_variably_modified(ty),
                                )?;
                                if !self.unit.is_variably_modified(&ty)? {
                                    self.discard_sve_feature_uses(checkpoint);
                                }
                                if let (Some(dependencies), Some(previous)) =
                                    (&mut self.parameter_type_dependencies, dependency_expression)
                                {
                                    dependencies.finish_type_expression(
                                        previous,
                                        matches!(
                                            self.unit.resolve(&ty)?.kind,
                                            TypeKind::Function(_)
                                        ),
                                    )?;
                                }
                                self.retain_typeof_alignment(&mut ty, true, value.span.start)?;
                                return Ok(ty);
                            }
                        },
                        ast::TypeSpecifier::Atomic(name) => {
                            let inner = self.type_name(&name.node)?;
                            self.atomic_type(inner, true, ty.span.start)?.kind
                        }
                        ast::TypeSpecifier::BFloat16 => {
                            if self.unit.target == toucan_target::Target::I686UnknownLinuxGnu {
                                return Err(Error::new(
                                    ty.span.start,
                                    "__bf16 is unavailable on i686 GNU Linux",
                                ));
                            }
                            TypeKind::Float(FloatKind::BFloat16)
                        }
                        ast::TypeSpecifier::Float128 => {
                            if !matches!(
                                self.unit.target,
                                toucan_target::Target::I686UnknownLinuxGnu
                                    | toucan_target::Target::X86_64UnknownLinuxGnu
                                    | toucan_target::Target::X86_64UnknownLinuxMusl
                            ) {
                                return Err(Error::new(
                                    ty.span.start,
                                    "__float128 spelling is unavailable in this Clang target profile",
                                ));
                            }
                            direct_complex_base = true;
                            TypeKind::Float(FloatKind::FLOAT128)
                        }
                        ast::TypeSpecifier::TS18661Float(float) => {
                            if self.unit.target == toucan_target::Target::I686UnknownLinuxGnu
                                && float.format == ast::TS18661FloatFormat::BinaryInterchange
                                && float.width == 16
                            {
                                return Err(Error::new(
                                    ty.span.start,
                                    "_Float16 is unavailable on i686 GNU Linux",
                                ));
                            }
                            if matches!(
                                float.format,
                                ast::TS18661FloatFormat::BinaryInterchange
                                    | ast::TS18661FloatFormat::BinaryExtended
                            ) && matches!(float.width, 32 | 64)
                            {
                                if self.unit.compiler != Compiler::Gnu {
                                    return Err(Error::new(
                                        ty.span.start,
                                        "GNU _Float32/_Float64/_Float32x/_Float64x types are unavailable in the Clang profile",
                                    ));
                                }
                                direct_complex_base = true;
                            }
                            if float.format == ast::TS18661FloatFormat::BinaryInterchange
                                && float.width == 128
                            {
                                if self.unit.compiler != toucan_target::Compiler::Gnu {
                                    return Err(Error::new(
                                        ty.span.start,
                                        "the Clang profile rejects the _Float128 type spelling",
                                    ));
                                }
                                direct_complex_base = true;
                            }
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
            if complex
                && direct_complex_base
                && !float
                && !double
                && !char_
                && !signed
                && !unsigned
                && !short
                && long == 0
                && !int
                && !int128
            {
                let TypeKind::Float(kind) = special else {
                    unreachable!()
                };
                return Ok(Type::new(TypeKind::Complex(kind)));
            }
            if long > 0
                || short
                || signed
                || unsigned
                || char_
                || float
                || double
                || int
                || int128
                || complex
            {
                return Err(Error::new(offset, "invalid modifiers on type"));
            }
            let mut ty = Type::new(special);
            if !self.unit.alignment_origins.is_empty() && matches!(ty.kind, TypeKind::Typedef(_)) {
                ty.alignment = self.unit.typedef_alignment_metadata(&ty)?;
            }
            return Ok(ty);
        }
        if (types.is_empty() && !self.unit.language_mode.is_c90())
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
        if complex {
            if !float && !double {
                return Err(Error::new(
                    offset,
                    "complex types require float, double, or long double; GNU integer and implicit complex types are unsupported",
                ));
            }
            return Ok(Type::new(TypeKind::Complex(if float {
                FloatKind::Float
            } else if long == 1 {
                FloatKind::LongDouble
            } else {
                FloatKind::Double
            })));
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
        self.with_frontend_folding(|analyzer| analyzer.type_name_inner(name))
    }

    fn type_name_inner(&mut self, name: &ast::TypeName) -> Result<Type, Error> {
        let start = name.specifiers.first().map_or(0, |node| node.span.start);
        let end = name.declarator.as_ref().map_or_else(
            || name.specifiers.last().map_or(start, |node| node.span.end),
            |node| node.span.end,
        );
        let key = (start, end);
        if let Some(ty) = self.type_names.get(&key) {
            return Ok(ty.clone());
        }
        let (ty, mut attributes) = self.specifier_qualifiers(&name.specifiers, true)?;
        attributes.target_type_name = true;
        attributes.require_function_attributes(false)?;
        attributes.require_no_weak()?;
        attributes.require_no_transparent_union()?;
        if attributes.c11_alignment.is_some() {
            return Err(Error::new(
                start,
                "_Alignas is not permitted in a type name",
            ));
        }
        attributes.type_name_use = true;
        let (ty, type_use) = if let Some(declarator) = &name.declarator {
            let (_, ty, extra) = self.declarator(ty, declarator, &attributes)?;
            extra.require_function_attributes(false)?;
            extra.require_no_weak()?;
            extra.require_no_transparent_union()?;
            (ty, extra.type_use)
        } else {
            (
                self.apply_calling_convention(ty, &attributes, start)?,
                attributes.type_use,
            )
        };
        if let (Some(checked), Some(type_use)) = (&mut self.checked, type_use) {
            checked.save_type_name_use(name, type_use)?;
        }
        self.type_names.insert(key, ty.clone());
        Ok(ty)
    }

    pub(crate) fn declarator(
        &mut self,
        ty: Type,
        declaration: &Node<ast::Declarator>,
        attributes: &Attributes,
    ) -> Result<(Option<String>, Type, Attributes), Error> {
        self.declarator_with_definition(ty, declaration, attributes, None)
    }

    fn declarator_with_definition(
        &mut self,
        ty: Type,
        declaration: &Node<ast::Declarator>,
        attributes: &Attributes,
        definition: Option<&Node<ast::FunctionDefinition>>,
    ) -> Result<(Option<String>, Type, Attributes), Error> {
        let alias_base = attributes.alias_base && !has_function_derivation(declaration);
        let mut prepared = self
            .prepare_declarator_attributes(declaration, alias_base, attributes.type_name_use)?
            .into_iter();
        let (name, ty, mut extra) = self.declarator_at(
            ty,
            declaration,
            DeclaratorContext {
                parameter_array: None,
                parenthesized: false,
                alias_base,
                base_use: attributes.type_use,
                type_name: attributes.type_name_use,
                definition,
            },
            &mut prepared,
        )?;
        debug_assert!(prepared.as_slice().is_empty());
        if let Some(mode) = &attributes.mode {
            self.floating_machine_mode(&ty, mode, declaration.span.start)?;
        }
        extra.target_type_name |= attributes.target_type_name;
        extra
            .target_attributes
            .extend(attributes.target_attributes.iter().cloned());
        extra
            .minimum_vector_width
            .extend_from_slice(&attributes.minimum_vector_width);
        extra.always_inline =
            crate::target_features::merge_inline(extra.always_inline, attributes.always_inline);
        extra.no_inline =
            crate::target_features::merge_inline(extra.no_inline, attributes.no_inline);
        extra.gnu_inline =
            crate::target_features::merge_inline(extra.gnu_inline, attributes.gnu_inline);
        extra.inline_specifier = extra.inline_specifier.or(attributes.inline_specifier);
        extra.nodebug_arguments = extra.nodebug_arguments.or(attributes.nodebug_arguments);
        if name.is_some() {
            self.check_nodebug_function_like(&ty, &extra)?;
        }
        crate::dll_storage::merge_attributes(
            &mut extra.dll_storage,
            attributes.dll_storage.as_deref(),
        );
        extra.weak = extra.weak.or(attributes.weak);
        extra.returns_twice = extra.returns_twice.or(attributes.returns_twice);
        extra.noreturn = extra.noreturn.or(attributes.noreturn);
        extra.c11_noreturn = extra.c11_noreturn.or(attributes.c11_noreturn);
        extra.type_noreturn |= attributes.type_noreturn;
        extra.transparent_union = extra.transparent_union.or(attributes.transparent_union);
        if !attributes.diagnostic_attributes.is_empty() {
            let mut diagnostic_attributes = attributes.diagnostic_attributes.clone();
            diagnostic_attributes.append(&mut extra.diagnostic_attributes);
            extra.diagnostic_attributes = diagnostic_attributes;
        }
        if attributes.calling_convention.is_none() && !attributes.type_noreturn {
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

    pub(crate) fn check_parameter(
        &mut self,
        parameter: crate::parameters::ParameterSyntax<'_>,
        prepared: Option<&(Type, Attributes)>,
    ) -> Result<(Option<crate::checked::SiteId>, bool), Error> {
        let mut site = None;
        let storage = storage_specifiers(parameter.specifiers(), self.unit.compiler)?;
        if storage.thread_local
            || !matches!(
                storage.class,
                None | Some(ast::StorageClassSpecifier::Register)
            )
        {
            return Err(Error::new(
                parameter.span().start,
                "only register storage is permitted for a parameter",
            ));
        }
        if parameter
            .specifiers()
            .iter()
            .any(|specifier| matches!(specifier.node, ast::DeclarationSpecifier::Alignment(_)))
        {
            return Err(Error::new(
                parameter.span().start,
                "alignment is not permitted on a parameter",
            ));
        }
        let (base, mut attributes) = match prepared {
            Some(prepared) => prepared.clone(),
            None => self.specifiers(parameter.specifiers())?,
        };
        attributes.require_function_attributes(false)?;
        attributes.require_no_weak()?;
        attributes.require_no_transparent_union()?;
        attributes.alias_base =
            attributes.alias_base && !parameter.declarator().is_some_and(has_function_derivation);
        let mut array_qualifiers = Qualifiers::default();
        let mut array_atomic = false;
        let (name, mut parameter_type, mut declared_type_use) = if let Some(declarator) =
            parameter.declarator()
        {
            let array = outermost_derived(declarator).and_then(|derived| {
                if let ast::DerivedDeclarator::Array(array) = &derived.node {
                    Some((derived.span.start, array))
                } else {
                    None
                }
            });
            if let Some((_, array)) = array {
                for qualifier in &array.node.qualifiers {
                    add_qualifier(&mut array_qualifiers, &mut array_atomic, qualifier)?;
                }
            }
            let dependency_cursor = self.parameter_type_dependencies.as_mut().map(|deps| {
                deps.parameter_cursor(declarator_name_span(declarator).unwrap_or(parameter.span()))
            });
            let mut prepared = self
                .prepare_declarator_attributes(
                    declarator,
                    attributes.alias_base,
                    attributes.type_name_use,
                )?
                .into_iter();
            let result = self.declarator_at(
                base,
                declarator,
                DeclaratorContext {
                    parameter_array: array.map(|(offset, _)| offset),
                    parenthesized: false,
                    alias_base: attributes.alias_base,
                    base_use: attributes.type_use,
                    type_name: attributes.type_name_use,
                    definition: None,
                },
                &mut prepared,
            );
            debug_assert!(result.is_err() || prepared.as_slice().is_empty());
            if let (Some(deps), Some(previous)) =
                (&mut self.parameter_type_dependencies, dependency_cursor)
            {
                deps.restore_cursor(previous);
            }
            let (name, ty, extra) = result?;
            self.check_nodebug_function_like(&ty, &extra)?;
            extra.require_function_attributes(false)?;
            extra.require_no_weak()?;
            extra.require_no_transparent_union()?;
            attributes.alignment = attributes.alignment.max(extra.alignment);
            attributes.msvc_alignment = attributes.msvc_alignment.max(extra.msvc_alignment);
            attributes.noescape.extend(extra.noescape);
            (name, ty, extra.type_use)
        } else {
            (
                parameter
                    .implicit_identifier()
                    .map(|identifier| identifier.node.name.clone()),
                base,
                attributes.type_use,
            )
        };
        if let Some(mode) = &attributes.mode {
            self.floating_machine_mode(&parameter_type, mode, parameter.span().start)?;
        }
        parameter_type =
            self.apply_calling_convention(parameter_type, &attributes, parameter.span().start)?;
        let mut extra = Attributes::default();
        self.attributes(parameter.extensions(), &mut extra)?;
        self.check_nodebug_function_like(&parameter_type, &attributes)?;
        self.check_nodebug_function_like(&parameter_type, &extra)?;
        crate::dll_storage::check_arguments(attributes.dll_storage.as_deref())?;
        crate::dll_storage::check_arguments(extra.dll_storage.as_deref())?;
        extra.require_function_attributes(false)?;
        extra.require_no_weak()?;
        extra.require_no_transparent_union()?;
        if let Some(mode) = &extra.mode {
            parameter_type = self.machine_mode(parameter_type, mode, parameter.span().start)?;
            if let (Some(checked), Some(id)) = (&mut self.checked, declared_type_use) {
                declared_type_use =
                    Some(checked.retype_use(id, &parameter_type, parameter.span().start)?);
            }
        }
        if let Some(bytes) = extra.vector_size {
            parameter_type = self.vector_type(parameter_type, bytes, parameter.span().start)?;
        }
        parameter_type =
            self.apply_calling_convention(parameter_type, &extra, parameter.span().start)?;
        let parameter_alignment = self.check_declaration_alignment(
            &parameter_type,
            &attributes,
            &extra,
            crate::object_alignment::AlignmentSubject::Parameter,
            parameter.span().start,
        )?;
        let qualifiers = self.unit.qualifiers(&parameter_type)?;
        if matches!(self.unit.resolve(&parameter_type)?.kind, TypeKind::Void)
            && (qualifiers != Qualifiers::default()
                || (storage.class.is_some() && self.unit.compiler == toucan_target::Compiler::Gnu))
        {
            return Err(Error::new(
                parameter.span().start,
                "void parameter must be unqualified",
            ));
        }
        if let (Some(checked), Some(id)) = (&mut self.checked, declared_type_use) {
            parameter.retain_written_type(
                checked,
                id,
                &self.unit.resolve(&parameter_type)?.kind,
            )?;
        }
        if let Some(deps) = &mut self.parameter_type_dependencies
            && let TypeKind::Typedef(name) = &parameter_type.kind
            && matches!(
                self.unit.resolve(&parameter_type)?.kind,
                TypeKind::Array { .. } | TypeKind::VariableArray { .. }
            )
        {
            deps.alias(name)?;
        }
        parameter_type = match &self.unit.resolve(&parameter_type)?.kind {
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } => {
                // Qualifying an array typedef qualifies its elements.
                // Parameter adjustment removes only the array layer.
                let mut element = (**element).clone();
                element.qualifiers.is_const |= qualifiers.is_const;
                element.qualifiers.is_volatile |= qualifiers.is_volatile;
                element.qualifiers.is_restrict |= qualifiers.is_restrict;
                element
                    .qualifiers
                    .set_unaligned(element.qualifiers.is_unaligned() || qualifiers.is_unaligned());
                let mut pointer = element.pointer();
                pointer.qualifiers = array_qualifiers;
                if array_atomic && self.gnu_sync_profile() {
                    self.atomic_type(pointer, false, parameter.span().start)?
                } else {
                    pointer
                }
            }
            TypeKind::Function(_) => parameter_type.pointer(),
            _ => parameter_type,
        };
        let noescape =
            self.check_noescape_parameter(&parameter_type, &attributes.noescape, &extra.noescape)?;
        if let Some(checked) = &mut self.checked
            && (name.is_some()
                || !matches!(self.unit.resolve(&parameter_type)?.kind, TypeKind::Void))
        {
            site = parameter.retain_declaration(
                checked,
                LocalDeclaration {
                    name: name.as_deref(),
                    name_span: parameter
                        .declarator()
                        .and_then(declarator_name_span)
                        .or_else(|| {
                            parameter
                                .implicit_identifier()
                                .map(|identifier| identifier.span)
                        }),
                    ty: &parameter_type,
                    kind: EntityKind::Parameter,
                    storage: Storage::Automatic,
                    linked: false,
                    register: storage.class == Some(ast::StorageClassSpecifier::Register),
                    definition: false,
                    allocation: None,
                },
            )?;
            if let Some(site) = site {
                checked.attach_alignment(site, parameter_alignment, parameter_alignment)?;
                checked.attach_noescape_parameter(
                    site,
                    &attributes.noescape,
                    &extra.noescape,
                    noescape,
                )?;
            }
        }
        if let Some(name) = &name {
            self.retain_local_alignment(name, parameter_alignment);
        }
        let scope = self
            .lexical_scopes
            .last_mut()
            .expect("prototype scope is active");
        if let Some(name) = &name {
            if parameter.specifiers().iter().any(|specifier| matches!(&specifier.node, ast::DeclarationSpecifier::StorageClass(storage) if storage.node == ast::StorageClassSpecifier::Register)) {
                scope.register.insert(name.clone());
            }
            if scope
                .names
                .insert(name.clone(), Some(scope.parameters.len()))
                .is_some()
            {
                return Err(Error::new(
                    parameter.span().start,
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
        Ok((site, noescape))
    }

    /// Evaluates prefix and pointer-qualifier operands in lexical order before
    /// type construction visits enclosing array and function suffixes.
    fn prepare_declarator_attributes(
        &mut self,
        declaration: &Node<ast::Declarator>,
        alias_base: bool,
        type_name: bool,
    ) -> Result<Vec<Attributes>, Error> {
        if self.nesting >= 128 {
            return Err(Error::new(
                declaration.span.start,
                "declarator nesting limit exceeded",
            ));
        }
        self.nesting += 1;
        let result = (|| {
            let mut prepared = Vec::new();
            let mut declaration = declaration;
            let mut parenthesized = false;
            for _ in 0..128 {
                if parenthesized && !declaration.node.extensions.is_empty() {
                    let mut attributes = Attributes {
                        target_type_name: type_name,
                        ..Attributes::default()
                    };
                    self.attributes(&declaration.node.extensions, &mut attributes)?;
                    prepared.push(attributes);
                }
                for derived in &declaration.node.derived {
                    let ast::DerivedDeclarator::Pointer(qualifiers) = &derived.node else {
                        continue;
                    };
                    for qualifier in qualifiers {
                        if let ast::PointerQualifier::Extension(extensions) = &qualifier.node {
                            let mut attributes = Attributes {
                                alias_base,
                                target_type_name: type_name,
                                ..Attributes::default()
                            };
                            self.attributes(extensions, &mut attributes)?;
                            prepared.push(attributes);
                        }
                    }
                }
                let ast::DeclaratorKind::Declarator(inner) = &declaration.node.kind.node else {
                    return Ok(prepared);
                };
                declaration = inner;
                parenthesized = true;
            }
            Err(Error::new(
                declaration.span.start,
                "declarator nesting limit exceeded",
            ))
        })();
        self.nesting -= 1;
        result
    }

    /// `parameter_array` identifies the outermost array adjusted to a pointer.
    fn declarator_at(
        &mut self,
        mut ty: Type,
        declaration: &Node<ast::Declarator>,
        context: DeclaratorContext<'_>,
        prepared: &mut std::vec::IntoIter<Attributes>,
    ) -> Result<(Option<String>, Type, Attributes), Error> {
        let DeclaratorContext {
            parameter_array,
            parenthesized,
            alias_base,
            base_use,
            type_name,
            definition,
        } = context;
        if self.nesting >= 128 {
            return Err(Error::new(
                declaration.span.start,
                "declarator nesting limit exceeded",
            ));
        }
        self.nesting += 1;
        let prefix_attributes = if parenthesized && !declaration.node.extensions.is_empty() {
            let mut attributes = prepared.next().expect("prepared prefix attributes");
            if self.unit.compiler == Compiler::Gnu
                && let Some(alignment) = attributes.alignment.take()
                && !matches!(
                    self.unit.resolve(&ty)?.kind,
                    TypeKind::Void | TypeKind::Function(_)
                )
            {
                // GNU attributes after `(` annotate the incoming type, before
                // this declarator adds pointers, arrays, or function types.
                ty.alignment = u32::try_from(alignment)
                    .ok()
                    .and_then(crate::TypeAlignment::new)
                    .ok_or_else(|| {
                        Error::new(
                            declaration.span.start,
                            "type alignment exceeds the supported range",
                        )
                    })?;
            }
            Some(attributes)
        } else {
            None
        };
        // In a parenthesized declarator, its incoming type may already be a
        // function. Leading conventions annotate that function before a new
        // pointer or an outer function is constructed around it.
        let mut leading_convention_applied = false;
        if (parenthesized || self.unit.compiler == Compiler::Clang)
            && declaration
                .node
                .extensions
                .iter()
                .any(|extension| is_calling_extension(&extension.node))
            && self.has_function_boundary(&ty)?
        {
            let extensions = declaration
                .node
                .extensions
                .iter()
                .filter(|extension| is_calling_extension(&extension.node))
                .cloned()
                .collect::<Vec<_>>();
            let mut attributes = Attributes {
                alias_base,
                ..Attributes::default()
            };
            self.attributes(&extensions, &mut attributes)?;
            ty = self.apply_calling_convention(ty, &attributes, declaration.span.start)?;
            leading_convention_applied = true;
        }
        let mut type_use = if let Some(checked) = &mut self.checked {
            Some(match base_use {
                Some(id) => id,
                None => checked.plain_type_use(&ty, declaration.span.start)?,
            })
        } else {
            None
        };
        let mut alias_convention = None;
        let mut pending_pointer_convention = None;
        let mut nodebug_arguments = None;
        let mut target_attributes = Vec::new();
        let mut minimum_vector_width = Vec::new();
        let mut always_inline = None;
        let mut no_inline = None;
        let mut gnu_inline = None;
        let mut noescape = Vec::new();
        let mut type_noreturn = false;
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
            let mut retained_bound = None;
            let mut prototype_scope = None;
            let mut parameter_uses = self.checked.as_ref().map(|_| Vec::new());
            ty = match &derived.node {
                ast::DerivedDeclarator::Pointer(qualifiers) => {
                    let mut pointer = ty.pointer();
                    let mut atomic = false;
                    let mut pointer_width = None;
                    for qualifier in qualifiers {
                        match &qualifier.node {
                            ast::PointerQualifier::TypeQualifier(qualifier) => {
                                add_qualifier(&mut pointer.qualifiers, &mut atomic, qualifier)?
                            }
                            ast::PointerQualifier::MsvcPointerWidth(width) => {
                                if let Some(previous) = pointer_width.replace(*width) {
                                    return Err(Error::new(
                                        qualifier.span.start,
                                        if previous == *width {
                                            "duplicate pointer width qualifier"
                                        } else {
                                            "conflicting pointer width qualifiers"
                                        },
                                    ));
                                }
                                if *width == 32 {
                                    if !matches!(
                                        self.unit.target,
                                        Target::Aarch64PcWindowsMsvc | Target::X86_64PcWindowsMsvc
                                    ) {
                                        return Err(Error::new(
                                            qualifier.span.start,
                                            "__ptr32 pointer ABI is unsupported on this target",
                                        ));
                                    }
                                    pointer.qualifiers.set_msvc_ptr32(true);
                                }
                            }
                            ast::PointerQualifier::Extension(_) => {
                                let mut attributes =
                                    prepared.next().expect("prepared pointer attributes");
                                noescape.extend_from_slice(&attributes.noescape);
                                type_noreturn |= attributes.type_noreturn;
                                nodebug_arguments =
                                    nodebug_arguments.or(attributes.nodebug_arguments);
                                target_attributes
                                    .extend(attributes.target_attributes.iter().cloned());
                                minimum_vector_width
                                    .extend_from_slice(&attributes.minimum_vector_width);
                                always_inline = crate::target_features::merge_inline(
                                    always_inline,
                                    attributes.always_inline,
                                );
                                no_inline = crate::target_features::merge_inline(
                                    no_inline,
                                    attributes.no_inline,
                                );
                                gnu_inline = crate::target_features::merge_inline(
                                    gnu_inline,
                                    attributes.gnu_inline,
                                );
                                if alias_base {
                                    merge_convention(
                                        &mut alias_convention,
                                        attributes.calling_convention,
                                        qualifier.span.start,
                                    )?;
                                }
                                if self.unit.compiler == Compiler::Clang
                                    && attributes.calling_convention.is_some()
                                {
                                    attributes.alias_base = true;
                                    if !self.has_function_boundary(&pointer)? {
                                        merge_convention(
                                            &mut pending_pointer_convention,
                                            attributes.calling_convention.take(),
                                            qualifier.span.start,
                                        )?;
                                    }
                                }
                                pointer = self.apply_calling_convention(
                                    pointer,
                                    &attributes,
                                    qualifier.span.start,
                                )?;
                                if let Some(bytes) = attributes.vector_size {
                                    pointer =
                                        self.vector_type(pointer, bytes, qualifier.span.start)?;
                                }
                                if attributes.packed
                                    || attributes.alignment.is_some()
                                    || attributes.mode.is_some()
                                    || attributes.link_name.is_some()
                                    || !attributes.diagnostic_attributes.is_empty()
                                    || attributes.weak.is_some()
                                    || attributes.returns_twice.is_some()
                                    || attributes.transparent_union.is_some()
                                {
                                    return Err(Error::new(
                                        qualifier.span.start,
                                        "attributes that change a nested type's representation are unsupported",
                                    ));
                                }
                            }
                        }
                    }
                    if atomic {
                        pointer = self.atomic_type(pointer, false, derived.span.start)?;
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
                    if self.unit.typedef_alignment(&ty)?.is_some()
                        && !self.unit.is_variable_length_array(&ty)?
                    {
                        let alignment = self.unit.alignment(&ty)?;
                        if self.unit.layout(&ty)?.size_bytes() % alignment != 0 {
                            return Err(Error::new(
                                derived.span.start,
                                "array element size is not a multiple of its typedef alignment",
                            ));
                        }
                    }
                    let kind = match &array.node.size {
                        ast::ArraySize::Unknown => TypeKind::Array {
                            element: Box::new(ty),
                            length: None,
                        },
                        ast::ArraySize::VariableExpression(expression)
                        | ast::ArraySize::StaticExpression(expression) => {
                            // Array bounds have their own expression context, even
                            // inside a prototype or an unevaluated outer operand.
                            let allocation_context = self.allocation_context(true);
                            let bound = self.value_expression_type(expression);
                            self.restore_allocation_context(allocation_context, false);
                            let bound = bound?;
                            self.integer_type(&bound, expression.span.start)?;
                            let constant = if self.is_integer_constant_expression(expression, 0)? {
                                // Undefined arithmetic does not form an ICE. Such an
                                // expression remains a runtime bound; executing it is UB.
                                self.eval(expression).ok()
                            } else {
                                None
                            };
                            let length = constant.map(IntegerValue::as_u64).transpose()?;
                            let minimum =
                                matches!(array.node.size, ast::ArraySize::StaticExpression(_));
                            if (minimum || length.is_none())
                                && let Some(checked) = &mut self.checked
                            {
                                let scope = self.lexical_scopes.last();
                                let definition =
                                    scope.is_some_and(|scope| scope.is_definition_parameters);
                                let prototype = scope.is_some_and(|scope| {
                                    !scope.is_block && !scope.is_definition_parameters
                                });
                                retained_bound = Some(checked.array_bound(
                                    declaration,
                                    Some(expression),
                                    array.span,
                                    crate::checked::bounds::BoundContext {
                                        prototype,
                                        definition,
                                        type_name,
                                        minimum,
                                        constant: length,
                                    },
                                )?);
                            }
                            if let Some(length) = length {
                                TypeKind::Array {
                                    element: Box::new(ty),
                                    length: Some(length),
                                }
                            } else {
                                TypeKind::VariableArray {
                                    element: Box::new(ty),
                                    identity: self.array_identity(array.span)?,
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
                            if let Some(checked) = &mut self.checked {
                                retained_bound = Some(checked.array_bound(
                                    declaration,
                                    None,
                                    array.span,
                                    crate::checked::bounds::BoundContext {
                                        prototype: true,
                                        definition: false,
                                        type_name,
                                        minimum: false,
                                        constant: None,
                                    },
                                )?);
                            }
                            TypeKind::VariableArray {
                                element: Box::new(ty),
                                identity: self.array_identity(array.span)?,
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
                        prototype_scope =
                            Some(checked.enter_scope(ScopeKind::Prototype, derived.span, None)?);
                    }
                    self.lexical_scopes.push(LexicalScope {
                        is_definition_parameters: self.definition_parameters
                            == Some(derived.span.start),
                        parameters: Vec::with_capacity(function.node.parameters.len()),
                        ..LexicalScope::default()
                    });
                    let sve_checkpoint = self.sve_feature_checkpoint();
                    let definition_parameters = self
                        .lexical_scopes
                        .last()
                        .expect("prototype scope")
                        .is_definition_parameters;
                    let prototype = !function.node.parameters.is_empty();
                    let mut no_escape = Vec::new();
                    for (index, parameter) in function.node.parameters.iter().enumerate() {
                        let (site, noescape) = self.check_parameter(
                            crate::parameters::ParameterSyntax::Prototype(parameter),
                            None,
                        )?;
                        if noescape {
                            no_escape.push(index as u32);
                        }
                        if let (Some(checked), Some(uses), Some(site)) =
                            (&self.checked, &mut parameter_uses, site)
                        {
                            uses.push(checked.site_type_use(site));
                        }
                    }
                    if !definition_parameters {
                        self.discard_sve_feature_uses(sve_checkpoint);
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
                    let parameter_contracts =
                        self.intern_parameter_contracts(&no_escape, derived.span.start)?;
                    Type::new(TypeKind::Function(Box::new(FunctionType {
                        noreturn: false,
                        parameter_contracts,
                        return_type: ty,
                        parameters,
                        variadic,
                        prototype,
                        calling_convention: CallingConvention::C,
                    })))
                }
                ast::DerivedDeclarator::KRFunction(parameters)
                    if definition.is_some()
                        && self.definition_parameters == Some(derived.span.start) =>
                {
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
                    prototype_scope = self.check_old_style_parameters(
                        definition.expect("definition context"),
                        parameters,
                        derived.span,
                    )?;
                    Type::new(TypeKind::Function(Box::new(FunctionType {
                        noreturn: false,
                        parameter_contracts: None,
                        return_type: ty,
                        parameters: Vec::new(),
                        variadic: false,
                        prototype: false,
                        calling_convention: CallingConvention::C,
                    })))
                }
                ast::DerivedDeclarator::KRFunction(parameters) if parameters.is_empty() => {
                    Type::new(TypeKind::Function(Box::new(FunctionType {
                        noreturn: false,
                        parameter_contracts: None,
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
            // A flat parser declarator creates a nested semantic type. Validate
            // each new layer before a later modifier or retained node clones it.
            self.unit.is_variably_modified(&ty).map_err(|mut error| {
                error.offset = derived.span.start;
                error
            })?;
            if let (Some(checked), Some(current)) = (&mut self.checked, type_use) {
                use crate::checked::bounds::TypeStep;
                type_use = Some(match &derived.node {
                    ast::DerivedDeclarator::Pointer(_) => {
                        if let TypeKind::Atomic(value) = &ty.kind {
                            let pointer = checked.wrap_type_use(
                                current,
                                value,
                                TypeStep::Pointer,
                                None,
                                derived.span.start,
                            )?;
                            checked.wrap_type_use(
                                pointer,
                                &ty,
                                TypeStep::AtomicValue,
                                None,
                                derived.span.start,
                            )?
                        } else {
                            checked.wrap_type_use(
                                current,
                                &ty,
                                TypeStep::Pointer,
                                None,
                                derived.span.start,
                            )?
                        }
                    }
                    ast::DerivedDeclarator::Array(_) => checked.wrap_type_use(
                        current,
                        &ty,
                        TypeStep::Element,
                        retained_bound,
                        derived.span.start,
                    )?,
                    ast::DerivedDeclarator::Function(_) | ast::DerivedDeclarator::KRFunction(_) => {
                        checked.function_type_use(
                            current,
                            parameter_uses.as_deref().unwrap_or_default(),
                            prototype_scope,
                            if matches!(derived.node, ast::DerivedDeclarator::KRFunction(_)) {
                                self.function_scope
                                    .as_ref()
                                    .and_then(|scope| scope.old_style.as_ref())
                                    .and_then(|signature| signature.retained.as_ref())
                                    .map(|retained| retained.parameters.as_slice())
                            } else {
                                None
                            },
                            &ty,
                            derived.span.start,
                        )?
                    }
                    ast::DerivedDeclarator::Block(_) => unreachable!(),
                });
            }
        }
        let mut attributes = if let Some(attributes) = prefix_attributes {
            attributes
        } else {
            let mut attributes = Attributes {
                target_type_name: type_name,
                ..Attributes::default()
            };
            self.attributes(&declaration.node.extensions, &mut attributes)?;
            attributes
        };
        attributes.noescape.extend(noescape);
        attributes.type_noreturn |= type_noreturn;
        attributes.nodebug_arguments = attributes.nodebug_arguments.or(nodebug_arguments);
        attributes.target_attributes.extend(target_attributes);
        attributes.minimum_vector_width.extend(minimum_vector_width);
        attributes.always_inline =
            crate::target_features::merge_inline(attributes.always_inline, always_inline);
        attributes.no_inline =
            crate::target_features::merge_inline(attributes.no_inline, no_inline);
        attributes.gnu_inline =
            crate::target_features::merge_inline(attributes.gnu_inline, gnu_inline);
        attributes.target_type_name = type_name;
        if type_name && self.unit.compiler == Compiler::Clang {
            attributes.mode = None;
        }
        if let Some(mode) = &attributes.mode {
            ty = self.machine_mode(ty, mode, declaration.span.start)?;
        }
        if let Some(bytes) = attributes.vector_size {
            ty = self.vector_type(ty, bytes, declaration.span.start)?;
        }
        let calling_convention = if leading_convention_applied {
            None
        } else {
            attributes.calling_convention
        };
        let (name, ty, mut attributes) = match &declaration.node.kind.node {
            ast::DeclaratorKind::Identifier(identifier) => {
                (Some(identifier.node.name.clone()), ty, attributes)
            }
            ast::DeclaratorKind::Abstract => (None, ty, attributes),
            ast::DeclaratorKind::Declarator(inner) => {
                let (name, ty, inner_attributes) = self.declarator_at(
                    ty,
                    inner,
                    DeclaratorContext {
                        base_use: type_use,
                        parenthesized: true,
                        ..context
                    },
                    prepared,
                )?;
                type_use = inner_attributes.type_use;
                attributes.noescape.extend(inner_attributes.noescape);
                attributes.type_noreturn |= inner_attributes.type_noreturn;
                if alias_base {
                    merge_convention(
                        &mut alias_convention,
                        inner_attributes.calling_convention,
                        declaration.span.start,
                    )?;
                }
                // GCC ignores packed on a type inside a parenthesized declarator.
                if inner_attributes.packed && self.unit.compiler == Compiler::Clang {
                    attributes.packed = true;
                }
                if inner_attributes.msvc_alignment.is_some() {
                    attributes.msvc_alignment = inner_attributes.msvc_alignment;
                }
                if inner_attributes.alignment.is_some() {
                    attributes.alignment = attributes.alignment.max(inner_attributes.alignment);
                }
                if inner_attributes.link_name.is_some() {
                    attributes.link_name = inner_attributes.link_name;
                }
                attributes.nodebug_arguments = attributes
                    .nodebug_arguments
                    .or(inner_attributes.nodebug_arguments);
                attributes
                    .target_attributes
                    .extend(inner_attributes.target_attributes);
                attributes
                    .minimum_vector_width
                    .extend(inner_attributes.minimum_vector_width);
                attributes.always_inline = crate::target_features::merge_inline(
                    attributes.always_inline,
                    inner_attributes.always_inline,
                );
                attributes.no_inline = crate::target_features::merge_inline(
                    attributes.no_inline,
                    inner_attributes.no_inline,
                );
                attributes.gnu_inline = crate::target_features::merge_inline(
                    attributes.gnu_inline,
                    inner_attributes.gnu_inline,
                );
                attributes.inline_specifier = attributes
                    .inline_specifier
                    .or(inner_attributes.inline_specifier);
                crate::dll_storage::merge_attributes(
                    &mut attributes.dll_storage,
                    inner_attributes.dll_storage.as_deref(),
                );
                attributes.weak = attributes.weak.or(inner_attributes.weak);
                attributes.returns_twice =
                    attributes.returns_twice.or(inner_attributes.returns_twice);
                attributes.noreturn = attributes.noreturn.or(inner_attributes.noreturn);
                attributes.c11_noreturn = attributes.c11_noreturn.or(inner_attributes.c11_noreturn);
                attributes.transparent_union = attributes
                    .transparent_union
                    .or(inner_attributes.transparent_union);
                attributes
                    .diagnostic_attributes
                    .extend(inner_attributes.diagnostic_attributes);
                (name, ty, attributes)
            }
        };
        let ty = self.apply_calling_convention(
            ty,
            &Attributes {
                calling_convention,
                alias_base,
                type_noreturn: attributes.type_noreturn,
                ..Attributes::default()
            },
            declaration.span.start,
        )?;
        // A convention after `*` can precede its function prototype, as in
        // `int *__cdecl f(int)`, when no incoming callback type consumed it.
        let ty = self.apply_calling_convention(
            ty,
            &Attributes {
                calling_convention: pending_pointer_convention,
                alias_base: true,
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
        if let (Some(checked), Some(current)) = (&mut self.checked, type_use) {
            let current = checked.retype_use(current, &ty, declaration.span.start)?;
            checked.declarator_type_use(declaration, name.as_deref(), current)?;
            attributes.type_use = Some(current);
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
        let reference = binding.is_some() && declaration.node.declarations.is_none();
        let introduced = binding.is_none();
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
                transparent_union: false,
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
        self.note_lexical_tag(
            Tag::Record(id),
            introduced,
            declaration.node.declarations.is_some(),
            self.unit.records[id].fields.is_some(),
            self.unit.records[id].name.is_none(),
            declaration.span.start,
        )?;
        if self.scope() == Scope::File
            && let Some(origins) = &mut self.documentation_origins
        {
            // File declarations document their typedef or object, not an embedded
            // forward tag. Standalone tags are marked in declaration_specifiers;
            // a new tag inside a record field has its own documentable cursor.
            origins.push(
                crate::DocumentationTarget::Record(id),
                declaration.span.start,
                declaration
                    .node
                    .identifier
                    .as_ref()
                    .map_or(declaration.span.start, |name| name.span.start),
                reference
                    || (self.lexical_record.is_none() && declaration.node.declarations.is_none()),
            )?;
        }
        if self.scope() == Scope::File
            && let Some(origins) = &mut self.declaration_origins
        {
            origins.push(
                crate::DeclarationTarget::Record(id),
                declaration
                    .node
                    .identifier
                    .as_ref()
                    .map_or(declaration.span, |name| name.span),
                declaration.node.declarations.is_some(),
                false,
                reference,
                false,
            )?;
        }
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
        if declaration.node.declarations.is_some() {
            let previous = self.lexical_record.replace(id);
            let previous_doc = self.documentation_parent(
                declaration
                    .node
                    .identifier
                    .as_ref()
                    .map_or(declaration.span.start, |name| name.span.start),
            );
            let result = self.complete_record(id, kind, declaration);
            self.restore_documentation_parent(previous_doc);
            self.lexical_record = previous;
            result?;
        }
        Ok(id)
    }

    /// Complete fields while the caller holds the lexical record context.
    fn complete_record(
        &mut self,
        id: usize,
        kind: RecordKind,
        declaration: &Node<ast::StructType>,
    ) -> Result<(), Error> {
        let declarations = declaration.node.declarations.as_ref().unwrap();
        if self.unit.records[id].fields.is_some() {
            return Err(Error::new(
                declaration.span.start,
                "record is defined more than once",
            ));
        }
        let mut fields = Vec::new();
        for declaration in declarations {
            match &declaration.node {
                ast::StructDeclaration::StaticAssert(assertion) => self.static_assert(assertion)?,
                ast::StructDeclaration::Field(field) => {
                    let dependency_field = !field.node.declarators.is_empty()
                        && self
                            .parameter_type_dependencies
                            .as_ref()
                            .is_some_and(|deps| {
                                self.unit.records[id].scope == Scope::File || deps.active()
                            });
                    if dependency_field && let Some(deps) = &mut self.parameter_type_dependencies {
                        deps.begin(field.span, false)?;
                    }
                    let (mut base, mut attributes) =
                        self.specifier_qualifiers(&field.node.specifiers, false)?;
                    attributes.require_function_attributes(false)?;
                    attributes.require_no_weak()?;
                    attributes.require_no_transparent_union()?;
                    if field.node.declarators.is_empty() {
                        let Some(syntax) =
                            anonymous_record_specifier(&field.node.specifiers, self.unit.target)
                        else {
                            continue;
                        };
                        match syntax {
                            AnonymousRecordSpecifier::Direct => {
                                let atomic = self.unit.atomic_value(&base)?;
                                if atomic.is_some() && self.unit.compiler == Compiler::Gnu {
                                    return Err(Error::new(
                                        field.span.start,
                                        "GNU atomic anonymous record members are unsupported",
                                    ));
                                }
                                let TypeKind::Record(record) =
                                    self.unit.resolve(atomic.unwrap_or(&base))?.kind
                                else {
                                    continue;
                                };
                                if self.unit.records[record].name.is_some() {
                                    continue;
                                }
                                // Clang discards written qualifiers on direct
                                // anonymous members; GNU retains const/volatile.
                                if self.unit.compiler == Compiler::Clang {
                                    base = Type::new(TypeKind::Record(record));
                                }
                            }
                            AnonymousRecordSpecifier::MicrosoftTag
                            | AnonymousRecordSpecifier::MicrosoftTypedef(_) => {
                                let ty = match syntax {
                                    AnonymousRecordSpecifier::MicrosoftTypedef(name) => self
                                        .local_typedef(name)
                                        .or_else(|| self.unit.typedefs.get(name))
                                        .ok_or_else(|| {
                                            Error::new(field.span.start, "unknown member typedef")
                                        })?,
                                    // Written qualifiers do not belong to the
                                    // Microsoft anonymous field's storage type.
                                    _ => self.unit.atomic_value(&base)?.unwrap_or(&base),
                                };
                                let TypeKind::Record(record) = self.unit.resolve(ty)?.kind else {
                                    continue;
                                };
                                // Clang embeds the canonical record, dropping
                                // typedef qualifiers/alignment and declaration
                                // attributes. Attributes on the tag itself have
                                // already been applied to its record identity.
                                base = Type::new(TypeKind::Record(record));
                                attributes = Attributes::default();
                            }
                        }
                        let field_alignment = self.check_declaration_alignment(
                            &base,
                            &attributes,
                            &Attributes::default(),
                            crate::object_alignment::AlignmentSubject::Field { bitfield: false },
                            field.span.start,
                        )?;
                        let member = Field {
                            name: None,
                            ty: base,
                            bit_width: None,
                            alignment: field_alignment
                                .explicit()
                                .map(|value| u64::from(value.get())),
                            packed: attributes.packed,
                        };
                        if self.unit.records[id].scope == Scope::File
                            && let Some(origins) = &mut self.documentation_origins
                        {
                            origins.push(
                                crate::DocumentationTarget::Field {
                                    record: id,
                                    field: fields.len(),
                                },
                                field.span.start,
                                field.span.start,
                                false,
                            )?;
                        }
                        if let Some(checked) = &mut self.checked {
                            let site = checked.member_declaration(
                                field,
                                crate::checked::OccurrenceKind::Field,
                                id,
                                fields.len(),
                                &member,
                                None,
                            )?;
                            if let Some(site) = site {
                                checked.attach_alignment(site, field_alignment, field_alignment)?;
                            }
                        }
                        fields.push(member);
                    } else {
                        for declarator in &field.node.declarators {
                            if dependency_field
                                && let Some(deps) = &mut self.parameter_type_dependencies
                            {
                                deps.begin(
                                    crate::checked::references::member_name_span(declarator)
                                        .unwrap_or(declarator.span),
                                    true,
                                )?;
                            }
                            let (name, mut ty, mut extra) =
                                if let Some(declarator) = &declarator.node.declarator {
                                    self.declarator(base.clone(), declarator, &attributes)?
                                } else {
                                    (None, base.clone(), Attributes::default())
                                };
                            extra.require_function_attributes(false)?;
                            extra.require_no_weak()?;
                            extra.require_no_transparent_union()?;
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
                                // Clang checks the width before declarator machine modes.
                                // The base type already includes modes carried by typedefs.
                                let final_width = self.unit.layout(&ty)?.size_bits;
                                let declared_width =
                                    if self.unit.compiler == Compiler::Clang && base != ty {
                                        self.unit.layout(&base)?.size_bits
                                    } else {
                                        final_width
                                    };
                                if width > declared_width {
                                    return Err(Error::new(
                                        declarator.span.start,
                                        "bitfield is wider than its type",
                                    ));
                                }
                                if width > final_width {
                                    return Err(Error::new(
                                        declarator.span.start,
                                        "bitfields wider than their attribute-modified type are not supported",
                                    ));
                                }
                            }
                            if !declarator.node.extensions.is_empty() {
                                self.attributes(&declarator.node.extensions, &mut extra)?;
                                extra.require_function_attributes(false)?;
                                extra.require_no_weak()?;
                                extra.require_no_transparent_union()?;
                                // Suffix attributes apply after checking the declared width.
                                if let Some(mode) = &extra.mode {
                                    ty = self.machine_mode(ty, mode, declarator.span.start)?;
                                    if let Some(width) = bit_width
                                        && width > self.unit.layout(&ty)?.size_bits
                                    {
                                        return Err(Error::new(
                                            declarator.span.start,
                                            "bitfields wider than their attribute-modified type are not supported",
                                        ));
                                    }
                                }
                                if let Some(bytes) = extra.vector_size {
                                    ty = self.vector_type(ty, bytes, declarator.span.start)?;
                                    if bit_width.is_some() {
                                        return Err(Error::new(
                                            declarator.span.start,
                                            "vector bitfield types are not supported",
                                        ));
                                    }
                                }
                            }
                            let field_alignment = self.check_declaration_alignment(
                                &ty,
                                &attributes,
                                &extra,
                                crate::object_alignment::AlignmentSubject::Field {
                                    bitfield: bit_width.is_some(),
                                },
                                declarator.span.start,
                            )?;
                            let member = Field {
                                name,
                                ty,
                                bit_width,
                                alignment: field_alignment
                                    .explicit()
                                    .map(|value| u64::from(value.get())),
                                packed: extra.packed || attributes.packed,
                            };
                            if self.unit.records[id].scope == Scope::File
                                && let Some(origins) = &mut self.documentation_origins
                            {
                                origins.push(
                                    crate::DocumentationTarget::Field {
                                        record: id,
                                        field: fields.len(),
                                    },
                                    field.span.start,
                                    crate::checked::references::member_name_span(declarator)
                                        .unwrap_or(declarator.span)
                                        .start,
                                    false,
                                )?;
                            }
                            if let Some(checked) = &mut self.checked {
                                let site = checked.member_declaration(
                                    declarator,
                                    crate::checked::OccurrenceKind::StructDeclarator,
                                    id,
                                    fields.len(),
                                    &member,
                                    crate::checked::references::member_name_span(declarator),
                                )?;
                                if let Some(site) = site {
                                    checked.attach_alignment(
                                        site,
                                        field_alignment,
                                        field_alignment,
                                    )?;
                                }
                            }
                            if dependency_field
                                && let Some(deps) = &mut self.parameter_type_dependencies
                            {
                                deps.record_field(id)?;
                            }
                            fields.push(member);
                        }
                    }
                    if dependency_field && let Some(deps) = &mut self.parameter_type_dependencies {
                        deps.discard();
                    }
                }
            }
        }
        let mut member_names = FxHashSet::default();
        let mut has_named_member = false;
        let mut remaining_member_work = 1_000_000;
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
                &mut remaining_member_work,
            )?;
            // GCC also counts anonymous records containing only unnamed bitfields.
            has_named_member |= !member_names.is_empty()
                || (self.unit.compiler == toucan_target::Compiler::Gnu
                    && field.name.is_none()
                    && field.bit_width.is_none());
        }
        self.unit.records[id].pack = self
            .packs
            .iter()
            .take_while(|(offset, _)| *offset <= declaration.span.start)
            .last()
            .and_then(|(_, pack)| *pack);
        self.unit.records[id].fields = Some(fields);
        Ok(())
    }

    /// Anonymous members share their containing record's member namespace.
    fn check_member_names<'a>(
        &'a self,
        fields: &'a [Field],
        names: &mut FxHashSet<&'a str>,
        offset: usize,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<(), Error> {
        if depth >= 128 {
            return Err(Error::new(
                offset,
                "anonymous member nesting exceeds the 128-level limit",
            ));
        }
        for field in fields {
            // Microsoft anonymous tag and typedef members can share a record
            // graph. A depth bound alone does not limit repeated field visits.
            *remaining = remaining.checked_sub(1).ok_or_else(|| {
                Error::new(offset, "anonymous member validation work limit exceeded")
            })?;
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
                    self.check_member_names(fields, names, offset, depth + 1, remaining)?;
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
            TypeKind::Void | TypeKind::Function(_) | TypeKind::Sve(_) => false,
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
            TypeKind::VariableArray { element, .. } | TypeKind::Atomic(element) => {
                self.is_complete_object(element, depth + 1)?
            }
            _ => true,
        })
    }

    /// C11 6.7.3 permits `restrict` only on pointers to object or incomplete types.
    fn check_restrict(&self, ty: &Type, offset: usize) -> Result<(), Error> {
        if !self.unit.qualifiers(ty)?.is_restrict {
            return Ok(());
        }
        let mut resolved = self.unit.resolve(ty)?;
        if self.gnu_sync_profile()
            && let TypeKind::Atomic(value) = &resolved.kind
        {
            resolved = self.unit.resolve(value)?;
        }
        // GNU C propagates qualifiers on array typedefs to their element type.
        if self.unit.compiler == toucan_target::Compiler::Gnu {
            for _ in 0..128 {
                let (TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. }) =
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
        let reference = binding.is_some() && declaration.node.enumerators.is_empty();
        let introduced = binding.is_none();
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
                packed: false,
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
        self.note_lexical_tag(
            Tag::Enum(id),
            introduced,
            !declaration.node.enumerators.is_empty(),
            self.unit.enums[id].complete,
            self.unit.enums[id].name.is_none(),
            declaration.span.start,
        )?;
        if self.scope() == Scope::File
            && let Some(origins) = &mut self.documentation_origins
        {
            origins.push(
                crate::DocumentationTarget::Enum(id),
                declaration.span.start,
                declaration
                    .node
                    .identifier
                    .as_ref()
                    .map_or(declaration.span.start, |name| name.span.start),
                reference
                    || (self.lexical_record.is_none() && declaration.node.enumerators.is_empty()),
            )?;
        }
        if self.scope() == Scope::File
            && let Some(origins) = &mut self.declaration_origins
        {
            origins.push(
                crate::DeclarationTarget::Enum(id),
                declaration
                    .node
                    .identifier
                    .as_ref()
                    .map_or(declaration.span, |name| name.span),
                !declaration.node.enumerators.is_empty(),
                false,
                reference,
                false,
            )?;
        }
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
        let previous_doc = self.documentation_parent(
            declaration
                .node
                .identifier
                .as_ref()
                .map_or(declaration.span.start, |name| name.span.start),
        );
        let result = self.complete_enum(id, declaration);
        self.restore_documentation_parent(previous_doc);
        result?;
        Ok(id)
    }

    fn documentation_parent(&mut self, name: usize) -> Option<usize> {
        self.documentation_origins
            .as_mut()
            .and_then(|origins| origins.parent_name.replace(name))
    }

    fn restore_documentation_parent(&mut self, previous: Option<usize>) {
        if let Some(origins) = &mut self.documentation_origins {
            origins.parent_name = previous;
        }
    }

    fn complete_enum(&mut self, id: usize, declaration: &Node<ast::EnumType>) -> Result<(), Error> {
        let mut previous: Option<IntegerValue> = None;
        for enumerator in &declaration.node.enumerators {
            let mut attributes = Attributes::default();
            self.attributes(&enumerator.node.extensions, &mut attributes)?;
            attributes.require_function_attributes(false)?;
            attributes.require_no_weak()?;
            attributes.require_no_transparent_union()?;
            let value = if let Some(expression) = &enumerator.node.expression {
                self.within_enum_expression(|analyzer| analyzer.eval(expression))?
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
            if self.scope() == Scope::File
                && let Some(origins) = &mut self.declaration_origins
            {
                origins.push(
                    crate::DeclarationTarget::Enumerator {
                        enumeration: id,
                        variant: self.unit.enums[id].variants.len(),
                    },
                    enumerator.node.identifier.span,
                    true,
                    false,
                    false,
                    false,
                )?;
            }
            if self.scope() == Scope::File
                && let Some(origins) = &mut self.documentation_origins
            {
                origins.push(
                    crate::DocumentationTarget::Enumerator {
                        enumeration: id,
                        variant: self.unit.enums[id].variants.len(),
                    },
                    enumerator.span.start,
                    enumerator.node.identifier.span.start,
                    false,
                )?;
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
        Ok(())
    }

    /// Clang lets a later unannotated declaration inherit an established ABI.
    pub(crate) fn inherit_calling_convention(
        &self,
        ty: Type,
        previous: &Type,
    ) -> Result<Type, Error> {
        if self.unit.compiler != Compiler::Clang {
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
        let ty = if attributes.type_noreturn {
            self.apply_type_noreturn(ty, 0)?
        } else {
            ty
        };
        let Some(convention) = attributes.calling_convention else {
            return Ok(ty);
        };
        self.apply_convention_at(ty, convention, offset, 0, attributes.alias_base)
    }

    /// Finds a function through declarator pointers and arrays, without walking
    /// through the function's return type or record members.
    fn has_function_boundary<'a>(&'a self, mut ty: &'a Type) -> Result<bool, Error> {
        for _ in 0..128 {
            match &self.unit.resolve(ty)?.kind {
                TypeKind::Function(_) => return Ok(true),
                TypeKind::Pointer(element)
                | TypeKind::Array { element, .. }
                | TypeKind::VariableArray { element, .. } => ty = element,
                _ => return Ok(false),
            }
        }
        Err(Error::new(
            0,
            "calling convention type nesting exceeds the 128-level limit",
        ))
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
        resolved.alignment = self.unit.typedef_alignment_metadata(&ty)?;
        let clang = self.unit.compiler == Compiler::Clang;
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
                if convention == CallingConvention::Aarch64Vector
                    && matches!(
                        self.unit.target,
                        Target::Aarch64UnknownLinuxGnu | Target::Aarch64UnknownLinuxMusl
                    )
                    && self.unit.compiler == Compiler::Gnu
                    && function.aarch64_pcs(&self.unit)? == Some(crate::Aarch64Pcs::Sve)
                {
                    return Err(Error::new(
                        offset,
                        "aarch64_vector_pcs cannot apply to an SVE function type on the GNU profile",
                    ));
                }
                // Clang accepts ms_abi on Windows ARM64, where it is the
                // default C ABI. Rust has no win64 ABI for that architecture.
                function.calling_convention = if self.unit.target == Target::Aarch64PcWindowsMsvc
                    && convention == CallingConvention::Win64
                {
                    CallingConvention::C
                } else {
                    convention
                };
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
            TypeKind::Array { element, .. } | TypeKind::VariableArray { element, .. } => {
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

    fn apply_tag_attributes(
        &mut self,
        ty: &Type,
        attributes: &Attributes,
        offset: usize,
    ) -> Result<(), Error> {
        attributes.require_function_attributes(false)?;
        attributes.require_no_weak()?;
        if let Some(span) = attributes.transparent_union {
            self.apply_transparent_record(ty, span.start)?;
        }
        if !attributes.packed && attributes.vendor_alignment().is_none() {
            return Ok(());
        }
        if let TypeKind::Record(id) = self.unit.resolve(ty)?.kind {
            let record = &mut self.unit.records[id];
            record.packed |= attributes.packed;
            if attributes.vendor_alignment().is_some() {
                record.alignment = if attributes.msvc_alignment.is_some() {
                    record.alignment.max(attributes.vendor_alignment())
                } else {
                    attributes.alignment
                };
            }
        } else if let TypeKind::Enum(id) = self.unit.resolve(ty)?.kind {
            self.unit.enums[id].packed |= attributes.packed;
            if let Some(alignment) = attributes.vendor_alignment()
                && self.unit.compiler == Compiler::Clang
            {
                // GCC ignores tag alignment. Clang preserves it independently
                // of integer size, so an ordinary Rust integer is suitable
                // only when the attribute leaves all storage rules unchanged.
                if self.unit.target.is_windows() {
                    return Err(Error::new(
                        offset,
                        "alignment attributes on Microsoft enum tags are unsupported",
                    ));
                }
                if !self.unit.enums[id].complete {
                    return Err(Error::new(
                        offset,
                        "alignment attributes on incomplete enum tags are unsupported",
                    ));
                }
                if self.unit.layout(ty)?.alignment_bits / 8 != alignment {
                    return Err(Error::new(
                        offset,
                        "enum alignment that changes storage layout is unsupported",
                    ));
                }
            }
        } else if attributes.packed || attributes.vendor_alignment().is_some() {
            return Err(Error::new(
                offset,
                "layout attributes on a non-record type are unsupported",
            ));
        }
        Ok(())
    }

    /// Function-pointer fields and parameters are valid Clang nodebug subjects.
    fn check_nodebug_function_like(&self, ty: &Type, attributes: &Attributes) -> Result<(), Error> {
        if attributes.nodebug_arguments.is_none() {
            return Ok(());
        }
        let kind = &self.unit.resolve(ty)?.kind;
        if matches!(kind, TypeKind::Function(_))
            || matches!(kind, TypeKind::Pointer(element) if matches!(self.unit.resolve(element)?.kind, TypeKind::Function(_)))
        {
            attributes.check_nodebug_subject()?;
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
                ast::Extension::Declspec(attribute) => {
                    self.declspec_attribute(attribute, extension.span, result, false)?;
                }
                ast::Extension::Attribute(attribute)
                | ast::Extension::CallingConvention(attribute) => {
                    let name = attribute.name.node.trim_matches('_');
                    if self.unit.target.is_armv7() && name == "pcs" {
                        // `pcs("aapcs")` changes the default hard-float
                        // argument ABI to base AAPCS. Silently ignoring it
                        // would emit an incompatible Rust function signature.
                        return Err(Error::new(
                            extension.span.start,
                            "ARMv7 pcs calling convention is unsupported",
                        ));
                    }
                    if matches!(extension.node, ast::Extension::CallingConvention(_)) {
                        match name {
                            "pascal" => continue,
                            "vectorcall" | "regcall" if self.unit.target.is_aarch64() => continue,
                            "vectorcall" | "regcall" => {
                                return Err(Error::new(
                                    extension.span.start,
                                    format!(
                                        "unsupported calling-convention keyword `{}` on this target",
                                        attribute.name.node
                                    ),
                                ));
                            }
                            _ => {}
                        }
                    }
                    match crate::attributes::Attribute::from_name(name) {
                        Some(crate::attributes::Attribute::MinimumVectorWidth) => {
                            if self.unit.compiler == Compiler::Gnu || result.target_type_name {
                                // Ignoring an attribute does not skip parsing its expressions.
                                // GNU additionally permits bare identifier arguments without lookup.
                                let checkpoint = self.sve_feature_checkpoint();
                                for argument in &attribute.arguments {
                                    if self.unit.compiler != Compiler::Gnu
                                        || !matches!(argument.node, ast::Expression::Identifier(_))
                                    {
                                        self.expression_info(argument)?;
                                    }
                                }
                                self.discard_sve_feature_uses(checkpoint);
                            } else {
                                if result.minimum_vector_width.len() >= 256 {
                                    return Err(Error::new(
                                        extension.span.start,
                                        "minimum vector width attribute count exceeds the 256-entry limit",
                                    ));
                                }
                                result.minimum_vector_width.push(
                                    self.parse_minimum_vector_width(attribute, extension.span)?,
                                );
                            }
                        }
                        Some(crate::attributes::Attribute::Target) => {
                            if result.target_attributes.len() >= 256 {
                                return Err(Error::new(
                                    extension.span.start,
                                    "target attribute count exceeds the 256-entry limit",
                                ));
                            }
                            result
                                .target_attributes
                                .push(self.parse_target_attribute(attribute, extension.span)?);
                        }
                        Some(crate::attributes::Attribute::AlwaysInline) => {
                            result.always_inline = crate::target_features::merge_inline(
                                result.always_inline,
                                Some((extension.span, !attribute.arguments.is_empty())),
                            )
                        }
                        Some(crate::attributes::Attribute::GnuInline) => {
                            let arguments = !attribute.arguments.is_empty();
                            if arguments && self.unit.compiler == Compiler::Gnu {
                                return Err(Error::new(
                                    extension.span.start,
                                    "gnu_inline takes no arguments",
                                ));
                            }
                            result.gnu_inline = crate::target_features::merge_inline(
                                result.gnu_inline,
                                Some((extension.span, arguments)),
                            );
                        }
                        Some(crate::attributes::Attribute::NoInline) => {
                            result.no_inline = crate::target_features::merge_inline(
                                result.no_inline,
                                Some((extension.span, !attribute.arguments.is_empty())),
                            )
                        }
                        Some(crate::attributes::Attribute::NoEscape) => {
                            if self.unit.compiler == Compiler::Clang {
                                if result.noescape.len() >= 256 {
                                    return Err(Error::new(
                                        extension.span.start,
                                        "noescape attribute count exceeds the 256 limit",
                                    ));
                                }
                                // Attribute operands undergo ordinary lookup even when the
                                // declaration subject makes the attribute ineffective.
                                let checkpoint = self.sve_feature_checkpoint();
                                let checked = (|| {
                                    for argument in &attribute.arguments {
                                        self.expression_info(argument)?;
                                    }
                                    Ok::<_, Error>(())
                                })();
                                self.discard_sve_feature_uses(checkpoint);
                                checked?;
                                result
                                    .noescape
                                    .push((extension.span, !attribute.arguments.is_empty()));
                            }
                        }
                        Some(crate::attributes::Attribute::NoDebug) => {
                            // Debug information is outside the retained semantic graph.
                            // GCC ignores this unknown attribute, including its arguments.
                            if self.unit.compiler == Compiler::Clang
                                && !attribute.arguments.is_empty()
                            {
                                result.nodebug_arguments = Some(extension.span.start);
                            }
                        }
                        Some(crate::attributes::Attribute::TransparentUnion) => {
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    "transparent_union takes no arguments",
                                ));
                            }
                            result.transparent_union = Some(extension.span);
                        }
                        Some(crate::attributes::Attribute::ReturnsTwice)
                        | Some(crate::attributes::Attribute::NoReturn) => {
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    format!("{name} takes no arguments"),
                                ));
                            }
                            if name == "returns_twice" {
                                result.returns_twice = Some(extension.span);
                            } else {
                                result.noreturn = Some(extension.span);
                                result.type_noreturn = self.unit.compiler == Compiler::Clang;
                                self.has_type_noreturn |= result.type_noreturn;
                            }
                        }
                        Some(crate::attributes::Attribute::Weak) => {
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    "weak takes no arguments",
                                ));
                            }
                            result.weak = Some(extension.span);
                        }
                        Some(crate::attributes::Attribute::Warning)
                        | Some(crate::attributes::Attribute::Error) => {
                            let kind = if name == "warning" {
                                crate::checked::DiagnosticAttributeKind::Warning
                            } else {
                                crate::checked::DiagnosticAttributeKind::Error
                            };
                            result.diagnostic_attributes.push(self.diagnostic_attribute(
                                attribute,
                                extension.span,
                                kind,
                            )?);
                        }
                        Some(crate::attributes::Attribute::DiagnoseIf)
                        | Some(crate::attributes::Attribute::EnableIf) => {
                            return Err(Error::new(
                                extension.span.start,
                                format!("call-constraint attribute `{name}` is unsupported"),
                            ));
                        }
                        Some(crate::attributes::Attribute::Mode) => {
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
                        Some(crate::attributes::Attribute::VectorSize) => {
                            let [value] = attribute.arguments.as_slice() else {
                                return Err(Error::new(
                                    extension.span.start,
                                    "vector_size requires one byte count",
                                ));
                            };
                            let bytes = self.eval(value)?.as_u64()?;
                            if result.vector_size.is_some_and(|old| old != bytes) {
                                return Err(Error::new(
                                    extension.span.start,
                                    "conflicting vector_size attributes",
                                ));
                            }
                            result.vector_size = Some(bytes);
                        }
                        Some(crate::attributes::Attribute::Packed) => {
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    "packed takes no arguments",
                                ));
                            }
                            result.packed = true;
                        }
                        Some(crate::attributes::Attribute::Aligned) => {
                            let value = match attribute.arguments.as_slice() {
                                [] => u64::from(self.unit.target.default_maximum_alignment()),
                                [value] => self
                                    .alignment_operand(|analyzer| analyzer.eval(value)?.as_u64())?,
                                _ => {
                                    return Err(Error::new(
                                        extension.span.start,
                                        "aligned attributes accept at most one alignment",
                                    ));
                                }
                            };
                            if value == 0 && self.unit.compiler == Compiler::Clang {
                                return Err(Error::new(
                                    extension.span.start,
                                    "aligned attributes require a positive power of two",
                                ));
                            }
                            set_alignment(result, value, extension.span.start)?;
                        }
                        Some(crate::attributes::Attribute::Aarch64VectorPcs)
                        | Some(crate::attributes::Attribute::Aarch64SvePcs) => {
                            if name == "aarch64_sve_pcs"
                                && matches!(
                                    self.unit.target,
                                    Target::Aarch64UnknownLinuxGnu
                                        | Target::Aarch64UnknownLinuxMusl
                                )
                                && self.unit.compiler == Compiler::Gnu
                            {
                                // GCC 13 does not implement this Clang attribute.
                                continue;
                            }
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    "AArch64 calling convention attributes take no arguments",
                                ));
                            }
                            let convention = if name == "aarch64_vector_pcs" {
                                CallingConvention::Aarch64Vector
                            } else {
                                CallingConvention::Aarch64Sve
                            };
                            convention
                                .for_target(self.unit.target)
                                .map_err(|mut error| {
                                    error.offset = extension.span.start;
                                    error
                                })?;
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
                        Some(crate::attributes::Attribute::Cdecl)
                        | Some(crate::attributes::Attribute::Stdcall)
                        | Some(crate::attributes::Attribute::Fastcall)
                        | Some(crate::attributes::Attribute::Thiscall)
                        | Some(crate::attributes::Attribute::MsAbi)
                        | Some(crate::attributes::Attribute::SysvAbi) => {
                            if self.unit.target == Target::I686UnknownLinuxGnu
                                && matches!(name, "stdcall" | "fastcall" | "thiscall")
                            {
                                return Err(Error::new(
                                    extension.span.start,
                                    format!(
                                        "{name} calling convention is unsupported on i686 GNU Linux"
                                    ),
                                ));
                            }
                            if !attribute.arguments.is_empty() {
                                return Err(Error::new(
                                    extension.span.start,
                                    "calling convention attributes take no arguments",
                                ));
                            }
                            if self.unit.target.is_armv7() {
                                // Clang accepts these x86 spellings on ARMv7
                                // but ignores them and uses the default PCS.
                                continue;
                            }
                            let convention = match name {
                                "ms_abi" => Some(CallingConvention::Win64),
                                // On i686 GNU Linux, sysv_abi names the default
                                // C ABI. Clang also ignores it on Windows ARM64.
                                "sysv_abi"
                                    if matches!(
                                        self.unit.target,
                                        Target::I686UnknownLinuxGnu | Target::Aarch64PcWindowsMsvc
                                    ) =>
                                {
                                    None
                                }
                                "sysv_abi" => Some(CallingConvention::SysV64),
                                // GNU ignores these x86-32 conventions on its
                                // 64-bit targets. Clang retains explicit cdecl.
                                "cdecl"
                                    if self.unit.compiler == Compiler::Clang
                                        && matches!(
                                            self.unit.target,
                                            Target::X86_64UnknownLinuxGnu
                                                | Target::X86_64UnknownLinuxMusl
                                                | Target::X86_64AppleDarwin
                                        ) =>
                                {
                                    Some(CallingConvention::SysV64)
                                }
                                "cdecl" if self.unit.target.is_windows() => {
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
                        Some(crate::attributes::Attribute::Ignored) => {}
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
                .unwrap_or_else(|| assertion.node.message.node.join(" "));
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
        if let Some(result) = self.floating_machine_mode(&ty, mode, offset)? {
            return Ok(result);
        }
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

/// Type attributes that attach to the next function boundary of a declarator.
fn is_calling_extension(extension: &ast::Extension) -> bool {
    match extension {
        ast::Extension::CallingConvention(_) => true,
        ast::Extension::Attribute(attribute) => matches!(
            attribute.name.node.trim_matches('_'),
            "cdecl"
                | "stdcall"
                | "fastcall"
                | "thiscall"
                | "ms_abi"
                | "sysv_abi"
                | "aarch64_vector_pcs"
                | "aarch64_sve_pcs"
        ),
        _ => false,
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

fn add_qualifier(
    result: &mut Qualifiers,
    atomic: &mut bool,
    qualifier: &Node<ast::TypeQualifier>,
) -> Result<(), Error> {
    match qualifier.node {
        ast::TypeQualifier::Const => result.is_const = true,
        ast::TypeQualifier::Volatile => result.is_volatile = true,
        ast::TypeQualifier::Restrict => result.is_restrict = true,
        ast::TypeQualifier::Unaligned => result.set_unaligned(true),
        ast::TypeQualifier::Atomic => *atomic = true,
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

#[derive(Clone, Copy)]
pub(crate) enum AnonymousRecordSpecifier<'a> {
    Direct,
    MicrosoftTag,
    /// The caller must resolve the unqualified written alias to a record.
    MicrosoftTypedef(&'a str),
}

/// Classifies source forms that can introduce an anonymous record member.
/// A Microsoft typedef still requires its original alias to denote a record;
/// qualifiers written beside the alias do not change that decision.
pub(crate) fn anonymous_record_specifier(
    specifiers: &[Node<ast::SpecifierQualifier>],
    target: Target,
) -> Option<AnonymousRecordSpecifier<'_>> {
    specifiers.iter().find_map(|specifier| {
        let ast::SpecifierQualifier::TypeSpecifier(ty) = &specifier.node else {
            return None;
        };
        match &ty.node {
            ast::TypeSpecifier::Struct(record) if record.node.identifier.is_none() => {
                Some(AnonymousRecordSpecifier::Direct)
            }
            ast::TypeSpecifier::Struct(_) if target.is_windows() => {
                Some(AnonymousRecordSpecifier::MicrosoftTag)
            }
            ast::TypeSpecifier::TypedefName(name) if target.is_windows() => {
                Some(AnonymousRecordSpecifier::MicrosoftTypedef(&name.node.name))
            }
            _ => None,
        }
    })
}
