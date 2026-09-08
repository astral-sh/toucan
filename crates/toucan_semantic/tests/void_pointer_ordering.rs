use std::process::Command;
use toucan_semantic::checked::{Conversion, ExprKind};
use toucan_semantic::{AnalysisOptions, TypeKind, analyze, analyze_with_options};
use toucan_target::Target;

const SOURCE: &str = "int order(const void *left, volatile void *right) { return left < right; }";

#[test]
fn void_pointer_ordering_retains_qualified_pointer_conversions() {
    for target in Target::ALL {
        for operator in ["<", "<=", ">", ">="] {
            let source = SOURCE.replace(" < ", &format!(" {operator} "));
            let ordinary = analyze(&source, target).unwrap();
            let analysis = analyze_with_options(
                &source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            )
            .unwrap();
            assert_eq!(format!("{ordinary:?}"), format!("{:?}", analysis.unit()));
            let code = analysis.checked().unwrap();
            let (_, expression) = code
                .expressions()
                .find(|(_, expression)| matches!(expression.kind(), ExprKind::Binary { .. }))
                .unwrap();
            let ExprKind::Binary { left, right, .. } = expression.kind() else {
                unreachable!()
            };
            for operand in [left, right] {
                let TypeKind::Pointer(pointee) = &code.ty(operand.effective_type()).unwrap().kind
                else {
                    panic!("pointer conversion")
                };
                assert_eq!(pointee.kind, TypeKind::Void);
                assert!(pointee.qualifiers.is_const && pointee.qualifiers.is_volatile);
                assert_eq!(
                    operand.conversions().last().unwrap().kind(),
                    Conversion::Pointer
                );
            }
        }
        for source in [
            "int f(void *p, int *q) { return p < q; }",
            "int f(int *p, char *q) { return p < q; }",
        ] {
            assert!(analyze(source, target).is_err());
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn void_pointer_ordering_matches_native_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("order.c");
    std::fs::write(&source, format!("{SOURCE}\nint main(void) {{ char bytes[2]; return !(order(bytes, bytes+1) && !order(bytes+1, bytes) && !order(bytes, bytes)); }}\n")).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        let executable = directory.path().join("order");
        let result = Command::new(compiler)
            .args(["-std=gnu11", "-Werror", "-O2"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(Command::new(&executable).status().unwrap().success());
    }
}
