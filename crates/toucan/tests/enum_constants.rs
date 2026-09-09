use std::path::Path;

use toucan::{BindingOptions, Config, Target};

#[test]
fn enum_binding_projection_preserves_macro_integer_promotions() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;
    let compilation = toucan::parse_source(
        Path::new("enums.h"),
        "enum Flags { POSITIVE = 3 };\n#define ENUM_ALIAS POSITIVE\n#define ENUM_PROMOTION (-1 < POSITIVE)\n",
        &config,
    ).unwrap();
    let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(source.contains("pub const POSITIVE: ::core::primitive::u32 = 3;"));
    assert!(source.contains("pub const ENUM_ALIAS: ::core::primitive::i32 = 3;"));
    assert!(source.contains("pub const ENUM_PROMOTION: ::core::primitive::i32 = 1;"));
    assert_eq!(
        report.enum_constants[0].c_type.as_deref(),
        Some("enum Flags")
    );
    assert_eq!(report.enum_constants[0].emitted[0].c_expression_bits, 32);
    assert!(report.enum_constants[0].emitted[0].c_expression_signed);
}

#[test]
fn prototype_enumerators_do_not_replace_exported_constants() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;
    let compilation = toucan::parse_source(
        Path::new("enums.h"),
        "enum Flags { VALUE = 3 }; int f(enum Local { VALUE = -7, LOCAL = 2 } value);",
        &config,
    )
    .unwrap();
    let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(source.contains("pub const VALUE: ::core::primitive::u32 = 3;"));
    assert!(!source.contains("pub const LOCAL:"));
    assert_eq!(report.enum_constants.len(), 1);
    assert_eq!(
        report.enum_constants[0].c_type.as_deref(),
        Some("enum Flags")
    );
    assert_eq!(report.enum_constants[0].variants, ["VALUE"]);
}
