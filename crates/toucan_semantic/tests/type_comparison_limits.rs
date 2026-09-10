use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

fn callbacks(depth: usize) -> String {
    let mut source = String::from("typedef int T0;\n");
    for level in 1..=depth {
        let previous = level - 1;
        source.push_str(&format!(
            "typedef void (*T{level})(T{previous}, T{previous});\n"
        ));
    }
    source
}

#[test]
fn branching_callback_comparisons_have_a_work_limit() {
    // The syntax and type depth are small, but expanding both parameters at
    // each level would compare more than sixteen million leaves.
    let prefix = callbacks(24);
    for suffix in [
        "extern T24 value; extern T24 value;",
        "typedef T24 Alias; typedef T24 Alias;",
        "_Static_assert(__builtin_types_compatible_p(T24, T24), \"same\");",
    ] {
        for compiler in [Compiler::Gnu, Compiler::Clang] {
            let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler).unwrap();
            let source = format!("{prefix}{suffix}");
            let error =
                analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap_err();
            assert!(
                error
                    .message
                    .contains("type comparison work limit exceeded"),
                "{compiler:?}: {suffix}: {error}"
            );
        }
    }
}

#[test]
fn ordinary_callback_comparisons_keep_their_compatibility_rules() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler).unwrap();
        let source = format!(
            "{}\nextern T8 value; extern T8 value;\n\
             typedef T8 Alias; typedef T8 Alias;\n\
             _Static_assert(__builtin_types_compatible_p(T8, T8), \"same\");\n\
             _Static_assert(!__builtin_types_compatible_p(T8, T7), \"different\");",
            callbacks(8)
        );
        for retain_code in [false, true] {
            analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code,
                    ..Default::default()
                },
            )
            .unwrap();
        }
    }
}
