use std::path::Path;

use toucan::{BindingOptions, Config, Target};

#[test]
fn complex_macros_report_storage_limits_and_allow_explicit_scalar_conversion() {
    let source = "#define IMAGINARY 2.0fi\n#define PAIR __builtin_complex(3.5,4.0)\n#define REAL (double)PAIR\n#define INTEGER (int)PAIR\n";
    for target in Target::ALL {
        let mut config = Config::new(target);
        config.preprocessor.allow_filesystem = false;
        config.preprocessor.defines.clear();
        let compilation = toucan::parse_source(Path::new("complex.h"), source, &config).unwrap();
        let (bindings, report) = compilation.bindings(&BindingOptions::default()).unwrap();
        assert_eq!(report.floating_macros, 1);
        assert_eq!(report.integer_macros, 1);
        assert!(bindings.contains("pub const REAL: ::core::primitive::f64"));
        assert!(bindings.contains("pub const INTEGER: ::core::primitive::i32 = 3"));
        assert_eq!(report.skipped_macros.len(), 2);
        assert!(report.skipped_macros.iter().all(|entry| {
            matches!(entry.name.as_str(), "IMAGINARY" | "PAIR")
                && entry.reason.contains("complex macro constants")
                && entry.reason.contains("Rust storage representation")
        }));
    }
}

#[test]
fn component_macros_emit_scalars_and_keep_clang_builtin_fold_boundaries() {
    for profile in toucan::CompilerProfile::ALL {
        let mut config = Config::with_profile(profile);
        config.preprocessor.allow_filesystem = false;
        config.preprocessor.defines.clear();
        let source = "#define REAL __real__ __builtin_complex(-0.0,4.0)\n#define IMAG __imag__ ~__builtin_complex(3.0,4.0)\n#define LIBRARY __builtin_cimag(__builtin_complex(3.0,4.0))\n";
        let compilation = toucan::parse_source(Path::new("parts.h"), source, &config).unwrap();
        let (bindings, report) = compilation.bindings(&BindingOptions::default()).unwrap();
        assert!(bindings.contains("pub const REAL: ::core::primitive::f64"));
        assert!(bindings.contains("pub const IMAG: ::core::primitive::f64"));
        if profile.compiler() == toucan::Compiler::Gnu {
            assert_eq!(report.floating_macros, 3);
            assert!(report.skipped_macros.is_empty());
        } else {
            assert_eq!(report.floating_macros, 2);
            assert_eq!(report.skipped_macros.len(), 1);
            assert_eq!(report.skipped_macros[0].name, "LIBRARY");
            assert!(
                report.skipped_macros[0]
                    .reason
                    .contains("frontend constant")
            );
        }
    }
}
