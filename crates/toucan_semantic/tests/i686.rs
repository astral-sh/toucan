use toucan_semantic::{
    AnalysisOptions, CallingConvention, TypeKind, analyze_with_profile, evaluate_integer,
    has_attribute,
};
use toucan_target::{Compiler, CompilerProfile, Target};

#[test]
fn i686_integer_layout_and_wide_literals_follow_the_compiler() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        let wchar = if compiler == Compiler::Gnu {
            "long int"
        } else {
            "int"
        };
        let atomic_alignment = if compiler == Compiler::Gnu { 16 } else { 4 };
        let component_alignment = if compiler == Compiler::Gnu { 8 } else { 4 };
        let source = format!(
            "struct S {{ char c; double d; long n; }}; \
             _Static_assert(sizeof(struct S)==16 && _Alignof(struct S)==4,\"record\"); \
             struct Aggregate {{ int values[4]; }}; \
             _Static_assert(_Alignof(_Atomic(struct Aggregate))=={atomic_alignment},\"atomic aggregate\"); \
             _Static_assert(_Alignof(_Atomic(double _Complex))=={atomic_alignment},\"atomic complex\"); \
             _Static_assert(_Generic(L'A',{wchar}:1,default:0),\"wide character type\"); \
             _Static_assert(_Generic(&(L\"A\")[0],{wchar}*:1,default:0),\"wide string type\"); \
             _Static_assert(_Alignof(double)==4 && __alignof__(double)==8,\"double alignments\"); \
             _Static_assert(_Generic(sizeof(void*),unsigned int:1,default:0),\"size_t type\"); \
             double _Complex value; \
             _Static_assert(_Alignof(__real__ value)=={component_alignment},\"real component alignment\"); \
             _Static_assert(__alignof__(__real__ value)==8,\"preferred component alignment\");"
        );
        let analysis = analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap();
        for (expression, expected) in [
            ("sizeof(void*)", 4),
            ("sizeof(long)", 4),
            ("__builtin_offsetof(struct S,d)", 4),
            ("__builtin_offsetof(struct S,n)", 12),
            ("sizeof(long double)", 12),
            ("_Alignof(long double)", 4),
        ] {
            assert_eq!(
                evaluate_integer(analysis.unit(), expression).unwrap().value,
                expected,
                "{compiler}: {expression}"
            );
        }
        let error = analyze_with_profile("__int128 value;", profile, &AnalysisOptions::default())
            .unwrap_err();
        assert!(error.message.contains("unavailable"), "{compiler}: {error}");
    }
}

#[test]
fn i686_nondefault_calling_conventions_do_not_become_c_abi() {
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let profile = CompilerProfile::new(Target::I686UnknownLinuxGnu, compiler).unwrap();
        assert_eq!(has_attribute(profile, "sysv_abi"), 1);
        let analysis = analyze_with_profile(
            "int __attribute__((sysv_abi)) f(int); int f(int);",
            profile,
            &AnalysisOptions::default(),
        )
        .unwrap();
        let function = analysis.unit().declarations[0].ty.clone();
        let TypeKind::Function(function) = function.kind else {
            panic!("expected function type");
        };
        assert_eq!(function.calling_convention, CallingConvention::C);
        analyze_with_profile(
            "int __attribute__((cdecl)) f(int);",
            profile,
            &AnalysisOptions::default(),
        )
        .unwrap();
        for convention in ["stdcall", "fastcall", "thiscall"] {
            let source = format!("int __attribute__(({convention})) f(int);");
            let error =
                analyze_with_profile(&source, profile, &AnalysisOptions::default()).unwrap_err();
            assert!(error.message.contains(convention), "{compiler}: {error}");
        }
    }
}
