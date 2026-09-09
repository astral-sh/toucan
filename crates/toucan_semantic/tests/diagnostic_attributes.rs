use toucan_semantic::checked::{DiagnosticAttributeKind, Limits};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "extern void f(void) __attribute__((warning(\"message\")));",
    "extern void f(void) __attribute__((__error__(\"message\")));",
    "__attribute__((warning(\"prefix\"))) void f(void), g(void);",
    "int (__attribute__((warning(\"nested\"))) f)(void);",
    "__attribute__((warning(\"definition\"))) void f(void) {}",
    "void f(void) __attribute__((warning(\"first\"), warning(\"second\")));",
    "void f(void) __attribute__((warning(\"\")));",
    "void f(void) __attribute__((warning(\"line\\n\" \"\\u00e9\")));",
    "void f(void) __attribute__((warning(\"literal \\\\0\")));",
    "void g(void) { extern void f(void) __attribute__((warning(\"local\"))); extern void f(void) __attribute__((warning(\"again\"))); f(); }",
];

const UNSUPPORTED: &[&str] = &[
    "void f(void) __attribute__((warning));",
    "void f(void) __attribute__((warning(1)));",
    "void f(void) __attribute__((warning(\"a\", \"b\")));",
    "int x __attribute__((warning(\"object\")));",
    "typedef int F(void) __attribute__((warning(\"type\")));",
    "int (*p)(void) __attribute__((warning(\"pointer\")));",
    "int f(void p(void) __attribute__((warning(\"parameter\"))));",
    "struct S { int x __attribute__((warning(\"field\"))); };",
    "struct __attribute__((warning(\"record\"))) S;",
    "enum E { X __attribute__((warning(\"enumerator\"))) };",
    "typedef double __attribute__((warning(\"type\"))) _Float32;",
    "void f(void) { __attribute__((warning(\"empty\"))) int; }",
    "void f(void) { typedef int F(void) __attribute__((warning(\"type\"))); }",
    "void f(void) { sizeof(int __attribute__((warning(\"type\")))); }",
    "void f(void) __attribute__((warning(L\"wide\")));",
    "void f(void) __attribute__((warning(\"numeric\\x41\")));",
    "void f(void) __attribute__((warning(\"interior\\0nul\")));",
    "void f(void) __attribute__((diagnose_if(1,\"stop\",\"error\")));",
    "void f(void) __attribute__((enable_if(1,\"condition\")));",
];

const CONFLICTS: &[&str] = &[
    "void f(void) __attribute__((warning(\"w\"),error(\"e\")));",
    "void f(void) __attribute__((warning(\"w\"))); void f(void) __attribute__((error(\"e\")));",
    "void a(void){extern void f(void) __attribute__((warning(\"w\")));} void b(void){extern void f(void) __attribute__((error(\"e\")));}",
    "void a(void){extern void f(void) __attribute__((warning(\"w\")));} void f(void) __attribute__((error(\"e\")));",
];

#[test]
fn diagnostic_attributes_validate_arguments_and_supported_attachments() {
    for target in Target::ALL {
        for source in VALID {
            let plain = analyze(source, target).unwrap_or_else(|error| panic!("{source}: {error}"));
            let retained = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(format!("{plain:?}"), format!("{:?}", retained.unit()));
            assert!(
                !retained
                    .checked()
                    .unwrap()
                    .diagnostic_attributes()
                    .is_empty()
            );
        }
        for source in UNSUPPORTED {
            let plain = analyze(source, target).unwrap_err();
            let retained = analyze_with_options(
                source,
                target,
                &AnalysisOptions {
                    retain_code: true,
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert_eq!(
                (plain.offset, plain.message),
                (retained.offset, retained.message),
                "{source}"
            );
        }
        for source in CONFLICTS {
            let accepted = matches!(
                target,
                Target::X86_64UnknownLinuxGnu
                    | Target::X86_64UnknownLinuxMusl
                    | Target::Aarch64UnknownLinuxGnu
                    | Target::Aarch64UnknownLinuxMusl
            );
            for retain_code in [false, true] {
                let result = analyze_with_options(
                    source,
                    target,
                    &AnalysisOptions {
                        retain_code,
                        ..Default::default()
                    },
                );
                assert_eq!(result.is_ok(), accepted, "{target}: {source}: {result:?}");
                if let Err(error) = result {
                    assert!(error.message.contains("conflicting warning and error"));
                }
            }
        }
    }
}

#[test]
fn written_sites_retain_messages_redeclarations_and_original_spans() {
    let source = "struct S {int x;}; struct S s = (struct S){}; __attribute__((warning(\"prefix\\n\" \"\\u00e9\"))) void a(void), b(void); void a(void) __attribute__((__error__(\"later\"))); void use(void) { extern void local(void) __attribute__((warning(\"local\"))); extern void local(void) __attribute__((error(\"twice\"))); local(); a(); }";
    let analysis = analyze_with_options(
        source,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    )
    .unwrap();
    let code = analysis.checked().unwrap();
    let facts: Vec<_> = code
        .diagnostic_attributes()
        .iter()
        .map(|attribute| {
            let site = code.declaration(attribute.declaration()).unwrap();
            let entity = code.entity(site.entity()).unwrap();
            let span = attribute.source().range();
            assert!(source[span.clone()].contains("warning") || source[span].contains("error"));
            (
                entity.name().unwrap(),
                site.entity(),
                attribute.kind(),
                attribute.message(),
            )
        })
        .collect();
    assert_eq!(facts.len(), 5);
    assert_eq!(facts[0].0, "a");
    assert_eq!(facts[1].0, "b");
    assert_eq!(facts[0].3, "prefix\né");
    assert_eq!(facts[1].3, "prefix\né");
    assert_eq!(facts[0].1, facts[2].1);
    assert_eq!(facts[2].2, DiagnosticAttributeKind::Error);
    assert_eq!(facts[2].3, "later");
    assert_eq!(facts[3].1, facts[4].1);
    assert_eq!(facts[3].3, "local");
    assert_eq!(facts[4].3, "twice");
    let long = format!(
        "void f(void) __attribute__((warning(\"{}\")));",
        "x".repeat(4096)
    );
    analyze(&long, Target::X86_64UnknownLinuxGnu).unwrap();
    let error = analyze_with_options(
        &long,
        Target::X86_64UnknownLinuxGnu,
        &AnalysisOptions {
            retain_code: true,
            retain_declaration_origins: false,
            retain_object_values: false,
            limits: Limits {
                payload_bytes: 2048,
                ..Default::default()
            },
            ..AnalysisOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.message.contains("payload"), "{error}");
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn diagnostic_attributes_match_checking_phase_and_record_codegen_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("attributes.c");
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        let version = std::process::Command::new(&compiler)
            .arg("--version")
            .output()
            .unwrap();
        assert!(version.status.success());
        let version = String::from_utf8(version.stdout).unwrap();
        let gnu = version.contains("Free Software Foundation");
        assert!(
            gnu || version.to_ascii_lowercase().contains("clang"),
            "{version}"
        );
        for source in CONFLICTS {
            std::fs::write(&input, format!("{source}\n")).unwrap();
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", "-pedantic-errors", "-fsyntax-only"])
                .arg(&input)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(gnu),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for source in VALID {
            std::fs::write(&input, format!("{source}\n")).unwrap();
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", "-pedantic-errors", "-fsyntax-only"])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for (body, error_o0, error_o2) in [
            ("f();", true, true),
            ("if (0) f();", false, false),
            ("int x=0; if(x) f();", true, false),
            ("sizeof(&f);", false, false),
        ] {
            let source = format!(
                "extern void f(void) __attribute__((error(\"reached\"))); void g(void) {{ {body} }}\n"
            );
            analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
            std::fs::write(&input, &source).unwrap();
            let output = std::process::Command::new(&compiler)
                .args(["-std=c11", "-fsyntax-only", "-Wno-unused-value"])
                .arg(&input)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            for (optimization, rejected) in [("-O0", error_o0), ("-O2", error_o2)] {
                let output = std::process::Command::new(&compiler)
                    .args(["-std=c11", optimization, "-Wno-unused-value", "-c"])
                    .arg(&input)
                    .arg("-o")
                    .arg(directory.path().join("attributes.o"))
                    .output()
                    .unwrap();
                assert_eq!(
                    toucan_test_support::compiler_acceptance(&output),
                    Ok(!rejected),
                    "{compiler} {optimization}: {body}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                if rejected {
                    assert!(String::from_utf8_lossy(&output.stderr).contains("reached"));
                }
            }
        }
        let source =
            "void f(void) __attribute__((warning(\"warning reached\"))); void g(void) { f(); }\n";
        std::fs::write(&input, source).unwrap();
        let output = std::process::Command::new(&compiler)
            .args(["-std=c11", "-O0", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(directory.path().join("attributes.o"))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("warning reached"));
    }
}

#[test]
#[ignore = "requires Clang with cross targets; run with --include-ignored"]
fn diagnostic_attributes_match_clang_target_constraints() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("attributes.c");
    for target in Target::ALL {
        for (source, accepted) in VALID
            .iter()
            .map(|source| (*source, true))
            .chain(CONFLICTS.iter().map(|source| (*source, false)))
            .chain(UNSUPPORTED.iter().take(3).map(|source| (*source, false)))
        {
            std::fs::write(&input, format!("{source}\n")).unwrap();
            let output = std::process::Command::new("clang")
                .args([
                    "-target",
                    target.triple(),
                    "-std=c11",
                    "-pedantic-errors",
                    "-fsyntax-only",
                ])
                .arg(&input)
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{target}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
