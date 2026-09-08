use std::process::{Command, Output};
fn run(source: &str, subcommand: &str, args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("mode.h");
    std::fs::write(&header, source).unwrap();
    Command::new(env!("CARGO_BIN_EXE_toucan"))
        .env("SOURCE_DATE_EPOCH", "0")
        .arg(subcommand)
        .arg(&header)
        .args([
            "--target",
            "x86_64-unknown-linux-gnu",
            "--compiler",
            "clang",
        ])
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn explicit_modes_change_keywords_and_report_the_selected_mode() {
    let source = "enum {asm=3,typeof=5};int f(void){return asm+typeof;}";
    let output = run(source, "inspect", &["--std=c11", "--checked-code"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["translation_unit"]["language_mode"], "c11");
    assert!(!run(source, "check", &["--std=gnu11"]).status.success());
    assert!(!run("int x;", "check", &["--std=c99"]).status.success());
}
#[test]
fn definitions_and_trigraphs_follow_occurrence_order() {
    for args in [
        vec!["--std=c11", "-U__STRICT_ANSI__", "-D__STRICT_ANSI__=7"],
        vec!["-D__STRICT_ANSI__=7", "--std=c11"],
        vec!["-UX", "-DX=7", "--std=c11"],
        vec!["-DF(x)=x", "-UF", "-DF=7", "--std=c11"],
    ] {
        let name = if args.iter().any(|arg| arg.starts_with("-D__STRICT")) {
            "__STRICT_ANSI__"
        } else if args.iter().any(|arg| arg.starts_with("-DX")) {
            "X"
        } else {
            "F"
        };
        let output = run(
            &format!("_Static_assert({name}==7,\"ordered\");"),
            "check",
            &args,
        );
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for (args, enabled) in [
        (vec!["--trigraphs", "--std=gnu11"], false),
        (vec!["--std=gnu11", "--trigraphs"], true),
        (vec!["--trigraphs=false", "--std=c11"], true),
        (vec!["--std=c11", "--trigraphs=false"], false),
        (vec!["--std=c11", "--trigraphs=false", "--trigraphs"], true),
    ] {
        let output = run("const char *p=\"??=\";", "preprocess", &args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert_eq!(text.contains("\"#\""), enabled, "{args:?}: {text}");
    }
}
