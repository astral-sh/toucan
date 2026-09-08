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
        Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
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
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for compiler in ["gcc", "clang"] {
        let version = Command::new(compiler).arg("--version").output().unwrap();
        assert!(version.status.success());
        let is_gnu = !String::from_utf8_lossy(&version.stdout)
            .to_ascii_lowercase()
            .contains("clang");
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
                    "{compiler} {name}: expected acceptance={accept}, status={}, acceptance={acceptance:?}\n{source}\n{stderr}",
                    output.status
                ));
            }
        }
        let source = r#"struct S{int x;};_Atomic(struct S) make(void){return (struct S){7};}
int main(void){int n=3;_Atomic int scalar=(_Atomic(int))1;_Atomic(int*) pointer=(_Atomic(int*))&n;_Atomic(struct S) record=make();__auto_type inferred=(_Atomic(int*))&n;struct S result=record;return scalar!=1||*pointer!=3||result.x!=7||*inferred!=3;}"#;
        let path = temp.path().join("runtime.c");
        std::fs::write(&path, source).unwrap();
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
                    "{compiler} runtime {opt}: {}\n{source}\n{}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                ));
                continue;
            }
            let status = Command::new(exe).status().unwrap();
            if !status.success() {
                failures.push(format!("{compiler} runtime {opt}: {status}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
