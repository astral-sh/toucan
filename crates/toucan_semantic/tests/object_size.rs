use toucan_semantic::checked::{BoundEvaluation, Builtin, ExprKind, UseContext};
use toucan_semantic::{
    AnalysisOptions, IntegerKind, TypeKind, analyze, analyze_with_options, evaluate_integer,
};
use toucan_target::Target;

const NAMES: [&str; 2] = ["__builtin_object_size", "__builtin_dynamic_object_size"];
const VALID: &[&str] = &[
    "unsigned long long f(void *p) {return NAME(p,0);}",
    "unsigned long long f(const int *p) {return NAME(p,1);}",
    "unsigned long long f(void) {char a[4]; return NAME(a,2);}",
    "unsigned long long f(void) {return NAME(\"text\",3);}",
    "unsigned long long f(void) {return NAME(0,0x100000000ULL);}",
    "unsigned long long f(void *p) {return NAME(p,(int)1.0);}",
    "unsigned long long f(void *p) {return NAME(p,1U+1U);}",
    "unsigned long long f(int n) {char a[n++]; return NAME(a,0);}",
    "unsigned long long f(char *p) {return NAME(p++,0);}",
    "int f(int (*NAME)(int,int)) {return NAME(1,2);}",
];
const INVALID: &[&str] = &[
    "unsigned long long f(void *p) {return NAME(p);}",
    "unsigned long long f(void *p) {return NAME(p,0,0);}",
    "unsigned long long f(void) {return NAME(1,0);}",
    "unsigned long long f(void) {return NAME((void)1,0);}",
    "unsigned long long f(const volatile int *p) {return NAME(p,0);}",
    "unsigned long long f(void *p) {return NAME(p,-1);}",
    "unsigned long long f(void *p) {return NAME(p,4);}",
    "unsigned long long f(void *p,int mode) {return NAME(p,mode);}",
    "unsigned long long f(void *p) {return NAME(p,1/0);}",
    "void f(void *p) {NAME(p,0)=1;}",
];
const GNU_MODES: &[&str] = &[
    "unsigned long long f(void *p) {return NAME(p,1.9);}",
    "unsigned long long f(void *p) {return NAME(p,(0,1));}",
];

fn options() -> AnalysisOptions {
    AnalysisOptions {
        retain_code: true,
        ..Default::default()
    }
}
fn gnu_target(target: Target) -> bool {
    matches!(
        target,
        Target::I686UnknownLinuxGnu
            | Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
}

#[test]
fn object_size_queries_check_parameters_modes_and_target_result_type() {
    for target in Target::ALL {
        for name in NAMES {
            for source in VALID {
                let source = source.replace("NAME", name);
                let plain = analyze(&source, target)
                    .unwrap_or_else(|error| panic!("{target}: {source}: {error}"));
                let retained = analyze_with_options(&source, target, &options()).unwrap();
                assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
            }
            for source in INVALID {
                let source = source.replace("NAME", name);
                let plain = analyze(&source, target).unwrap_err();
                let retained = analyze_with_options(&source, target, &options()).unwrap_err();
                assert_eq!(
                    (plain.offset, plain.message),
                    (retained.offset, retained.message),
                    "{source}"
                );
            }
            for source in GNU_MODES {
                let source = source.replace("NAME", name);
                assert_eq!(
                    analyze(&source, target).is_ok(),
                    gnu_target(target),
                    "{source}"
                );
                assert_eq!(
                    analyze_with_options(&source, target, &options()).is_ok(),
                    gnu_target(target)
                );
            }
            let size_type = if target == Target::I686UnknownLinuxGnu {
                "unsigned int"
            } else if target.is_windows() {
                "unsigned long long"
            } else {
                "unsigned long"
            };
            analyze(&format!("_Static_assert(_Generic({name}(0,0), {size_type}:1, default:0), \"size type\");"),target).unwrap();
            let unit = analyze("char a[4];", target).unwrap();
            assert_eq!(
                evaluate_integer(&unit, &format!("{name}(a,0)"))
                    .unwrap()
                    .value,
                4
            );
        }
    }
}

#[test]
fn retained_queries_keep_unevaluated_conversions_and_existing_vla_bounds() {
    let source = "void *allocate(unsigned long n) __attribute__((alloc_size(1))); void f(int n) {char a[n++]; __builtin_object_size(a,0); __builtin_dynamic_object_size(allocate(n++),0x100000001ULL);}";
    for target in Target::ALL {
        let analysis = analyze_with_options(source, target, &options()).unwrap();
        let code = analysis.checked().unwrap();
        assert_eq!(code.bounds().count(), 1);
        assert_eq!(
            code.bounds().next().unwrap().1.evaluation(),
            BoundEvaluation::Required
        );
        let mut found = Vec::new();
        for (_, expression) in code.expressions() {
            if let ExprKind::BuiltinCall {
                builtin, arguments, ..
            } = expression.kind()
            {
                if !matches!(builtin, Builtin::ObjectSize | Builtin::DynamicObjectSize) {
                    continue;
                }
                found.push(*builtin);
                assert_eq!(arguments.len(), 2);
                assert_eq!(
                    arguments[0].context(),
                    if gnu_target(target) || *builtin == Builtin::DynamicObjectSize {
                        UseContext::UnevaluatedValue
                    } else {
                        UseContext::CompilerQuery
                    }
                );
                assert_eq!(arguments[1].context(), UseContext::UnevaluatedValue);
                assert_eq!(
                    code.ty(arguments[1].effective_type()).unwrap().kind,
                    TypeKind::Integer(IntegerKind::Int)
                );
                let TypeKind::Pointer(pointee) =
                    &code.ty(arguments[0].effective_type()).unwrap().kind
                else {
                    panic!()
                };
                assert!(pointee.qualifiers.is_const);
                assert_eq!(pointee.kind, TypeKind::Void);
                assert!(!arguments[0].conversions().is_empty());
                assert_eq!(
                    arguments[1].conversions().is_empty(),
                    *builtin == Builtin::ObjectSize
                );
            }
        }
        assert_eq!(found, [Builtin::ObjectSize, Builtin::DynamicObjectSize]);
    }
}

#[test]
fn fresh_vla_types_are_checked_on_every_profile() {
    for name in NAMES {
        for pointer in ["(int (*)[n++])0", "(void *)(unsigned long)sizeof(int[n++])"] {
            let source = format!("unsigned long long f(int n) {{return {name}({pointer},0);}}");
            for target in Target::ALL {
                let result = analyze_with_options(&source, target, &options());
                if gnu_target(target) {
                    let analysis = result.unwrap();
                    assert!(
                        analysis
                            .checked()
                            .unwrap()
                            .bounds()
                            .all(|(_, bound)| bound.evaluation() == BoundEvaluation::Unevaluated)
                    );
                } else {
                    result.unwrap();
                    analyze(&source, target).unwrap();
                }
            }
        }
    }
}

fn is_gnu_compiler(compiler: &str) -> bool {
    let output = std::process::Command::new(compiler)
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    let version = String::from_utf8(output.stdout).unwrap();
    let gnu = version.contains("Free Software Foundation");
    assert!(
        gnu || version.to_ascii_lowercase().contains("clang"),
        "{version}"
    );
    gnu
}

#[test]
#[ignore = "requires GCC and Clang with cross targets; run with --include-ignored"]
fn object_size_constraints_match_consumed_native_calls() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("object-size.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.into_iter().map(Some).collect()),
    ] {
        let gnu = is_gnu_compiler(compiler);
        for target in targets {
            for name in NAMES {
                let types = format!(
                    "_Static_assert(_Generic({name}(0,0), __SIZE_TYPE__:1, default:0), \"result\");\n"
                );
                for (source, accepted) in VALID
                    .iter()
                    .map(|s| ((*s).to_owned(), true))
                    .chain(INVALID.iter().map(|s| ((*s).to_owned(), false)))
                    .chain(GNU_MODES.iter().map(|s| ((*s).to_owned(), gnu)))
                    .chain([(types, true)])
                {
                    let source = source.replace("NAME", name);
                    std::fs::write(&input, format!("{source}\n")).unwrap();
                    let mut command = std::process::Command::new(compiler);
                    command.args([
                        "-std=c11",
                        "-pedantic-errors",
                        "-Werror=int-conversion",
                        "-Wno-unused-value",
                        "-O0",
                        "-c",
                    ]);
                    command.arg(if gnu {
                        "-Werror=discarded-qualifiers"
                    } else {
                        "-Werror=incompatible-pointer-types-discards-qualifiers"
                    });
                    if let Some(target) = target {
                        command.args(["-target", target.triple()]);
                    }
                    let output = command
                        .arg(&input)
                        .arg("-o")
                        .arg(directory.path().join("object-size.o"))
                        .output()
                        .unwrap();
                    assert_eq!(
                        toucan_test_support::compiler_acceptance(&output),
                        Ok(accepted),
                        "{compiler} {target:?}: {source}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn object_size_queries_suppress_pointer_and_allocator_side_effects() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("effects.c");
    for name in NAMES {
        let source = format!(
            "#include <stdlib.h>\nint calls; void *allocator(size_t n) __attribute__((alloc_size(1))); void *allocator(size_t n) {{++calls; return malloc(n);}} volatile size_t result; int main(void){{char a[10]; char *p=a; unsigned n=7; result={name}(p++,0); if(p!=a)return 1; result={name}(malloc(n++),0); result={name}(allocator(n++),0); if(n!=7 || calls)return 2; result={name}(a,0); return result!=10;}}\n"
        );
        std::fs::write(&input, source).unwrap();
        for compiler in [
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            "clang".into(),
        ] {
            for optimization in ["-O0", "-O2"] {
                let binary = directory.path().join("effects");
                let output = std::process::Command::new(&compiler)
                    .args(["-std=c11", optimization])
                    .arg(&input)
                    .arg("-o")
                    .arg(&binary)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    std::process::Command::new(binary)
                        .status()
                        .unwrap()
                        .success(),
                    "{compiler} {name} {optimization}"
                );
            }
        }
    }
}
