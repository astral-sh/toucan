use std::path::Path;

use toucan_preprocessor::{Config, OriginKind, Preprocessor};

const HEADER: &str = "#define ARM_ALIGN 1\n\
#define BOOST_ALIGN(x) x\n\
#pragma pack(push, BOOST_ALIGN(ARM_ALIGN))\n\
struct Packed { char lead; int value; };\n\
#pragma pack(pop)\n\
struct Normal { char lead; int value; };\n";

#[test]
fn raw_pack_operands_expand_only_with_microsoft_extensions() {
    for (ms_extensions, expected) in [
        (true, "#pragma pack ( push , 1 )\n"),
        (false, "#pragma pack ( push , BOOST_ALIGN ( ARM_ALIGN ) )\n"),
    ] {
        let result = Preprocessor::new(Config {
            ms_extensions,
            allow_filesystem: false,
            ..Config::default()
        })
        .preprocess_str(Path::new("packing.h"), HEADER)
        .unwrap();
        let start = result.source.find("#pragma pack").unwrap();
        assert!(
            result.source[start..].starts_with(expected),
            "{:?}",
            result.source
        );
        let origin = result
            .resolve_location(start + expected.find('1').unwrap_or(0))
            .unwrap();
        assert_eq!(origin.path.as_ref(), Path::new("packing.h"));
        assert_eq!(origin.line, 3);
        assert_eq!(origin.kind, OriginKind::Directive);
    }
}
