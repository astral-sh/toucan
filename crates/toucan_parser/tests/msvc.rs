extern crate toucan_parser;

use toucan_parser::ast::{Extension, TypeSpecifier};
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config, Flavor};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[derive(Default)]
struct Widths(Vec<(u8, Span)>);

impl<'ast> Visit<'ast> for Widths {
    fn visit_type_specifier(&mut self, value: &'ast TypeSpecifier, span: &'ast Span) {
        if let TypeSpecifier::MsvcInteger(width) = value {
            self.0.push((*width, *span));
        }
        visit::visit_type_specifier(self, value, span);
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
    widths.visit_translation_unit(&parsed.unit);
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
struct Conventions(Vec<(String, Span)>);

impl<'ast> Visit<'ast> for Conventions {
    fn visit_extension(&mut self, extension: &'ast Extension, span: &'ast Span) {
        if let Extension::CallingConvention(attribute) = extension {
            self.0.push((attribute.name.node.clone(), *span));
        }
        visit::visit_extension(self, extension, span);
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
    conventions.visit_translation_unit(&parsed.unit);
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
    fn visit_extension(&mut self, extension: &'ast Extension, span: &'ast Span) {
        if let Extension::Declspec(attribute) = extension {
            self.0.push((
                attribute.name.node.clone(),
                *span,
                attribute.arguments.len(),
            ));
        }
        visit::visit_extension(self, extension, span);
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
    attributes.visit_translation_unit(&parsed.unit);
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
