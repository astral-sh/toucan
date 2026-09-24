use std::path::Path;

use toucan::{BindingOptions, CompilerProfile, Config, MacroType, Target};

#[test]
fn bundled_limits_preserve_promoted_c_integer_types_in_bindings() {
    let header = r#"
#include <limits.h>
_Static_assert(_Generic(UCHAR_MAX, int: 1, default: 0), "unsigned char promotes to int");
_Static_assert(_Generic(USHRT_MAX, int: 1, default: 0), "unsigned short promotes to int");
_Static_assert(_Generic(CHAR_MAX, int: 1, default: 0), "plain char promotes to int");
_Static_assert(_Generic(UINT_MAX, unsigned int: 1, default: 0), "unsigned int stays unsigned");
"#;
    for profile in CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        config.preprocessor.allow_filesystem = false;
        let compilation = toucan::parse_source(Path::new("limits.h"), header, &config)
            .unwrap_or_else(|error| panic!("{profile:?}: {error}"));
        let (source, report) = compilation
            .bindings(&BindingOptions {
                allowlist: ["UCHAR_MAX", "USHRT_MAX", "CHAR_MAX", "UINT_MAX"]
                    .map(str::to_owned)
                    .to_vec(),
                ..BindingOptions::default()
            })
            .unwrap();
        let char_max = if profile.target().char_is_signed() {
            127
        } else {
            255
        };
        for declaration in [
            "pub const UCHAR_MAX: ::core::primitive::i32 = 255;".to_owned(),
            "pub const USHRT_MAX: ::core::primitive::i32 = 65535;".to_owned(),
            format!("pub const CHAR_MAX: ::core::primitive::i32 = {char_max};"),
            "pub const UINT_MAX: ::core::primitive::u32 = 4294967295;".to_owned(),
        ] {
            assert!(source.contains(&declaration), "{profile:?}: {source}");
        }
        assert_eq!(report.integer_macros, 4, "{profile:?}");
    }
}

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
