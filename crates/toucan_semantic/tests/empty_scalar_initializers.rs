use toucan_semantic::{
    Analysis, AnalysisOptions, ArithmeticConstant, analyze_with_profile, checked::InitializerKind,
    evaluate_arithmetic,
};
use toucan_target::{Compiler, CompilerProfile, LanguageMode};

const SOURCE: &str = r#"
_Bool boolean = {};
int integer = {};
const unsigned long qualified = {{{}}};
enum E { Nonzero = 7 }; enum E enumeration = {};
float single = {}; double real = {}; long double extended = {};
double _Complex complex = {};
int *pointer = {}; int (*function_pointer)(void) = {};
struct S { int x; int *p; }; struct S record = {.x = {}, .p = {}};
int elements[2] = {{}, {}};
int literal = (int){}; double nested = (double){{{}}};
int *null_literal = (int*){};
int local(void) { int zero = {}; return zero + (int){}; }
"#;

fn check(source: &str, profile: CompilerProfile) -> Result<Analysis, toucan_semantic::Error> {
    let plain = analyze_with_profile(source, profile, &Default::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&plain, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(format!("{:?}", a.unit()), format!("{:?}", b.unit())),
        (Err(a), Err(b)) => assert_eq!((&a.message, a.offset), (&b.message, b.offset)),
        _ => panic!("{profile:?}: {source}: {plain:?} {retained:?}"),
    }
    retained
}

#[test]
fn empty_scalars_keep_typed_zero_fill_and_real_source_occurrences() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let analysis = check(SOURCE, profile.with_language_mode(mode)).unwrap();
            let code = analysis.checked().unwrap();
            let mut empty = 0;
            for (_, initializer) in code.initializers() {
                if let InitializerKind::List {
                    entries,
                    zero_fill_unwritten,
                    union_member,
                } = initializer.kind()
                    && entries.is_empty()
                {
                    assert!(*zero_fill_unwritten);
                    assert!(union_member.is_none());
                    assert!(code.ty(initializer.ty()).is_some());
                    let occurrence = code.occurrence(initializer.occurrence()).unwrap();
                    assert!(!occurrence.source().synthetic());
                    assert!(
                        SOURCE[occurrence.source().range()]
                            .trim_end()
                            .ends_with("{}")
                    );
                    empty += 1;
                }
            }
            assert_eq!(empty, 19);
        }
    }
}

#[test]
fn arithmetic_queries_return_positive_zero_in_the_destination_format() {
    for profile in CompilerProfile::ALL {
        let analysis = check("enum E { Nonzero = 7 };", profile).unwrap();
        for ty in [
            "_Bool",
            "int",
            "unsigned long",
            "enum E",
            "float",
            "double",
            "long double",
            "double _Complex",
        ] {
            for braces in ["{}", "{{{}}}"] {
                let value = evaluate_arithmetic(analysis.unit(), &format!("({ty}){braces}"))
                    .unwrap_or_else(|error| panic!("{profile:?} {ty}{braces}: {error}"));
                match value {
                    ArithmeticConstant::Integer(value) => assert_eq!(value.value, 0),
                    ArithmeticConstant::Floating(value) => assert_eq!(value.to_bits(), 0),
                    ArithmeticConstant::Complex(value) => {
                        assert_eq!(value.real().to_bits(), 0);
                        assert_eq!(value.imaginary().to_bits(), 0);
                    }
                }
            }
        }
    }
}

#[test]
fn atomic_braces_follow_the_compiler_and_invalid_destinations_stay_rejected() {
    for profile in CompilerProfile::ALL {
        for source in [
            "_Atomic(int) x={};",
            "_Atomic(int) x={{}};",
            "_Atomic(int) x={0};",
            "_Atomic(int*) p={};",
            "struct S{int n;};_Atomic(struct S) x={};",
        ] {
            let result = check(source, profile);
            assert_eq!(
                result.is_ok(),
                profile.compiler() == Compiler::Gnu,
                "{profile:?}: {source}: {result:?}"
            );
            if let Ok(analysis) = result {
                let code = analysis.checked().unwrap();
                let site = code
                    .declarations()
                    .find(|(_, site)| site.initializer().is_some())
                    .unwrap()
                    .1;
                let initializer = code.initializer(site.initializer().unwrap()).unwrap();
                assert!(
                    analysis
                        .unit()
                        .atomic_value(code.ty(initializer.ty()).unwrap())
                        .unwrap()
                        .is_some()
                );
            }
        }
        for source in [
            "void x={};",
            "struct S;struct S x={};",
            "int f(void)={};",
            "int x={.member={}};",
            "int x={0,1};",
            "int f(int n){int a[n]={};return a[0];}",
        ] {
            assert!(check(source, profile).is_err(), "{profile:?}: {source}");
        }
    }
    let profile = CompilerProfile::ALL[0];
    let braces = format!("{}{}", "{".repeat(140), "}".repeat(140));
    for retain_code in [false, true] {
        let error = analyze_with_profile(
            &format!("int x={braces};"),
            profile,
            &AnalysisOptions {
                retain_code,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.message.contains("nesting limit exceeded"));
    }
    let analysis = check("", profile).unwrap();
    assert!(evaluate_arithmetic(analysis.unit(), &format!("(int){braces}")).is_err());
}

#[test]
#[ignore = "requires native GCC/Clang; checks source decisions and executes zero-value probes"]
fn native_empty_scalars_have_zero_values() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("empty.c");
    for (compiler, variable, fallback) in [
        (Compiler::Gnu, "TOUCAN_GCC", "gcc"),
        (Compiler::Clang, "TOUCAN_CLANG", "clang"),
    ] {
        if compiler == Compiler::Gnu && !cfg!(target_os = "linux") {
            continue;
        }
        let cc = std::env::var(variable).unwrap_or_else(|_| fallback.into());
        for mode in LanguageMode::ALL {
            for (source, expected) in [
                (SOURCE, true),
                ("_Atomic(int) x={};", compiler == Compiler::Gnu),
                ("_Atomic(int) x={{}};", compiler == Compiler::Gnu),
                ("_Atomic(int) x={0};", compiler == Compiler::Gnu),
                ("_Atomic(int*) x={};", compiler == Compiler::Gnu),
                ("void x={};", false),
                ("struct S;struct S x={};", false),
                ("int x={.member={}};", false),
            ] {
                std::fs::write(&file, source).unwrap();
                let output = Command::new(&cc)
                    .arg(format!("-std={mode}"))
                    .arg("-fsyntax-only")
                    .arg(&file)
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output).unwrap(),
                    expected,
                    "{cc} {mode}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let main = r#"
int main(void) {
    unsigned long long bits = 1;
    __builtin_memcpy(&bits, &real, sizeof(real));
    if (bits || boolean || integer || qualified || enumeration || single || extended) return 1;
    if (__builtin_signbit(single) || __builtin_signbit(real) || __builtin_signbit(extended)) return 2;
    if (__real__ complex || __imag__ complex || __builtin_signbit(__real__ complex) || __builtin_signbit(__imag__ complex)) return 3;
    if (pointer || function_pointer || record.x || record.p || elements[0] || elements[1]) return 4;
    if (literal || nested || null_literal || local()) return 5;
    return 0;
}
"#;
            std::fs::write(&file, format!("{SOURCE}{main}")).unwrap();
            for optimization in ["-O0", "-O2"] {
                let binary = dir.path().join("empty");
                let output = Command::new(&cc)
                    .arg(format!("-std={mode}"))
                    .arg(optimization)
                    .args(["-fsanitize=undefined", "-fno-sanitize-recover=all"])
                    .arg(&file)
                    .arg("-o")
                    .arg(&binary)
                    .output()
                    .unwrap();
                assert!(
                    toucan_test_support::compiler_acceptance(&output).unwrap(),
                    "{cc} {mode}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let output = Command::new(&binary).output().unwrap();
                assert!(output.status.success(), "{cc} {mode}: {output:?}");
            }
        }
    }
}
