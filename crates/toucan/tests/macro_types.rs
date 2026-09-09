use std::path::Path;

use toucan::{BindingOptions, Config, MacroType, Target};

#[test]
fn unsigned_macro_policy_preserves_original_types_and_full_values() {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.defines.clear();
    let compilation = toucan::parse_source(Path::new("macros.h"), "#define SMALL 5LL\n#define BIG (1LL << 40)\n#define MAXIMUM 18446744073709551615ULL\n#define NEGATIVE -5LL\n", &config).unwrap();
    let (original, _) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(original.contains("pub const SMALL: ::core::primitive::i64 = 5;"));
    let (source, report) = compilation
        .bindings(&BindingOptions {
            macro_type: MacroType::Unsigned,
            ..BindingOptions::default()
        })
        .unwrap();
    assert!(source.contains("pub const SMALL: ::core::primitive::u32 = 5;"));
    assert!(source.contains("pub const BIG: ::core::primitive::u64 = 1099511627776;"));
    assert!(source.contains("pub const MAXIMUM: ::core::primitive::u64 = 18446744073709551615;"));
    assert!(source.contains("pub const NEGATIVE: ::core::primitive::i64 = -5;"));
    let small = report
        .macro_types
        .iter()
        .find(|item| item.c_name == "SMALL")
        .unwrap();
    assert_eq!((small.c_bits, small.c_signed), (64, true));
    assert_eq!((small.rust_bits, small.rust_signed), (32, false));
    assert_eq!(report.macro_types.len(), 2);
}
