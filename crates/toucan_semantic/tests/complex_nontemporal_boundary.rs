use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile};

#[test]
fn complex_memory_hint_has_a_specific_implementation_boundary() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|p| p.compiler() == Compiler::Clang)
    {
        for call in [
            "(void)__builtin_nontemporal_load(p)",
            "__builtin_nontemporal_store(2.0, p)",
        ] {
            let source = format!("void f(_Complex double *p){{{call};}}");
            let plain = analyze_with_profile(&source, profile, &Default::default()).unwrap_err();
            let kept = analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!((plain.offset, &plain.message), (kept.offset, &kept.message));
            assert!(
                plain
                    .message
                    .contains("complex non-temporal accesses are unsupported"),
                "{plain}"
            );
        }
    }
}
