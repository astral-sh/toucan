use toucan_semantic::checked::OccurrenceKind;
use toucan_semantic::{AnalysisOptions, analyze_with_options};
use toucan_target::Target;

#[test]
fn attribute_delimiters_keep_their_written_endpoints() {
    for attribute in [
        "__attribute__((unused))",
        "__attribute__ ( ( unused ) )",
        "__attribute__( (\n unused \t) \n)",
    ] {
        let declarator = format!("value {attribute}");
        let source = format!("const char *text = \"é𝄞\"; int {declarator} ;");
        let analysis = analyze_with_options(
            &source,
            Target::X86_64UnknownLinuxGnu,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let occurrence = analysis
            .checked()
            .unwrap()
            .occurrences()
            .map(|(_, occurrence)| occurrence)
            .find(|occurrence| {
                occurrence.kind() == OccurrenceKind::InitDeclarator
                    && source[occurrence.source().range().clone()].starts_with("value")
            })
            .unwrap();
        assert_eq!(&source[occurrence.source().range().clone()], declarator);
    }
}

#[test]
fn record_attribute_operands_keep_their_original_source_ranges() {
    for keyword in ["struct", "union", "enum"] {
        let body = if keyword == "enum" {
            "VALUE"
        } else {
            "char member;"
        };
        for operand in ["8", "sizeof(int)"] {
            let source =
                format!("{keyword} __attribute__ ( ( aligned({operand}) ) ) Record {{ {body} }};");
            let analysis = analyze_with_options(
                &source,
                Target::X86_64UnknownLinuxGnu,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let expressions: Vec<_> = analysis
                .checked()
                .unwrap()
                .occurrences()
                .map(|(_, occurrence)| occurrence)
                .filter(|occurrence| occurrence.kind() == OccurrenceKind::Expression)
                .map(|occurrence| &source[occurrence.source().range().clone()])
                .collect();
            assert!(expressions.contains(&operand), "{source}: {expressions:?}");
        }
    }
}

#[test]
fn nested_prefix_alignment_keeps_its_type_layer_and_original_operands() {
    use toucan_semantic::{TypeKind, analyze_with_profile};
    use toucan_target::{Compiler, CompilerProfile};

    const SOURCE: &str = "struct S{unsigned (__attribute__((aligned(sizeof(short)))) *pointer);unsigned (__attribute__((aligned(sizeof(short)))) value);};";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(
            SOURCE,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let unit = analysis.unit();
        let fields = unit
            .records
            .iter()
            .find(|record| record.name.as_deref() == Some("S"))
            .unwrap()
            .fields
            .as_ref()
            .unwrap();
        let TypeKind::Pointer(pointee) = &fields[0].ty.kind else {
            panic!("expected pointer")
        };
        let gnu = profile.compiler() == Compiler::Gnu;
        assert_eq!(
            pointee.alignment.bytes().map(|value| value.get()),
            gnu.then_some(2)
        );
        assert_eq!(fields[0].ty.alignment.bytes(), None);
        assert_eq!(
            fields[1].ty.alignment.bytes().map(|value| value.get()),
            gnu.then_some(2)
        );
        for field in fields {
            assert_eq!(field.alignment, (!gnu).then_some(2));
        }
        let operands = analysis
            .checked()
            .unwrap()
            .occurrences()
            .filter(|(_, occurrence)| occurrence.kind() == OccurrenceKind::Expression)
            .map(|(_, occurrence)| &SOURCE[occurrence.source().range().clone()])
            .filter(|operand| *operand == "sizeof(short)")
            .count();
        assert_eq!(operands, 2);
    }
}
