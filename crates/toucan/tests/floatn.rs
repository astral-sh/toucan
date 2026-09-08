use std::path::Path;
use toucan::{BindingOptions, Compiler, CompilerProfile, Config, RustTarget};
#[test]
fn gnu_macro_constants_keep_exact_supported_encodings() {
    let source = "#define F32 (0.1f32)\n#define F64 (0.1f64)\n#define F32X (-0.0f32x)\n#define ROUND (1.0f32+0x1p-24f32)\n#define WIDE (1.0f64x)\n#define OLD_INT 7U\n#define OLD_TEXT \"yes\"\n";
    for p in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Gnu)
    {
        let mut config = Config::with_profile(p);
        config.preprocessor.defines.clear();
        let compilation = toucan::parse_source(Path::new("floatn.h"), source, &config).unwrap();
        for minor in [64, 83] {
            let (source, report) = compilation
                .bindings(&BindingOptions {
                    rust_target: RustTarget::stable(minor).unwrap(),
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(report.floating_macros, 4, "{report:?}");
            assert_eq!(report.integer_macros, 1);
            assert_eq!(report.string_macros, 1);
            assert!(source.contains("pub const F32: ::core::primitive::f32"));
            assert!(source.contains("pub const F64: ::core::primitive::f64"));
            assert!(source.contains("pub const F32X: ::core::primitive::f64"));
            for bits in [
                "0x3dcccccd",
                "0x3fb999999999999a",
                "0x8000000000000000",
                "0x3f800000",
            ] {
                assert!(source.contains(bits), "{source}");
            }
            assert_eq!(source.contains("::from_bits("), minor >= 83);
            assert!(
                report
                    .skipped_macros
                    .iter()
                    .any(|m| m.name == "WIDE" && m.reason.contains("_Float64x"))
            );
        }
    }
}
