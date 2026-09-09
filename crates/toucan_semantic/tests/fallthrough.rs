use std::process::Command;
use toucan_semantic::checked::StatementKind;
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "switch(x){case 0:x++;F;case 1:return x;}",
    "switch(x){case 0:if(x)F;else x++;case 1:return x;}",
    "switch(x){case 0:if(x)F;else F;case 1:return x;}",
    "switch(x){case 0:{F;}case 1:return x;}",
    "switch(x){case 0:F;;case 1:return x;}",
    "switch(x){case 0:F;typedef int T;case 1:return x;}",
    "switch(x){case 0:F;struct T{int y;};case 1:return x;}",
    "switch(x){case 0:F;int other(void);case 1:return x;}",
    "switch(x){case 0:F;_Static_assert(1,\"ok\");case 1:return x;}",
    "switch(x){case 0:F;label:;case 1:return x;}",
    "switch(x){case 0:F;label:{}case 1:return x;}",
    "switch(x){case 0:F;first:second:;case 1:return x;}",
    "switch(x){case 0:F;first:second:{}case 1:return x;}",
    "switch(x){case 0:F;label:{case 1:return x;}}",
    "switch(x){case 0:while(x){F;case 1:return x;}}",
    "switch(x){case 0:if(x){F;case 1:return x;}}",
];
const INVALID: &[&str] = &[
    "F;",
    "switch(x){case 0:F;}",
    "switch(x){case 0:F;return x;case 1:return x;}",
    "switch(x){case 0:F;int y;case 1:return x;}",
    "switch(x){case 0:F;extern int y;case 1:return x;}",
    "switch(x){case 0:F;static int y;case 1:return x;}",
    "switch(x){case 0:F;{typedef int T[x++];}case 1:return x;}",
    "switch(x){case 0:F;{typedef typeof(int[x++]) T;}case 1:return x;}",
    "switch(x){case 0:F;label:case 1:return x;}",
    "switch(x){case 0:while(x){F;}case 1:return x;}",
    "switch(x){case 0:(void)({F;});case 1:return x;}",
    "switch(x){case 0:__attribute__((fallthrough(1)));case 1:return x;}",
    "switch(x){case 0:__attribute__((unused));case 1:return x;}",
    "switch(x){case 0:__attribute__((error(\"bad\")));case 1:return x;}",
    "switch(x){case 0:F;F;case 1:return x;}",
];
fn source(body: &str) -> String {
    format!(
        "int f(int x) {{ {} return x; }}\n",
        body.replace('F', "__attribute__((__fallthrough__))")
    )
}

#[test]
fn fallthrough_annotations_follow_switch_control_flow_and_retain_targets() {
    let options = AnalysisOptions {
        retain_code: true,
        ..AnalysisOptions::default()
    };
    for target in Target::ALL {
        for body in VALID {
            let source = source(body);
            let plain =
                analyze(&source, target).unwrap_or_else(|error| panic!("{source}: {error}"));
            let checked = analyze_with_options(&source, target, &options).unwrap();
            assert_eq!(format!("{plain:?}"), format!("{:?}", checked.unit()));
            let code = checked.checked().unwrap();
            let mut count = 0;
            for (_, statement) in code.statements() {
                if let StatementKind::Fallthrough { switch } = statement.kind() {
                    assert!(matches!(
                        code.statement(*switch).unwrap().kind(),
                        StatementKind::Switch { .. }
                    ));
                    let span = code.occurrence(statement.occurrence()).unwrap().source();
                    assert!(source[span.range()].contains("__fallthrough__"));
                    count += 1;
                }
            }
            assert_eq!(count, body.matches('F').count());
        }
        for body in INVALID {
            let source = source(body);
            let plain = analyze(&source, target).unwrap_err();
            let checked = analyze_with_options(&source, target, &options).unwrap_err();
            assert_eq!(
                (plain.offset, plain.message),
                (checked.offset, checked.message),
                "{source}"
            );
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn fallthrough_constraints_match_compiler_codegen_checks() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("fallthrough.c");
    let object = directory.path().join("fallthrough.o");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for compiler in [gcc.as_str(), "clang"] {
        for body in VALID {
            std::fs::write(&input, source(body)).unwrap();
            for optimization in ["-O0", "-O2"] {
                let result = Command::new(compiler)
                    .args(["-std=gnu11", "-Werror", optimization, "-c"])
                    .arg(&input)
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{compiler}: {body}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
    // Clang checks placement consistently. GCC defers some checks until
    // lowering and accepts additional placements, including object declarations.
    for body in INVALID {
        std::fs::write(&input, source(body)).unwrap();
        let result = Command::new("clang")
            .args(["-std=gnu11", "-Werror", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(!result.status.success(), "clang accepted {body}");
    }
}
