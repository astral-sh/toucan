extern crate toucan_parser;

use toucan_parser::arena::Arena;

use toucan_parser::ast::{Extension, FunctionSpecifier, PointerQualifier, TypeSpecifier};
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config, Flavor};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct InlineSpecifiers(Vec<Span>);

impl<'ast> Visit<'ast> for InlineSpecifiers {
    fn visit_function_specifier(
        &mut self,
        value: &'ast FunctionSpecifier,
        span: &'ast Span,
        _arena: &'ast Arena,
    ) {
        if *value == FunctionSpecifier::Inline {
            self.0.push(*span);
        }
    }
}

#[test]
fn microsoft_forceinline_is_an_inline_specifier_only_in_ms_mode() {
    let source = "__forceinline int __cdecl __ascii_tolower(int const c) { return c + 1; }";
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let mut specifiers = InlineSpecifiers::default();
    specifiers.visit_translation_unit(&parsed.unit, &parsed.arena);
    assert_eq!(
        specifiers
            .0
            .iter()
            .map(|span| &source[span.start..span.end])
            .collect::<Vec<_>>(),
        ["__forceinline"]
    );
    assert!(parse_preprocessed(&Config::with_clang(), source.into()).is_err());
}

#[derive(Default)]
struct Widths(Vec<(u8, Span)>);

impl<'ast> Visit<'ast> for Widths {
    fn visit_type_specifier(
        &mut self,
        value: &'ast TypeSpecifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        if let TypeSpecifier::MsvcInteger(width) = value {
            self.0.push((*width, *span));
        }
        visit::visit_type_specifier(self, value, span, arena);
    }
}

#[test]
fn microsoft_widths_preserve_spans_and_resource_limits() {
    let source = "typedef __int8 B; _int16 f(__int32 x){return (_int64)x;}";
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let mut widths = Widths::default();
    widths.visit_translation_unit(&parsed.unit, &parsed.arena);
    assert_eq!(
        widths
            .0
            .iter()
            .map(|(width, span)| (*width, &source[span.start..span.end]))
            .collect::<Vec<_>>(),
        [
            (8, "__int8"),
            (16, "_int16"),
            (32, "__int32"),
            (64, "_int64")
        ]
    );
    let limit = ParseLimits {
        max_work: parsed.statistics.work - 1,
        ..Default::default()
    };
    let error = parse_preprocessed_with_limits(&config, source.into(), limit).unwrap_err();
    assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    assert_eq!(
        parse_preprocessed(&config, source.into())
            .unwrap()
            .statistics,
        parsed.statistics
    );
    assert!(parse_preprocessed(&Config::with_clang(), source.into()).is_err());
}

#[derive(Default)]
struct PointerWidths(Vec<(u8, Span)>);

impl<'ast> Visit<'ast> for PointerWidths {
    fn visit_pointer_qualifier(
        &mut self,
        value: &'ast PointerQualifier,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        if let PointerQualifier::MsvcPointerWidth(width) = value {
            self.0.push((*width, *span));
        }
        visit::visit_pointer_qualifier(self, value, span, arena);
    }
}

#[test]
fn microsoft_pointer_widths_keep_their_spelling_and_work_limits() {
    let source = "typedef void * __ptr64 HANDLE64; typedef void * __ptr32 P32; void * __ptr32 to32(void *p) { return (void * __ptr32)p; }";
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let mut widths = PointerWidths::default();
    widths.visit_translation_unit(&parsed.unit, &parsed.arena);
    assert_eq!(
        widths
            .0
            .iter()
            .map(|(width, span)| (*width, &source[span.start..span.end]))
            .collect::<Vec<_>>(),
        [
            (64, "__ptr64"),
            (32, "__ptr32"),
            (32, "__ptr32"),
            (32, "__ptr32"),
        ]
    );
    assert_eq!(
        parse_preprocessed(&config, source.into())
            .unwrap()
            .statistics,
        parsed.statistics
    );
    let limits = ParseLimits {
        max_work: parsed.statistics.work - 1,
        ..Default::default()
    };
    assert_eq!(
        parse_preprocessed_with_limits(&config, source.into(), limits)
            .unwrap_err()
            .resource
            .unwrap()
            .kind,
        ResourceKind::Work
    );
    assert!(parse_preprocessed(&Config::with_clang(), source.into()).is_err());
}

#[derive(Default)]
struct Conventions(Vec<(String, Span)>);

impl<'ast> Visit<'ast> for Conventions {
    fn visit_extension(
        &mut self,
        extension: &'ast Extension,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        if let Extension::CallingConvention(attribute) = extension {
            self.0.push((attribute.name.node.clone(), *span));
        }
        visit::visit_extension(self, extension, span, arena);
    }
}

#[test]
fn calling_keywords_keep_their_written_spelling_and_span() {
    let source = "typedef int (__cdecl *F)(int); int (*_stdcall value)(int);";
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let mut conventions = Conventions::default();
    conventions.visit_translation_unit(&parsed.unit, &parsed.arena);
    assert_eq!(
        conventions
            .0
            .iter()
            .map(|(name, span)| (name.as_str(), &source[span.start..span.end]))
            .collect::<Vec<_>>(),
        [("__cdecl", "__cdecl"), ("_stdcall", "_stdcall")]
    );
    let core = Config {
        flavor: Flavor::StdC11,
        ..config
    };
    assert_eq!(
        parse_preprocessed(&core, source.into()).unwrap().unit,
        parsed.unit
    );
}

#[derive(Default)]
struct Declspecs(Vec<(String, Span, usize)>);

impl<'ast> Visit<'ast> for Declspecs {
    fn visit_extension(
        &mut self,
        extension: &'ast Extension,
        span: &'ast Span,
        arena: &'ast Arena,
    ) {
        if let Extension::Declspec(attribute) = extension {
            self.0.push((
                attribute.name.node.clone(),
                *span,
                attribute.arguments.len(),
            ));
        }
        visit::visit_extension(self, extension, span, arena);
    }
}

#[test]
fn declspec_attributes_keep_tag_placement_operands_spans_and_limits() {
    let source = "__declspec(,noinline,,noreturn,) void f(void); struct __declspec(align(16)) S {_declspec(align(8)) int x;}; enum __declspec(deprecated(\"old\")) E{A}; __declspec(\"first\" \"second\"(unknown)) int annotated;";
    let config = Config {
        extensions_msvc: true,
        ..Config::with_clang()
    };
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let mut attributes = Declspecs::default();
    attributes.visit_translation_unit(&parsed.unit, &parsed.arena);
    assert_eq!(
        attributes
            .0
            .iter()
            .map(|(name, span, argc)| (name.as_str(), source[span.start..span.end].trim(), *argc))
            .collect::<Vec<_>>(),
        [
            ("noinline", "noinline", 0),
            ("noreturn", "noreturn", 0),
            ("align", "align(16)", 1),
            ("align", "align(8)", 1),
            ("deprecated", "deprecated(\"old\")", 1),
            ("\"first\"", "\"first\"", 0),
            ("\"second\"", "\"second\"(unknown)", 1),
        ]
    );
    let error = parse_preprocessed_with_limits(
        &config,
        source.into(),
        ParseLimits {
            max_work: parsed.statistics.work - 1,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    assert_eq!(
        parse_preprocessed(&config, source.into())
            .unwrap()
            .statistics,
        parsed.statistics
    );
    assert!(parse_preprocessed(&Config::with_clang(), source.into()).is_err());
}
