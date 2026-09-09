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
    assert!(!run("int x;", "check", &["--std=c23"]).status.success());
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

#[test]
fn command_line_overrides_remove_and_replace_feature_operators() {
    for name in ["__has_builtin", "__has_attribute"] {
        for args in [
            vec![format!("-U{name}")],
            vec![format!("-D{name}(x)=1"), format!("-U{name}")],
        ] {
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let source = format!("#ifdef {name}\n#error operator still defined\n#endif\nint x;\n");
            let output = run(&source, "check", &args);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let undefine = format!("-U{name}");
        let define = format!("-D{name}(x)=7");
        let source = format!("_Static_assert({name}(unknown)==7, \"override\");");
        let output = run(&source, "check", &[&undefine, &define]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn c90_compilation_and_preprocessing_keep_their_comment_policies() {
    let source = "_Static_assert(B==6,\"ordered definitions\");";
    let flags = ["--std=c90", "-DA=1//first", "-UA", "-DB=6//**/2"];
    let output = run(source, "check", &flags);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = run("B\n", "preprocess", &flags);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pp = String::from_utf8(output.stdout).unwrap();
    assert!(pp.ends_with("6 / 2\n"), "{pp}");
    let output = run(
        "_Static_assert(B==3,\"ordered definitions\");",
        "check",
        &["--std=c90", "-DB=6//**/2", "-DA=1//first", "-UA"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn c99_and_c17_aliases_select_predefines_and_retained_report_modes() {
    for (spelling, mode, version) in [
        ("c99", "c99", 199901),
        ("c9x", "c99", 199901),
        ("iso9899:1999", "c99", 199901),
        ("gnu9x", "gnu99", 199901),
        ("c17", "c17", 201710),
        ("c18", "c17", 201710),
        ("iso9899:2018", "c17", 201710),
        ("gnu18", "gnu17", 201710),
    ] {
        let source = format!(
            "_Static_assert(__STDC_VERSION__=={version}L,\"version\");int f(int*restrict p){{return *p;}}"
        );
        let flag = format!("--std={spelling}");
        let output = run(&source, "inspect", &["--std=c90", &flag, "--checked-code"]);
        assert!(
            output.status.success(),
            "{spelling}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["translation_unit"]["language_mode"], mode);
    }
}
