use toucan_semantic::{
    AnalysisOptions, ArithmeticConstant, analyze_with_profile, evaluate_arithmetic,
    evaluate_integer,
};
use toucan_target::{Compiler, CompilerProfile, Target};

fn profile(compiler: Compiler) -> CompilerProfile {
    CompilerProfile::new(Target::X86_64UnknownLinuxGnu, compiler).unwrap()
}

#[test]
fn clang_initializer_reads_keep_destination_conversions_and_retention_parity() {
    let source = r#"
        static const int self_query = __builtin_constant_p(self_query);
        static const int first = 7;
        const int second = first + 1;
        const int object = second * 2;
        static const unsigned char narrow = 255;
        const signed char converted = narrow;
        static const float rounded = 0.1f;
        const double sum = rounded + 0.5;
        static const _Bool flag = 3;
        const int truth = flag + 2;
        enum E { V = 9 }; static const enum E e = V;
        const int enumerated = e;
        static const _Atomic(int) atomic = 11;
        const int atomic_read = atomic;
        static _Thread_local const int tls = 13;
        const int tls_read = tls;
        const int query = __builtin_constant_p(first);
        const int parenthesized = ((first));
        const int conditional = first ? first + 1 : 2;
    "#;
    let plain = analyze_with_profile(
        source,
        profile(Compiler::Clang),
        &AnalysisOptions::default(),
    )
    .unwrap();
    for retain_code in [false, true] {
        let kept = analyze_with_profile(
            source,
            profile(Compiler::Clang),
            &AnalysisOptions {
                retain_object_values: true,
                retain_code,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{:?}", plain.unit()), format!("{:?}", kept.unit()));
        let values = kept.object_values().unwrap().entries();
        for (name, expected) in [
            ("self_query", 0),
            ("object", 16),
            ("converted", -1),
            ("truth", 3),
            ("enumerated", 9),
            ("atomic_read", 11),
            ("tls_read", 13),
            ("query", 1),
            ("parenthesized", 7),
            ("conditional", 8),
        ] {
            let value = values.iter().find(|v| v.name() == name).unwrap();
            let Some(ArithmeticConstant::Integer(value)) = value.value() else {
                panic!("{name}")
            };
            assert_eq!(value.signed_value(), expected, "{name}");
        }
        let sum = values.iter().find(|v| v.name() == "sum").unwrap();
        let Some(ArithmeticConstant::Floating(value)) = sum.value() else {
            panic!()
        };
        assert_eq!(
            value.to_bits(),
            u128::from((f64::from(0.1f32) + 0.5).to_bits())
        );
        assert_eq!(
            values.iter().find(|v| v.name() == "e").unwrap().value(),
            None
        );
    }
    assert!(!plain.unit().constants.contains_key("first"));
    assert!(evaluate_integer(plain.unit(), "first").is_err());
    assert!(evaluate_arithmetic(plain.unit(), "first").is_err());
}

#[test]
fn definition_order_linkage_and_lexical_shadowing_control_read_eligibility() {
    for source in [
        "extern const int first; const int first=7; const int object=first;",
        "const int first=7; extern const int first; const int object=first;",
        "extern const int first=7; const int object=first;",
        "typedef const int Number; static Number first=7; const int object=first;",
        "static const int first=7; int f(void){static const int object=first; return object;}",
        "const int first=7; int f(void){extern const int first; static const int object=first; return object;}",
        "const int first=7; int f(void){enum {first=3}; static const int object=first; return object;}",
    ] {
        for retain_code in [false, true] {
            analyze_with_profile(
                source,
                profile(Compiler::Clang),
                &AnalysisOptions {
                    retain_code,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }
    for source in [
        "extern const int first; const int object=first; const int first=7;",
        "const int first; const int object=first;",
        "static int first=7; const int object=first;",
        "static volatile const int first=7; const int object=first;",
        "static const volatile _Atomic(int) first=7; const int object=first;",
        "static _Atomic(int) first=7; const int object=first;",
        "typedef volatile const int Number; Number first=7; const int object=first;",
        "const int first __attribute__((weak))=7; const int object=first;",
        "extern const int first __attribute__((weak)); const int first=7; const int object=first;",
        "static const int object=object;",
        "int f(void); static const int first=f(); const int object=first;",
        "static int first=7; const int object=(first=8);",
        "static int first=7; const int object=(first++,first);",
        "static const int first=7; int f(int first){static const int object=first; return object;}",
        "static const int first=7; int f(void){int first=2;static const int object=first;return object;}",
    ] {
        for retain_code in [false, true] {
            assert!(
                analyze_with_profile(
                    source,
                    profile(Compiler::Clang),
                    &AnalysisOptions {
                        retain_code,
                        ..Default::default()
                    }
                )
                .is_err(),
                "{source}"
            );
        }
    }
}

#[test]
fn initializer_values_preserve_identifier_ice_rules_and_other_profiles() {
    for source in [
        "static const int first=7; enum E {V=first};",
        "static const int first=7; _Static_assert(first==7,\"extension\");",
        "static const int first=7; int object[first];",
        "static const int first=7; static int object=first; enum E {V=first};",
    ] {
        assert!(
            analyze_with_profile(
                source,
                profile(Compiler::Clang),
                &AnalysisOptions::default()
            )
            .is_err(),
            "{source}"
        );
    }
    // The query builtin has separately verified constant knowledge. Its result
    // is an ICE even though a direct read of the object is not one here.
    let source = "static const int first=7; static const int object=first; enum E {V=__builtin_constant_p(first)};";
    let unit = analyze_with_profile(
        source,
        profile(Compiler::Clang),
        &AnalysisOptions::default(),
    )
    .unwrap()
    .into_unit();
    assert_eq!(unit.constants["V"].value, 1);
    for source in [
        "static const int first=7; static const int object=first+1;",
        "static const _Atomic(int) first=7; const int object=first;",
    ] {
        assert!(
            analyze_with_profile(source, profile(Compiler::Gnu), &AnalysisOptions::default())
                .is_err(),
            "{source}"
        );
    }
    // Existing weak-attribute placement rules are independent of value capture.
    assert!(analyze_with_profile(
        "const int first=7; extern const int first __attribute__((weak)); const int object=first;",
        profile(Compiler::Clang), &AnalysisOptions::default(),
    ).is_err());
    // Address copies, aggregate elements, and local const definitions are separate work.
    for source in [
        "static const char *const first=\"abc\"; static const char *const object=first;",
        "static const int first[1]={7}; const int object=first[0];",
        "static const struct S {int n;} first={7}; const int object=first.n;",
        "int f(void){const int first=7; static const int object=first; return object;}",
    ] {
        assert!(
            analyze_with_profile(
                source,
                profile(Compiler::Clang),
                &AnalysisOptions::default()
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn clang_constant_queries_agree_before_cached_selection_and_in_function_bodies() {
    for source in [
        "static const int first=7; const int object=__builtin_choose_expr(__builtin_constant_p(first),first,5);",
        "static const int first=7; const int object=__builtin_constant_p(first)?first:5;",
        "static const int first=7; const int object=_Generic(first,int:first,default:5);",
    ] {
        let analysis = analyze_with_profile(
            source,
            profile(Compiler::Clang),
            &AnalysisOptions {
                retain_object_values: true,
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        let value = analysis.object_values().unwrap().entries().last().unwrap();
        let Some(ArithmeticConstant::Integer(value)) = value.value() else {
            panic!()
        };
        assert_eq!(value.signed_value(), 7, "{source}");
    }
    for source in [
        "static const int first=7; const int object=sizeof(enum{V=__builtin_constant_p(first)}); _Static_assert(V==1,\"query\");",
        "static const int first=7; int f(void){_Static_assert(__builtin_constant_p(first)==1,\"query\");return 0;}",
        "static const _Atomic(int) first=7; enum E{V=__builtin_constant_p(first)}; _Static_assert(V==1,\"query\");",
        "static const int first=7; int f(int first){_Static_assert(__builtin_constant_p(first)==0,\"query\");return 0;}",
        "static volatile const int first=7; enum E{V=__builtin_constant_p(first)}; _Static_assert(V==0,\"query\");",
        "const int first __attribute__((weak))=7; enum E{V=__builtin_constant_p(first)}; _Static_assert(V==0,\"query\");",
    ] {
        for retain_code in [false, true] {
            analyze_with_profile(
                source,
                profile(Compiler::Clang),
                &AnalysisOptions {
                    retain_code,
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }
}
