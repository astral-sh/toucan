use std::process::Command;

use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "int f(void) __attribute__((returns_twice));",
    "void f(void) __attribute__((__returns_twice__));",
    "int f(void) __attribute__((returns_twice()));",
    "int __attribute__((returns_twice)) f(void) { return 1; }",
    "static int __attribute__((returns_twice)) f(void) { return 1; }",
    "typedef int F(void); F f __attribute__((returns_twice));",
    "int (*f(void))(int) __attribute__((returns_twice));",
    "int f(void); int f(void) __attribute__((returns_twice)); int f(void);",
    "int f(void) __attribute__((returns_twice)); int f(void) { return 1; } int f(void) __attribute__((returns_twice));",
    "void g(void) { int f(void) __attribute__((returns_twice)); } int f(void);",
    "int f(void); void g(void) { int f(void) __attribute__((returns_twice)); int f(void); }",
    "int f(void) __attribute__((returns_twice)); void g(void) { int (*f)(void); }",
    "int f(void) __asm__(\"checkpoint\") __attribute__((returns_twice));",
];
const INVALID: &[&str] = &[
    "int f(void) __attribute__((returns_twice(1)));",
    "int f(void) __attribute__((returns_twice(1,2)));",
    "int x __attribute__((returns_twice));",
    "int (*f)(void) __attribute__((returns_twice));",
    "typedef int F(void) __attribute__((returns_twice));",
    "void g(int f __attribute__((returns_twice)));",
    "struct S { int f __attribute__((returns_twice)); };",
    "struct __attribute__((returns_twice)) S { int f; };",
    "void g(void) { int x __attribute__((returns_twice)); }",
    "void g(void) { typedef int F(void) __attribute__((returns_twice)); }",
];

#[test]
fn returns_twice_constraints_and_retention_agree() {
    for target in Target::ALL {
        for (source, accepted) in VALID
            .iter()
            .map(|s| (*s, true))
            .chain(INVALID.iter().map(|s| (*s, false)))
        {
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
                (Err(plain), Err(retained)) => assert_eq!(
                    (plain.offset, plain.message),
                    (retained.offset, retained.message)
                ),
                _ => unreachable!(),
            }
        }
        for source in [
            "int f(void) __attribute__((returns_twice,noreturn));",
            "int f(void) __attribute__((noreturn,returns_twice));",
            "int f(void) __attribute__((returns_twice)); _Noreturn int f(void);",
            "_Noreturn int f(void); void g(void) { int f(void) __attribute__((returns_twice)); }",
            "int f(void) __asm__(\"same\") __attribute__((returns_twice)); int g(void) __asm__(\"same\");",
            "int f(void) __attribute__((returns_twice)); int g(void) __asm__(\"f\");",
            "void g(void) { int f(void) __attribute__((returns_twice)); } int h(void) __asm__(\"f\");",
            "void g(void) { int f(void) __asm__(\"different\") __attribute__((returns_twice)); }",
            "void f(void) { sizeof(int __attribute__((returns_twice))); }",
        ] {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
    }
}

#[test]
fn returns_twice_redeclarations_keep_site_and_entity_identity() {
    // The UTF-16 escape invokes the parser adapter before the later attribute.
    let source = r#"unsigned short text[] = u"\U0001F600";
        int f(void);
        void g(void) { int f(void) __attribute__((__returns_twice__)); int f(void); }
        int f(void);
        void h(void) { int (*f)(void); }
    "#;
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
        let declaration = analysis
            .unit()
            .declarations
            .iter()
            .find(|item| item.name == "f")
            .unwrap();
        assert!(declaration.returns_twice);
        let code = analysis.checked().unwrap();
        let (id, entity) = code
            .entities()
            .find(|(_, entity)| entity.name() == Some("f") && entity.declaration().is_some())
            .unwrap();
        assert!(entity.returns_twice());
        let sites = code
            .declarations()
            .filter(|(_, site)| site.entity() == id)
            .map(|(_, site)| site)
            .collect::<Vec<_>>();
        assert_eq!(sites.len(), 4);
        assert_eq!(
            sites
                .iter()
                .map(|site| site.returns_twice())
                .collect::<Vec<_>>(),
            [false, true, true, true]
        );
        assert!(sites[0].returns_twice_attribute().is_none());
        let span = sites[1].returns_twice_attribute().unwrap();
        assert_eq!(&source[span.range()], "__returns_twice__");
        assert!(!span.synthetic());
        assert!(sites[2].returns_twice_attribute().is_none());
        let shadow = code
            .entities()
            .find(|(other, entity)| *other != id && entity.name() == Some("f"))
            .unwrap()
            .1;
        assert!(!shadow.returns_twice());
    }
}

#[test]
fn returns_twice_late_definition_follows_compiler_profile() {
    let source = "int f(void) { return 1; } int f(void) __attribute__((returns_twice));";
    for target in Target::ALL {
        let result = analyze(source, target);
        if matches!(
            target,
            Target::I686UnknownLinuxGnu
                | Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        ) {
            assert!(result.unwrap().declarations[0].returns_twice);
        } else {
            assert!(result.unwrap_err().message.contains("must precede"));
        }
    }
}

#[test]
#[ignore = "requires GNU GCC and Clang with five cross targets; run with --include-ignored"]
fn returns_twice_constraints_match_native_compilers() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("probe.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let version = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(
        !String::from_utf8_lossy(&version.stdout)
            .to_lowercase()
            .contains("clang"),
        "GNU GCC is required"
    );
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        for target in targets {
            for (source, accepted) in VALID
                .iter()
                .map(|s| (*s, true))
                .chain(INVALID.iter().map(|s| (*s, false)))
                .chain(std::iter::once((
                    "int f(void) { return 1; } int f(void) __attribute__((returns_twice));",
                    compiler != "clang",
                )))
            {
                std::fs::write(&input, format!("{source}\n")).unwrap();
                let mut command = Command::new(compiler);
                command.args(["-std=c11", "-Werror=attributes", "-fsyntax-only"]);
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
