use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

fn check(source: &str, profile: CompilerProfile) {
    let plain = analyze_with_profile(source, profile, &AnalysisOptions::default())
        .unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap_or_else(|error| panic!("retained {profile:?}: {source}: {error}"));
    assert_eq!(
        format!("{:?}", plain.unit()),
        format!("{:?}", retained.unit())
    );
}

#[test]
fn alignment_queries_follow_declarations_members_and_pointer_origins() {
    let cases = [
        ("int x __attribute__((aligned(32)));", "x", 32, 32),
        ("int x __attribute__((aligned(1)));", "x", 1, 1),
        ("int x __attribute__((aligned(32)));", "*(&x)", 32, 4),
        ("int x __attribute__((aligned(32)));", "*(&x+0)", 32, 4),
        ("int x __attribute__((aligned(32)));", "*(&x+(1-1))", 32, 4),
        ("int x __attribute__((aligned(32)));", "*(&x+1)", 4, 4),
        ("int x __attribute__((aligned(32)));", "(&x)[0]", 32, 4),
        (
            "int x __attribute__((aligned(32)));",
            "*((int*)(void*)&x)",
            32,
            4,
        ),
        ("int x __attribute__((aligned(32)));", "*(char*)&x", 4, 1),
        (
            "int x __attribute__((aligned(32)));",
            "*((int*)(char*)&x)",
            32,
            4,
        ),
        (
            "int x __attribute__((aligned(32)));",
            "_Generic(x,int:x)",
            32,
            32,
        ),
        ("int x __attribute__((aligned(32)));", "*(1?&x:&x)", 4, 4),
        ("int x[4] __attribute__((aligned(32)));", "*x", 4, 4),
        ("int x[4] __attribute__((aligned(32)));", "*(&x)", 32, 4),
        (
            "struct __attribute__((packed)) S{char lead;int x;};struct S s;",
            "s.x",
            1,
            1,
        ),
        (
            "struct __attribute__((packed)) S{char lead;int x;};struct S s;",
            "*(&s.x)",
            1,
            4,
        ),
        (
            "struct S{int x;};struct S s __attribute__((aligned(32)));",
            "s.x",
            4,
            4,
        ),
        (
            "struct __attribute__((packed)) O{char c;struct I{int x;} i;};struct O s;",
            "s.i.x",
            4,
            4,
        ),
        ("", "(void)0", 1, 1),
        ("typedef int T __attribute__((aligned(16)));", "(T)0", 4, 16),
        (
            "typedef int T __attribute__((aligned(16)));",
            "typeof((T)0)",
            4,
            16,
        ),
        (
            "typedef int *T __attribute__((aligned(16)));",
            "(T)0",
            8,
            16,
        ),
        (
            "struct S{int n;};typedef struct S T __attribute__((aligned(16)));T s;",
            "(T)s",
            4,
            16,
        ),
        ("struct S{unsigned x:3;};struct S s;", "(int)s.x", 4, 4),
    ];
    for profile in CompilerProfile::ALL {
        for spelling in ["_Alignof", "__alignof__"] {
            for (declaration, expression, gnu, clang) in cases {
                let expected = if profile.compiler() == Compiler::Gnu {
                    gnu
                } else {
                    clang
                };
                check(
                    &format!(
                        "{declaration}_Static_assert({spelling}({expression})=={expected},\"alignment\");"
                    ),
                    profile,
                );
            }
            let expected = if profile.compiler() == Compiler::Gnu
                && profile.target() == Target::X86_64UnknownLinuxGnu
            {
                1
            } else {
                4
            };
            check(
                &format!(
                    "int f(void);_Static_assert({spelling}(f)=={expected},\"function alignment\");"
                ),
                profile,
            );
            check(
                &format!(
                    "int f(void) __attribute__((aligned(32)));_Static_assert({spelling}(f)==32,\"function alignment\");"
                ),
                profile,
            );
        }
    }
}

include!("fixtures/alignof_queries.rs");

fn assertion_source(source: &str, value: u64) -> String {
    let (prefix, query) = source.split_once("unsigned long long result=").unwrap();
    format!(
        "{prefix}_Static_assert(({})=={value},\"compiler alignment\");",
        query.trim().trim_end_matches(';')
    )
}

#[test]
fn saved_compiler_queries_match_in_both_analysis_modes() {
    let mut failures = Vec::new();
    for &(name, target, compiler, source, value) in OBSERVATIONS {
        let profile = CompilerProfile::new(target, compiler).unwrap();
        let source = value.map_or_else(
            || source.to_owned(),
            |value| assertion_source(source, value),
        );
        for retained in [false, true] {
            let result = analyze_with_profile(
                &source,
                profile,
                &AnalysisOptions {
                    retain_code: retained,
                    ..Default::default()
                },
            );
            if result.is_ok() != value.is_some() {
                failures.push(format!(
                    "{name} {profile:?} retained={retained}: {source}: {}",
                    result.err().map_or("accepted".into(), |e| e.to_string())
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

const EFFECTS: &str = r#"
int f(int n) {
    typedef int T[n++];
    int object __attribute__((aligned(32)));
    unsigned a=__alignof__(int[n++]);
    unsigned b=_Alignof(*(int(*)[n++])0);
    unsigned c=__alignof__((n++,object));
    unsigned d=_Alignof(({struct S{int x;};int array[n++];array;}));
    return n + (a!=4) + (b!=4) + (c!=4) + (d!=sizeof(void*));
}
"#;

#[test]
fn owned_alignment_operands_remain_unevaluated_without_replaying_scopes() {
    use toucan_semantic::checked::{AlignmentOperand, BoundEvaluation, ExprKind, UseContext};
    for profile in CompilerProfile::ALL {
        check(EFFECTS, profile);
        let analysis = analyze_with_profile(
            EFFECTS,
            profile,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            analysis
                .unit()
                .records
                .iter()
                .filter(|record| record.name.as_deref() == Some("S"))
                .count(),
            1
        );
        let code = analysis.checked().unwrap();
        assert_eq!(
            code.bounds()
                .filter(|(_, b)| b.evaluation() == BoundEvaluation::Required)
                .count(),
            1
        );
        assert_eq!(
            code.bounds()
                .filter(|(_, b)| b.evaluation() == BoundEvaluation::Unevaluated)
                .count(),
            3
        );
        let mut queries = 0;
        for (_, expression) in code.expressions() {
            if let ExprKind::AlignOf {
                operand,
                alignment_bytes,
                ..
            } = expression.kind()
            {
                queries += 1;
                assert!([4, 8].contains(alignment_bytes));
                match operand {
                    AlignmentOperand::Type(operand) => {
                        assert!(code.type_use(operand.type_use).is_some())
                    }
                    AlignmentOperand::Expression(operand) => {
                        assert_eq!(operand.context(), UseContext::Unevaluated)
                    }
                }
            }
        }
        assert_eq!(queries, 4);
    }
}

const ALIGNED_PARAMETERS: &str = r#"
int prototype(int parameter __attribute__((aligned(32)))) {
    _Static_assert(__alignof__(parameter)==32,"parameter storage");
    return parameter;
}
int identifier_list(parameter) int parameter __attribute__((aligned(32))); {
    _Static_assert(__alignof__(parameter)==32,"parameter storage");
    return parameter;
}
"#;

fn component_alignment_assertions(gnu: bool) -> String {
    let scalar = if gnu { 32 } else { 4 };
    format!(
        r#"
volatile double _Complex complex_value __attribute__((aligned(32)));
int scalar_value __attribute__((aligned(32)));
_Static_assert(_Alignof(complex_value)==32,"complex object");
_Static_assert(_Alignof(__real__ complex_value)==8,"real component");
_Static_assert(_Alignof(__imag__ complex_value)==8,"imaginary component");
_Static_assert(_Alignof(__real__ scalar_value)=={scalar},"real scalar");
_Static_assert(_Alignof(__imag__ scalar_value)==4,"imaginary scalar");
"#
    )
}

#[test]
fn complex_components_use_component_alignment_and_scalar_queries_keep_profile_rules() {
    for profile in CompilerProfile::ALL {
        check(
            &component_alignment_assertions(profile.compiler() == Compiler::Gnu),
            profile,
        );
    }
}

#[test]
fn parameter_alignment_is_visible_after_the_definition_scope_is_promoted() {
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| profile.compiler() == Compiler::Clang)
    {
        check(ALIGNED_PARAMETERS, profile);
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang; run with --include-ignored"]
fn alignment_queries_do_not_execute_operand_effects() {
    use std::process::Command;
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let parameters = if compiler == "clang" {
            ALIGNED_PARAMETERS
        } else {
            ""
        };
        let components = component_alignment_assertions(compiler != "clang");
        std::fs::write(
            &input,
            format!("{EFFECTS}\n{parameters}\n{components}\nint main(void){{return f(3)!=4;}}"),
        )
        .unwrap();
        for opt in ["-O0", "-O2"] {
            let binary = directory.path().join("probe");
            let output = Command::new(compiler)
                .args(["-std=gnu11", opt])
                .arg(&input)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(true),
                "{compiler} {opt}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let result = Command::new(&binary).output().unwrap();
            assert!(
                result.status.success(),
                "{compiler} {opt}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang cross-target parsing; run with --include-ignored"]
fn saved_queries_still_match_the_installed_compiler_oracles() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let host = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let mut failures = Vec::new();
    for &(name, target, compiler, source, value) in OBSERVATIONS {
        if compiler == Compiler::Gnu && target != host {
            continue;
        }
        let mut command = Command::new(if compiler == Compiler::Gnu {
            gcc.as_str()
        } else {
            "clang"
        });
        command.args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]);
        if compiler == Compiler::Clang {
            command.args(["-target", target.triple()]);
        }
        let source = value.map_or_else(
            || source.to_owned(),
            |value| assertion_source(source, value),
        );
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        if toucan_test_support::compiler_acceptance(&output) != Ok(value.is_some()) {
            failures.push(format!(
                "{name} {target:?} {compiler:?}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sve_values_can_be_unevaluated_but_have_no_alignment_themselves() {
    for profile in CompilerProfile::ALL.into_iter().filter(|profile| {
        matches!(
            profile.target(),
            Target::Aarch64UnknownLinuxGnu | Target::Aarch64AppleDarwin
        )
    }) {
        check(
            "int f(__SVFloat32_t *value){_Static_assert(__alignof__((void)*value)==1,\"void\");return 0;}",
            profile,
        );
        assert!(
            analyze_with_profile(
                "int f(__SVFloat32_t *value){return __alignof__(*value);}",
                profile,
                &AnalysisOptions::default()
            )
            .is_err()
        );
    }
}
