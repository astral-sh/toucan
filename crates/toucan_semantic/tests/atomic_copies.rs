use toucan_semantic::checked::{Conversion, ExprKind};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const SOURCE: &str = r#"
struct S{int x;};
_Atomic(struct S) make(void){return (struct S){7};}
int f(int n){
 _Atomic int scalar=(_Atomic(int))1;
 _Atomic(int*) pointer=(_Atomic(int*))&n;
 _Atomic(struct S) record=make();
 pointer=(_Atomic(int*))&n;
 record=make();
 __auto_type inferred=(_Atomic(int*))&n;
 return sizeof scalar+sizeof pointer+sizeof record+sizeof inferred;
}
"#;
fn gnu(target: Target) -> bool {
    matches!(
        target,
        Target::X86_64UnknownLinuxGnu
            | Target::X86_64UnknownLinuxMusl
            | Target::Aarch64UnknownLinuxGnu
            | Target::Aarch64UnknownLinuxMusl
    )
}

#[test]
fn exact_atomic_copies_preserve_rvalue_types_without_loads() {
    for target in Target::ALL {
        let plain = analyze(SOURCE, target).unwrap_or_else(|e| panic!("{target}: {e}"));
        let checked = analyze_with_options(
            SOURCE,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(format!("{plain:?}"), format!("{:?}", checked.unit()));
        let code = checked.checked().unwrap();
        for (_, expression) in code.expressions() {
            if let ExprKind::Cast { value, .. } | ExprKind::Binary { right: value, .. } =
                expression.kind()
            {
                assert!(
                    value
                        .conversions()
                        .iter()
                        .all(|c| c.kind() != Conversion::AtomicLoad)
                );
            }
        }
        for (_, assignment) in code.assignment_conversions() {
            assert!(
                assignment
                    .conversions()
                    .iter()
                    .all(|c| c.kind() != Conversion::AtomicLoad)
            );
            if !gnu(target) {
                let original = code.expression(assignment.expression()).unwrap();
                if checked
                    .unit()
                    .atomic_value(code.ty(original.ty()).unwrap())
                    .unwrap()
                    .is_some()
                {
                    assert_eq!(original.ty(), assignment.effective_type());
                    assert!(assignment.conversions().is_empty());
                }
            }
        }
    }
}

#[test]
fn atomic_pointer_rvalues_do_not_gain_ordinary_pointer_conversions() {
    for source in [
        "void f(void){int *p=(_Atomic(int*))0;}",
        "void f(void){_Atomic(const int*) p=(_Atomic(int*))0;}",
        "void f(void){_Atomic int value={(_Atomic(int))1};}",
    ] {
        for target in Target::ALL {
            assert_eq!(
                analyze(source, target).is_ok(),
                gnu(target),
                "{target}: {source}"
            );
        }
    }
    for source in [
        "void f(void){_Atomic(double*) p=(_Atomic(int*))0;}",
        "void f(void){const _Atomic(int*) p=0;p=(_Atomic(int*))0;}",
    ] {
        for target in Target::ALL {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang"]
fn copy_constraints_and_values_match_native_compilers() {
    use std::ffi::OsStr;
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for compiler in ["gcc", "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let version = String::from_utf8_lossy(&version.stdout);
        let is_gnu = !version.to_ascii_lowercase().contains("clang");
        // The recorded Apple build crashes on pointer-to-atomic-pointer casts.
        // Check those exact sources with upstream Clang on the same host. Other
        // cases still use Apple Clang; a missing replacement is a test failure.
        let pointer_compiler = if needs_atomic_pointer_oracle(&version, SOURCE) {
            let path = std::env::var_os("TOUCAN_ATOMIC_POINTER_CLANG").expect(
                "Apple clang-1700.0.13.5 needs TOUCAN_ATOMIC_POINTER_CLANG pointing to upstream Clang 18 for atomic pointer casts; see docs/compiler-oracle-discrepancies.md",
            );
            let identity = Command::new(&path).arg("--version").output().unwrap();
            let identity_text = String::from_utf8_lossy(&identity.stdout);
            assert!(
                identity.status.success()
                    && identity_text.contains("clang version 18.")
                    && !identity_text.contains("Apple clang"),
                "expected upstream Clang 18: {identity_text}"
            );
            path
        } else {
            compiler.into()
        };
        let oracle = |source: &str| -> &OsStr {
            if needs_atomic_pointer_oracle(&version, source) {
                &pointer_compiler
            } else {
                OsStr::new(compiler)
            }
        };
        for (name, source, accept) in [
            (
                "scalar-init",
                "void f(void){_Atomic int x=(_Atomic(int))1;}",
                true,
            ),
            (
                "pointer-init",
                "void f(int n){_Atomic(int*) p=(_Atomic(int*))&n;}",
                true,
            ),
            (
                "record-init",
                "struct S{int x;};_Atomic(struct S) make(void){return (struct S){7};}void f(void){_Atomic(struct S) x=make();}",
                true,
            ),
            (
                "pointer-assignment",
                "void f(int n){_Atomic(int*) p=0;p=(_Atomic(int*))&n;}",
                true,
            ),
            (
                "record-assignment",
                "struct S{int x;};_Atomic(struct S) make(void){return (struct S){7};}void f(void){_Atomic(struct S) x;x=make();}",
                true,
            ),
            (
                "pointer-call-copy",
                "_Atomic(int*) make(int *p){return p;}void f(int n){_Atomic(int*) p=make(&n);p=make(&n);__auto_type q=make(&n);}",
                true,
            ),
            (
                "inferred-scalar",
                "void f(void){__auto_type x=(_Atomic(int))1;}",
                true,
            ),
            (
                "inferred-pointer",
                "void f(int n){__auto_type p=(_Atomic(int*))&n;}",
                true,
            ),
            ("combined", SOURCE, true),
            (
                "ordinary-pointer",
                "void f(void){int *p=(_Atomic(int*))0;}",
                is_gnu,
            ),
            (
                "qualified-pointer",
                "void f(void){_Atomic(const int*) p=(_Atomic(int*))0;}",
                is_gnu,
            ),
            (
                "scalar-braces",
                "void f(void){_Atomic int value={(_Atomic(int))1};}",
                is_gnu,
            ),
            (
                "incompatible-pointer",
                "void f(void){_Atomic(double*) p=(_Atomic(int*))0;}",
                false,
            ),
        ] {
            let path = temp.path().join(format!("{name}.c"));
            std::fs::write(&path, source).unwrap();
            let compiler = oracle(source);
            let output = Command::new(compiler)
                .args([
                    "-std=gnu11",
                    "-Werror=incompatible-pointer-types",
                    "-fsyntax-only",
                ])
                .arg(path)
                .output()
                .unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            let acceptance = toucan_test_support::compiler_acceptance(&output);
            if acceptance != Ok(accept) {
                failures.push(format!(
                    "{compiler:?} {name}: expected acceptance={accept}, status={}, acceptance={acceptance:?}\n{source}\n{stderr}",
                    output.status
                ));
            }
        }
        let source = r#"struct S{int x;};_Atomic(struct S) make(void){return (struct S){7};}
int main(void){int n=3;_Atomic int scalar=(_Atomic(int))1;_Atomic(int*) pointer=(_Atomic(int*))&n;_Atomic(struct S) record=make();__auto_type inferred=(_Atomic(int*))&n;struct S result=record;return scalar!=1||*pointer!=3||result.x!=7||*inferred!=3;}"#;
        // Also execute atomic pointer copies from function results with the
        // original compiler, including the affected Apple build.
        let call_source = source.replace("(_Atomic(int*))&n", "make_pointer(&n)");
        let call_source = format!("_Atomic(int*) make_pointer(int *p){{return p;}}\n{call_source}");
        for source in [source, &call_source] {
            let path = temp.path().join("runtime.c");
            std::fs::write(&path, source).unwrap();
            let compiler = oracle(source);
            for opt in ["-O0", "-O2"] {
                let exe = temp.path().join("runtime");
                let output = Command::new(compiler)
                    .args(["-std=gnu11", opt])
                    .arg(&path)
                    .arg("-o")
                    .arg(&exe)
                    .output()
                    .unwrap();
                if !output.status.success() {
                    failures.push(format!(
                        "{compiler:?} runtime {opt}: {}\n{source}\n{}",
                        output.status,
                        String::from_utf8_lossy(&output.stderr)
                    ));
                    continue;
                }
                let status = Command::new(exe).status().unwrap();
                if !status.success() {
                    failures.push(format!("{compiler:?} runtime {opt}: {status}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Limits the alternate oracle to the recorded Apple build and crashing cast.
fn needs_atomic_pointer_oracle(version: &str, source: &str) -> bool {
    version.lines().next() == Some("Apple clang version 17.0.0 (clang-1700.0.13.5)")
        && source.contains("(_Atomic(int*))&n")
}

#[test]
fn alternate_atomic_oracle_is_limited_to_the_recorded_crash() {
    let apple = "Apple clang version 17.0.0 (clang-1700.0.13.5)\nTarget: x86_64-apple-darwin";
    assert!(needs_atomic_pointer_oracle(apple, SOURCE));
    assert!(!needs_atomic_pointer_oracle(
        apple,
        "void f(void){int *p=(_Atomic(int*))0;}"
    ));
    assert!(!needs_atomic_pointer_oracle(
        "Apple clang version 17.0.0 (clang-1700.6.3.2)",
        SOURCE
    ));
    assert!(!needs_atomic_pointer_oracle(
        "Ubuntu clang version 18.1.3 (1ubuntu1)",
        SOURCE
    ));
}
