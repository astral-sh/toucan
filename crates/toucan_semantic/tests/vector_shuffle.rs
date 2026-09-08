use toucan_semantic::checked::{Builtin, ExprKind, UseContext};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const TYPES: &str = "typedef int V __attribute__((vector_size(16)));\n\
    typedef unsigned int U __attribute__((vector_size(16)));\n\
    typedef float F __attribute__((vector_size(16)));\n\
    typedef short S __attribute__((vector_size(8)));\n\
    typedef V A __attribute__((aligned(1)));\n";
const VALID: &str = r#"
V f(A a,V b,U mask){
    _Static_assert(__alignof__(__typeof__(__builtin_shuffle(a,b,mask)))==1,"alignment");
    V result=__builtin_shuffle(a,b,mask);
    return __builtin_shuffle(result,mask);
}
int main(void){
    V a={1,2,3,4},b={5,6,7,8},mask={4,-1,8,-9};
    V one=__builtin_shuffle(a,mask),two=__builtin_shuffle(a,b,mask);
    int expected_one[4]={1,4,1,4},expected_two[4]={5,8,1,8};
    for(int i=0;i<4;i++)if(one[i]!=expected_one[i] || two[i]!=expected_two[i])return 1;
    return 0;
}
"#;

#[test]
fn shuffle_retains_input_types_and_mask_evaluation() {
    let source = format!("{TYPES}{VALID}");
    for target in Target::ALL {
        let ordinary = analyze(&source, target);
        let retained = analyze_with_options(
            &source,
            target,
            &AnalysisOptions {
                retain_code: true,
                ..AnalysisOptions::default()
            },
        );
        if matches!(
            target,
            Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
        ) {
            let ordinary = ordinary.unwrap();
            let analysis = retained.unwrap();
            assert_eq!(format!("{ordinary:?}"), format!("{:?}", analysis.unit()));
            let code = analysis.checked().unwrap();
            let mut count = 0;
            for (_, expr) in code.expressions() {
                if let ExprKind::BuiltinCall {
                    builtin: Builtin::VectorShuffle,
                    arguments,
                    ..
                } = expr.kind()
                {
                    count += 1;
                    assert!(matches!(arguments.len(), 2 | 3));
                    assert_eq!(
                        code.ty(expr.ty()).unwrap(),
                        code.ty(arguments[0].effective_type()).unwrap()
                    );
                    assert!(
                        arguments
                            .iter()
                            .all(|arg| arg.context() == UseContext::Value)
                    );
                }
            }
            assert_eq!(count, 5);
        } else {
            let a = ordinary.unwrap_err();
            let b = retained.unwrap_err();
            assert_eq!((&a.message, a.offset), (&b.message, b.offset));
            assert!(a.message.contains("GNU compiler profile"));
        }
    }
}

#[test]
fn shuffle_rejects_mismatched_vectors_and_masks() {
    for body in [
        "V f(V a,S m){return __builtin_shuffle(a,m);}",
        "V f(V a,F m){return __builtin_shuffle(a,m);}",
        "V f(V a,U b,V m){return __builtin_shuffle(a,b,m);}",
        "V f(V a,F b,V m){return __builtin_shuffle(a,b,m);}",
        "V f(V a){return __builtin_shuffle(a,1);}",
        "V f(V a){return __builtin_shuffle(a,a,a,a);}",
        "V f(V a){return __builtin_shuffle(a);}",
    ] {
        let source = format!("{TYPES}{body}");
        for target in [
            Target::X86_64UnknownLinuxGnu,
            Target::Aarch64UnknownLinuxGnu,
        ] {
            let a = analyze(&source, target).unwrap_err();
            let b = analyze_with_options(
                &source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..AnalysisOptions::default()
                },
            )
            .unwrap_err();
            assert_eq!((&a.message, a.offset), (&b.message, b.offset));
        }
    }
}

#[test]
#[ignore = "requires genuine GNU GCC and Clang; run with --include-ignored"]
fn native_shuffle_modulo_matches_gcc_and_clang_rejects_the_name() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let source = format!("{TYPES}{VALID}");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let version = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        !String::from_utf8_lossy(&version.stdout).contains("clang"),
        "set TOUCAN_GCC to genuine GNU GCC"
    );
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("shuffle.c");
    std::fs::write(&input, &source).unwrap();
    for optimization in ["-O0", "-O2"] {
        let binary = temp.path().join(if cfg!(windows) {
            "shuffle.exe"
        } else {
            "shuffle"
        });
        let output = Command::new(&gcc)
            .args(["-std=gnu11", optimization])
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
        assert!(Command::new(binary).status().unwrap().success());
    }
    for target in Target::ALL {
        let mut child = Command::new("clang")
            .args([
                "-target",
                target.triple(),
                "-std=gnu11",
                "-fsyntax-only",
                "-x",
                "c",
                "-",
            ])
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
        assert_eq!(toucan_test_support::compiler_acceptance(&output), Ok(false));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unknown builtin '__builtin_shuffle'")
        );
    }
}
