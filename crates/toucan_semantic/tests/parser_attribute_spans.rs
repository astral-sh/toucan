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
