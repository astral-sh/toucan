use toucan_semantic::checked::{Conversion, EntityKind, ExprKind};
use toucan_semantic::{AnalysisOptions, Type, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "typedef union {int *p; void *v;} U __attribute__((transparent_union)); int f(U); int g(int *p, void *q) { return f(p)+f(q)+f(0); }",
    "typedef union {int i; unsigned u;} U __attribute__((transparent_union)); int f(U u) {return u.i;} int g(U u) { return f(u)+f(1)+f(1u); }",
    "typedef union {int i; float f;} U __attribute__((transparent_union)); int f(U); int g(float x) {return f(x);}",
    "typedef union {const int *p; const unsigned *q;} U __attribute__((transparent_union)); int f(U); int g(int *p) {return f(p);}",
    "typedef union {int *p; unsigned *q;} U __attribute__((transparent_union)); int f(U); int g(void *p) {return f(p);}",
    "typedef union {int i; unsigned u;} U __attribute__((transparent_union)); struct S {U u;}; U f(U u){return u;} int v(int,...); int g(U u){return v(1,u);}",
    "union X {int *p;void *v;}; typedef union X U; typedef U V __attribute__((transparent_union)); int f(union X); int g(int *p){return f(p);}",
    "typedef union {int i; unsigned u;} U __attribute__((transparent_union)); int f(U); int f(int); int g(int x){return f(x);}",
    "typedef union {int i; float f;} U __attribute__((transparent_union)); int f(float); int f(U); int g(float x){return f(x);}",
    "union __attribute__((transparent_union)) U {int *p;void *v;}; int f(union U); int g(int *p){return f(p);}",
    "typedef union __attribute__((packed)) {int i;unsigned u;} U __attribute__((transparent_union)); int f(U); int g(int x){return f(x);}",
    "void g(void) {union X{int *p;void *v;}; typedef union X U; typedef U V __attribute__((transparent_union)); int f(union X); int *p=0; f(p);}",
];
const INVALID: &[&str] = &[
    "typedef struct {int x;} U __attribute__((transparent_union));",
    "typedef union {float f;int i;} U __attribute__((transparent_union));",
    "typedef union {int i;long long j;} U __attribute__((transparent_union));",
    "typedef union {int i;unsigned u;} *U __attribute__((transparent_union));",
    "typedef union {int i;unsigned u;} U __attribute__((transparent_union(1)));",
    "typedef union {int i;unsigned u;} U __attribute__((transparent_union)); void f(void){ U u=1; }",
    "typedef union {int i;unsigned u;} U __attribute__((transparent_union)); void f(void){ U u; u=1; }",
    "typedef union {int *p;unsigned *q;} U __attribute__((transparent_union)); int f(U); int g(const int *p){return f(p);}",
    "typedef union {int *p;unsigned *q;} U __attribute__((transparent_union)); int f(U); int g(double *p){return f(p);}",
    "typedef union {int i;float f;} U __attribute__((transparent_union)); int f(U); int f(unsigned);",
    "union U {int i;unsigned u;}; int f(union U); int g(int x){return f(x);}",
];

const CLANG_PROFILE: &[&str] = &[
    "union X{int*p;void*q;};typedef union X U __attribute__((transparent_union));int f(union X);int g(int*p){return f(p);}",
    "union X{int*p;void*q;};typedef union X U __attribute__((transparent_union));void f(U u,union X x){u=x;}",
    "union X{int*p;void*q;};typedef union X U __attribute__((transparent_union));typedef union X V __attribute__((transparent_union));void f(U u,V v){u=v;}",
    "typedef union {int i;float f;} U __attribute__((transparent_union));int f(U);int g(short x){return f(x);}",
];
const GNU_PROFILE: &str = "typedef union {int i;short s;} U __attribute__((transparent_union));int f(U);int g(short x){return f(x);}";

fn check(source: &str, target: Target, accepted: bool) {
    let plain = analyze(source, target);
    let retained = analyze_with_options(
        source,
        target,
        &AnalysisOptions {
            retain_code: true,
            ..AnalysisOptions::default()
        },
    );
    assert_eq!(plain.is_ok(), accepted, "{target}: {source}: {plain:?}");
    assert_eq!(
        retained.is_ok(),
        accepted,
        "{target}: {source}: {retained:?}"
    );
    match (plain, retained) {
        (Ok(plain), Ok(retained)) => {
            assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()))
        }
        (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message)),
        _ => unreachable!(),
    }
}
#[test]
fn argument_conversions_preserve_storage_rules_and_retention_parity() {
    for target in Target::ALL {
        for source in VALID {
            check(source, target, true);
        }
        for source in INVALID {
            check(source, target, false);
        }
    }
}
#[test]
fn typedef_identity_and_scalar_conversion_follow_the_compiler_profile() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        for source in CLANG_PROFILE {
            check(source, target, !gnu);
        }
        check(
            "typedef union {int i;short s;} U __attribute__((transparent_union));int f(U);int g(short x){return f(x);}",
            target,
            gnu,
        );
    }
}
#[test]
fn retained_conversions_reference_source_fields_and_expose_parameter_abi() {
    let source = "union X {int i;float f;};typedef union X U __attribute__((transparent_union));int f(U);int g(float x){return f(x);} int read(U u){return u.i;}";
    for target in Target::ALL {
        let analysis = analyze_with_options(
            source,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        )
        .unwrap();
        let unit = analysis.unit();
        let code = analysis.checked().unwrap();
        let alias = Type::new(TypeKind::Typedef("U".into()));
        let record = unit.transparent_union(&alias).unwrap().unwrap();
        let origin = unit.record_origin(record).unwrap();
        let gnu = matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        assert_eq!(record != origin, gnu);
        let carrier = unit.parameter_abi_type(&alias).unwrap();
        assert_eq!(
            unit.resolve(carrier).unwrap().kind,
            if target == Target::X86_64PcWindowsMsvc {
                TypeKind::Record(record)
            } else {
                TypeKind::Integer(toucan_semantic::IntegerKind::Int)
            }
        );

        let mut count = 0;
        for (_, expression) in code.expressions() {
            if let ExprKind::Call { arguments, .. } = expression.kind() {
                let argument = &arguments[0];
                let step = argument.conversions().last().unwrap();
                let Conversion::TransparentUnion { field } = step.kind() else {
                    panic!("missing union conversion")
                };
                assert_eq!(
                    code.entity(field).unwrap().kind(),
                    EntityKind::Field {
                        record: origin,
                        index: usize::from(gnu)
                    }
                );
                assert_eq!(
                    code.ty(step.target_type()).unwrap().kind,
                    TypeKind::Record(record)
                );
                assert_eq!(
                    code.type_use(argument.type_use()).unwrap().shape(),
                    argument.effective_type()
                );
                count += 1;
            }
        }
        assert_eq!(count, 1);
        let f = unit
            .declarations
            .iter()
            .find(|item| item.name == "f")
            .unwrap();
        let TypeKind::Function(function) = &f.ty.kind else {
            panic!()
        };
        assert_eq!(
            unit.transparent_union(&function.parameters[0].ty).unwrap(),
            Some(record)
        );
    }
}
#[test]
fn member_prototypes_compose_without_changing_union_storage_compatibility() {
    for target in Target::ALL {
        for (first, second) in [("U", "float"), ("float", "U")] {
            let source = format!(
                "typedef union{{int i;float f;}}U __attribute__((transparent_union));int f({first});int f({second});"
            );
            let unit = analyze(&source, target).unwrap();
            let TypeKind::Function(f) = &unit
                .declarations
                .iter()
                .find(|d| d.name == "f")
                .unwrap()
                .ty
                .kind
            else {
                panic!()
            };
            assert_eq!(
                unit.resolve(&f.parameters[0].ty).unwrap().kind,
                TypeKind::Float(toucan_semantic::FloatKind::Float)
            );
        }
        let error=analyze("typedef union{int i;float f;}U __attribute__((transparent_union));int f(float);int f(U u){return u.i;}",target).unwrap_err();
        assert!(
            error.message.contains("member-typed redeclarations"),
            "{error}"
        );
    }
}
#[test]
#[ignore = "requires genuine GNU GCC and Clang cross targets; run with --include-ignored"]
fn constraints_match_native_gcc_and_five_clang_profiles() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("transparent.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        let version = std::process::Command::new(compiler)
            .arg("--version")
            .output()
            .unwrap();
        if compiler == gcc {
            assert!(
                !String::from_utf8_lossy(&version.stdout).contains("clang"),
                "TOUCAN_GCC must select GNU GCC"
            );
        }
        for target in targets {
            for (source, accepted) in VALID
                .iter()
                .map(|s| (*s, true))
                .chain(INVALID.iter().map(|s| (*s, false)))
                .chain(CLANG_PROFILE.iter().map(|s| (*s, compiler != gcc)))
                .chain(std::iter::once((GNU_PROFILE, compiler == gcc)))
            {
                std::fs::write(&input, format!("{source}\n")).unwrap();
                let mut command = std::process::Command::new(compiler);
                command.args([
                    "-std=c11",
                    "-Werror=attributes",
                    "-Werror=incompatible-pointer-types",
                    "-Werror=int-conversion",
                    "-fsyntax-only",
                ]);
                if compiler == gcc {
                    command.arg("-Werror=discarded-qualifiers");
                }
                if let Some(target) = target {
                    command.arg(format!("--target={target}"));
                }
                let output = command.arg(&input).output().unwrap();
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

#[test]
fn nominal_variants_have_a_bounded_cloned_representation() {
    let mut source = String::from("union X {");
    for index in 0..1024 {
        source.push_str(&format!("int f{index};"));
    }
    source.push_str("};");
    for index in 0..256 {
        source.push_str(&format!(
            "typedef union X U{index} __attribute__((transparent_union));"
        ));
    }
    let error = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap_err();
    assert!(
        error.message.contains("16 MiB representation budget"),
        "{error}"
    );
}

#[test]
#[ignore = "requires native GNU GCC and Clang; run with --include-ignored"]
fn native_calls_use_the_first_member_abi_and_profile_conversion() {
    if !matches!(std::env::consts::OS, "linux" | "macos") {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let source = r#"
        typedef union {int *p;void *v;} P __attribute__((transparent_union));
        typedef union {int i;float f;} U __attribute__((transparent_union));
        __attribute__((noinline)) int pointer(P p){return *p.p;}
        __attribute__((noinline)) int value(U u){return u.i;}
        int main(void) {
            int x=7; int (*callback)(P)=pointer;
            P stored={.p=&x};
            if (callback(&x)!=7 || pointer(stored)!=7) return 1;
            return value(1.5f)!=EXPECTED;
        }
    "#;
    let input = directory.path().join("abi.c");
    std::fs::write(&input, source).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, expected) in [(gcc.as_str(), 1069547520), ("clang", 1)] {
        let binary = directory.path().join("abi");
        let output = std::process::Command::new(compiler)
            .args(["-std=c11", "-O2", "-Werror=attributes"])
            .arg(format!("-DEXPECTED={expected}"))
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
            std::process::Command::new(&binary)
                .status()
                .unwrap()
                .success(),
            "{compiler}"
        );
    }
}

#[test]
fn member_type_alignment_is_distinct_from_field_and_union_alignment() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        check(
            "typedef int I __attribute__((aligned(16))); typedef union {int first; I second;} U __attribute__((transparent_union));",
            target,
            false,
        );
        check(
            "typedef int I __attribute__((aligned(1))); typedef union {int first; I second;} U __attribute__((transparent_union));",
            target,
            true,
        );
        check(
            "typedef union {int first; int second __attribute__((aligned(16)));} U __attribute__((transparent_union));",
            target,
            !gnu,
        );
        check(
            "typedef union __attribute__((aligned(16))) {int first; int second;} U __attribute__((transparent_union));",
            target,
            !gnu,
        );
    }
}

#[test]
fn typeof_aliases_preserve_known_identities_and_diagnose_missing_origins() {
    for target in Target::ALL {
        let gnu = matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        for operand in ["U", "u"] {
            check(
                &format!(
                    "union X{{int i;unsigned u;}}; typedef union X U; U u; typedef typeof({operand}) V __attribute__((transparent_union)); int f(union X); int g(int x){{return f(x);}}"
                ),
                target,
                true,
            );
        }
        for operand in ["U", "typeof(U)", "(U){0}"] {
            check(
                &format!(
                    "void g(void){{union X{{int i;unsigned u;}}; typedef union X U; typedef typeof({operand}) V __attribute__((transparent_union)); int f(union X); f(1);}}"
                ),
                target,
                true,
            );
        }
        let source = "void g(void){union X{int i;unsigned u;}; typedef union X U; U local; typedef typeof(local) V __attribute__((transparent_union));}";
        if gnu {
            let error = analyze(source, target).unwrap_err();
            assert!(
                error.message.contains("preserved typedef identity"),
                "{error}"
            );
        } else {
            check(source, target, true);
        }
    }
}
