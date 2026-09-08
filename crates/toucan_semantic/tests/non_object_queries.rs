use std::process::Command;

use toucan_semantic::checked::{ExprKind, StatementKind, UseContext, ValueCategory};
use toucan_semantic::{AnalysisOptions, Type, TypeKind, analyze_with_profile, evaluate_integer};
use toucan_target::{Compiler, CompilerProfile, Target};

fn source(profile: CompilerProfile) -> String {
    let function = if profile.compiler() == Compiler::Gnu && profile.target().is_x86_64() {
        1
    } else {
        4
    };
    let void_alias = if profile.compiler() == Compiler::Clang {
        32
    } else {
        1
    };
    let function_alias = if profile.compiler() == Compiler::Clang {
        32
    } else {
        function
    };
    let low_function = if profile.compiler() == Compiler::Gnu {
        function
    } else {
        1
    };
    let mut source = String::from(
        "typedef void V; typedef void VA __attribute__((aligned(32))); typedef void F(void); typedef void FA(void) __attribute__((aligned(32))); void f(void); FA fa; FA explicit_function __attribute__((aligned(8))); void low(void) __attribute__((aligned(1))); void *vp; F *fp; FA *fap; void (*factory(void))(void);\n",
    );
    for (operand, size, alignment) in [
        ("void", 1, 1),
        ("const volatile void", 1, 1),
        ("V", 1, 1),
        ("VA", 1, void_alias),
        ("(VA)0", 1, void_alias),
        ("(void)0", 1, 1),
        ("*vp", 1, 1),
        ("f()", 1, 1),
        ("F", 1, function),
        ("f", 1, function),
        ("void(void)", 1, function),
        ("*fp", 1, function),
        ("*factory()", 1, function),
        ("FA", 1, function_alias),
        ("fa", 1, function_alias),
        ("*fap", 1, function_alias),
        ("explicit_function", 1, 8),
        ("__typeof__(explicit_function)", 1, function_alias),
        ("low", 1, low_function),
        ("fp", 8, 8),
        ("fap", 8, 8),
        ("(1,f)", 8, 8),
        ("__builtin_prefetch((void*)0)", 1, 1),
        ("__builtin_free((void*)0)", 1, 1),
    ] {
        for (query, expected) in [
            ("sizeof", size),
            ("_Alignof", alignment),
            ("__alignof", alignment),
            ("__alignof__", alignment),
        ] {
            source.push_str(&format!("_Static_assert({query}({operand})=={expected},\"{query} {operand}\");\n_Static_assert(__builtin_types_compatible_p(__typeof__({query}({operand})),__typeof__(sizeof(void*))),\"size_t\");\n"));
        }
    }
    if profile.compiler() == Compiler::Gnu {
        source.push_str("_Static_assert(sizeof(__builtin_prefetch)==1 && sizeof(__builtin_malloc)==1,\"builtin function type\");\n");
    }
    source
}

fn compare(source: &str, profile: CompilerProfile) {
    let ordinary = analyze_with_profile(source, profile, &Default::default()).unwrap();
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(ordinary.unit()).unwrap(),
        serde_json::to_value(retained.unit()).unwrap()
    );
}

#[test]
fn non_object_queries_preserve_compiler_values_and_size_type() {
    for profile in CompilerProfile::ALL {
        for mode in toucan_target::LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            compare(&source(profile), profile);
            let analysis = analyze_with_profile(
                "typedef void V;typedef void F(void);",
                profile,
                &Default::default(),
            )
            .unwrap();
            assert_eq!(
                evaluate_integer(analysis.unit(), "sizeof(V)+sizeof(F)")
                    .unwrap()
                    .value,
                2
            );
            assert!(analysis.unit().layout(&Type::new(TypeKind::Void)).is_err());
            assert!(
                analysis
                    .unit()
                    .layout(&analysis.unit().typedefs["F"])
                    .is_err()
            );
        }
    }
}

#[test]
fn void_lvalues_do_not_acquire_loads_and_size_operands_stay_unevaluated() {
    let source = "void action(void);void f(volatile void*p){*p;(void)*p;sizeof(*p);sizeof(action());_Alignof(*p);__alignof__(action());}";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(
            source,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let code = analysis.checked().unwrap();
        let mut discarded = 0;
        for (_, statement) in code.statements() {
            if let StatementKind::Expression(Some(value)) = statement.kind() {
                let expression = code.expression(value.expression()).unwrap();
                if expression.category() == ValueCategory::ObjectLvalue {
                    assert!(matches!(
                        code.ty(expression.ty()).unwrap().kind,
                        TypeKind::Void
                    ));
                    assert!(value.conversions().is_empty());
                    discarded += 1;
                }
            }
        }
        assert_eq!(discarded, 1);
        let mut sizes = 0;
        for (_, expression) in code.expressions() {
            match expression.kind() {
                ExprKind::SizeOfValue { operand, variable } => {
                    assert!(!variable);
                    assert_eq!(operand.context(), UseContext::Unevaluated);
                    assert!(operand.conversions().is_empty());
                    sizes += 1;
                }
                ExprKind::Cast { value, .. }
                    if code.expression(value.expression()).unwrap().category()
                        == ValueCategory::ObjectLvalue =>
                {
                    assert!(
                        value
                            .conversions()
                            .iter()
                            .all(|conversion| conversion.kind()
                                != toucan_semantic::checked::Conversion::Lvalue)
                    );
                }
                _ => {}
            }
        }
        assert_eq!(sizes, 2);
    }
}

#[test]
fn size_extensions_do_not_complete_objects_or_enable_pointer_arithmetic() {
    for profile in CompilerProfile::ALL {
        for source in [
            "int x[sizeof(struct Missing)];",
            "int x[sizeof(int[])];",
            "struct S{int field:2;};int f(struct S s){return sizeof(s.field);}",
            "void f(void*p){p++;}",
            "void f(void(*p)(void)){p++;}",
            "void f(void*p){*p=(void)0;}",
            "void f(void*p){int x=*p;}",
            "typedef void V __attribute__((aligned(32))); V object;",
            "typedef void F(void) __attribute__((aligned(32))); F array[2];",
        ] {
            assert!(
                analyze_with_profile(source, profile, &Default::default()).is_err(),
                "{profile:?}: {source}"
            );
        }
        if profile.compiler() == Compiler::Clang {
            assert!(
                analyze_with_profile(
                    "int x[sizeof(__builtin_prefetch)];",
                    profile,
                    &Default::default()
                )
                .is_err()
            );
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn native_non_object_queries_and_effects() {
    let temp = tempfile::tempdir().unwrap();
    // This header-free fixture uses the host architecture's query rules and
    // exercises no libc or operating-system-specific ABI surface.
    let architecture = if cfg!(target_arch = "aarch64") {
        Target::Aarch64UnknownLinuxGnu
    } else {
        Target::X86_64UnknownLinuxGnu
    };
    for (compiler, command) in [(Compiler::Gnu, "gcc"), (Compiler::Clang, "clang")] {
        for mode in toucan_target::LanguageMode::ALL {
            let profile = CompilerProfile::new(architecture, compiler)
                .unwrap()
                .with_language_mode(mode);
            let mut source = source(profile);
            source.push_str("int main(void){int n=2;void*p=0;unsigned long long a=sizeof((void)n++),b=sizeof(*p),c=sizeof(int(int[n++])),d=sizeof((void)*(int(*)[n++])0);return n!=2||a!=1||b!=1||c!=1||d!=1;}\n");
            let input = temp.path().join("query.c");
            let binary = temp.path().join("query");
            std::fs::write(&input, &source).unwrap();
            let output = Command::new(command)
                .arg(format!("-std={mode}"))
                .arg(&input)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                toucan_test_support::compiler_acceptance(&output).unwrap(),
                "{command} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(&binary).status().unwrap().success());
            compare(&source, profile);
        }
    }
}
