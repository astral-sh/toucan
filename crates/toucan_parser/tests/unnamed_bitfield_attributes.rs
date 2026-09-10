extern crate toucan_parser;

use toucan_parser::ast::{Extension, StructDeclarator};
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config};
use toucan_parser::limits::{ParseLimits, ResourceKind};
use toucan_parser::print::Printer;
use toucan_parser::span::Span;
use toucan_parser::visit::{self, Visit};

#[test]
fn unnamed_bitfield_attributes_retain_their_owner_and_source_spans() {
    const SOURCE: &str = "struct S{unsigned :0 __attribute__((aligned(sizeof(long)))), named:3 __attribute__((packed)), :0 __attribute__((aligned(16)));};";
    #[derive(Default)]
    struct Fields(Vec<(bool, Vec<String>)>);
    impl<'ast> Visit<'ast> for Fields {
        fn visit_struct_declarator(&mut self, field: &'ast StructDeclarator, span: &'ast Span) {
            let extensions = if let Some(declarator) = &field.declarator {
                assert!(field.extensions.is_empty());
                &declarator.node.extensions
            } else {
                &field.extensions
            };
            let attributes = extensions
                .iter()
                .map(|extension| SOURCE[extension.span.start..extension.span.end].to_owned())
                .collect();
            self.0.push((field.declarator.is_some(), attributes));
            visit::visit_struct_declarator(self, field, span);
        }

        fn visit_extension(&mut self, extension: &'ast Extension, span: &'ast Span) {
            if let Extension::Attribute(attribute) = extension {
                assert!(!attribute.name.node.is_empty());
                assert!(span.start < span.end);
            }
            visit::visit_extension(self, extension, span);
        }
    }
    for config in [Config::with_gcc(), Config::with_clang()] {
        let parsed = parse_preprocessed(&config, SOURCE.into()).unwrap();
        let mut fields = Fields::default();
        fields.visit_translation_unit(&parsed.unit);
        assert_eq!(
            fields.0,
            [
                (false, vec!["aligned(sizeof(long))".into()]),
                (true, vec!["packed".into()]),
                (false, vec!["aligned(16)".into()]),
            ]
        );
        let mut printed = String::new();
        Printer::new(&mut printed).visit_translation_unit(&parsed.unit);
        assert_eq!(printed.matches("Attribute").count(), 3);
        assert_eq!(printed.matches("SizeOfTy").count(), 1);
        let error = parse_preprocessed_with_limits(
            &config,
            SOURCE.into(),
            ParseLimits {
                max_work: parsed.statistics.work - 1,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    }
}
