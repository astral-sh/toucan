use toucan_semantic::{AnalysisOptions, SymbolBinding, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "extern int f(void) __attribute__((weak)); int call(void) { return f ? f() : 0; }",
    "extern int x __attribute__((weak)); int read(void) { return &x ? x : 0; }",
    "int x __attribute__((weak));",
    "int x __attribute__((weak)) = 1;",
    "int __attribute__((weak)) f(void) { return 1; }",
    "void f(void) { extern int x __attribute__((weak)); if (&x) x = 1; }",
    "void f(void) { int x(void) __attribute__((weak)); if (x) x(); }",
    "extern int f(void) __attribute__((weak, weak));",
    "int f(void); int g(void) { return f(); } int f(void) __attribute__((weak));",
    "int f(void); int (*p)(void) = f; int f(void) __attribute__((weak));",
    "extern int x __attribute__((weak)); int x = 7;",
    "void f(void) { extern int x __attribute__((weak)); if (&x) x=1; } extern int x;",
    "int (x) __attribute__((weak));",
    "__attribute__((weak)) int (x);",
    "__attribute__((weak)) int x, y;",
    "extern int renamed __asm__(\"actual_name\") __attribute__((weak)); int use(void) { return &renamed ? renamed : 0; }",
];
const INVALID: &[&str] = &[
    "static int f(void) __attribute__((weak));",
    "static int x __attribute__((weak));",
    "void f(void) { int x __attribute__((weak)); }",
    "void f(void) { static int x __attribute__((weak)); }",
    "struct S { int x __attribute__((weak)); };",
    "typedef int A __attribute__((weak));",
    "static int f(void); extern int f(void) __attribute__((weak));",
    "extern int f(void) __attribute__((weak(1)));",
    "static int x; void f(void) { extern int x __attribute__((weak)); }",
];

#[test]
fn weak_symbols_require_external_declarations_and_keep_retention_parity() {
    for target in Target::ALL {
        for (source, accepted) in VALID
            .iter()
            .map(|source| (*source, true))
            .chain(INVALID.iter().map(|source| (*source, false)))
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
            "void f(int x __attribute__((weak)));",
            "struct __attribute__((weak)) S { int x; };",
            "enum E { x __attribute__((weak)) };",
            "void f(void) { sizeof(int __attribute__((weak))); }",
            "typedef int (*P)(void) __attribute__((weak));",
            "extern int x __attribute__((weakref));",
            "extern int x __attribute__((alias(\"y\")));",
            "extern int a __asm__(\"shared\") __attribute__((weak)); extern int b __asm__(\"shared\");",
            "extern int shared __attribute__((weak)); extern int b __asm__(\"shared\");",
            "void f(void) { extern int shared __attribute__((weak)); } extern int b __asm__(\"shared\");",
        ] {
            assert!(analyze(source, target).is_err(), "{target}: {source}");
        }
    }
}

#[test]
fn redeclarations_expose_effective_binding_and_explicit_attribute_spans() {
    let source = "extern int optional; void use(void) { extern int optional __attribute__((__weak__)); if (&optional) optional = 1; } extern int optional;";
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
            .find(|item| item.name == "optional")
            .unwrap();
        assert_eq!(declaration.symbol_binding, SymbolBinding::Weak);
        let code = analysis.checked().unwrap();
        let (id, entity) = code
            .entities()
            .find(|(_, entity)| entity.name() == Some("optional"))
            .unwrap();
        assert_eq!(entity.symbol_binding(), SymbolBinding::Weak);
        let sites = code
            .declarations()
            .filter(|(_, site)| site.entity() == id)
            .map(|(_, site)| site)
            .collect::<Vec<_>>();
        assert_eq!(sites.len(), 3);
        assert_eq!(
            sites
                .iter()
                .map(|site| site.symbol_binding())
                .collect::<Vec<_>>(),
            [
                SymbolBinding::Strong,
                SymbolBinding::Weak,
                SymbolBinding::Weak
            ]
        );
        assert!(sites[0].weak_attribute().is_none());
        let span = sites[1].weak_attribute().unwrap();
        assert!(source[span.range()].contains("__weak__"));
        assert!(sites[2].weak_attribute().is_none());
    }
}

#[test]
fn late_weak_definitions_follow_the_target_compiler_profile() {
    for source in [
        "int f(void) { return 1; } int f(void) __attribute__((weak));",
        "int x = 7; extern int x __attribute__((weak));",
    ] {
        for target in Target::ALL {
            let result = analyze(source, target);
            if matches!(
                target,
                Target::X86_64UnknownLinuxGnu | Target::Aarch64UnknownLinuxGnu
            ) {
                let unit = result.unwrap();
                assert_eq!(unit.declarations[0].symbol_binding, SymbolBinding::Weak);
            } else {
                assert!(result.unwrap_err().message.contains("must precede"));
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang cross targets; run with --include-ignored"]
fn weak_declaration_constraints_match_gcc_and_clang() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("weak.c");
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    for (compiler, targets) in [
        (gcc.as_str(), vec![None]),
        ("clang", Target::ALL.iter().copied().map(Some).collect()),
    ] {
        for target in targets {
            for (source, accepted) in VALID
                .iter()
                .map(|source| (*source, true))
                .chain(INVALID.iter().map(|source| (*source, false)))
            {
                std::fs::write(&input, format!("{source}\n")).unwrap();
                let mut command = std::process::Command::new(compiler);
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
