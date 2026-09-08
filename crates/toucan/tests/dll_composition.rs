use std::process::Command;

use toucan::semantic::{DllStorageClass, FunctionDefinitionKind};
use toucan::{AnalysisOptions, CompilerProfile, Target};

fn cases() -> Vec<(String, bool)> {
    let mut cases = Vec::new();
    for attribute in ["dllimport", "dllexport"] {
        for scope in [
            "void g(int x){",
            "void g(void){int x;",
            "void g(void){typedef int x;",
        ] {
            for (declaration, address) in [("int x", "&x"), ("int x[4]", "x")] {
                cases.push((format!("__declspec({attribute}) {declaration}; {scope} {{extern {declaration};static int*p={address};}} }}"), attribute == "dllexport"));
            }
            cases.push((format!("__declspec({attribute}) __inline__ int x(void){{return 1;}} {scope} {{extern int x(void);}} }} int(*p)(void)=x;"), true));
        }
    }
    for (source, accepted) in [
        (
            "void a(void){__declspec(dllimport) extern int x;} void b(int x){{extern int x;static int*p=&x;}}",
            false,
        ),
        (
            "extern int x;void a(void){{__declspec(dllimport) extern int x;{int x;{extern int x;static int*p=&x;}}}}",
            true,
        ),
        (
            "__declspec(dllimport) extern int x;void a(void){{__declspec(dllexport) extern int x;{int x;{extern int x;static int*p=&x;}}}}",
            false,
        ),
        (
            "__declspec(dllimport) extern int x;void a(void){{__declspec(dllexport) extern int x;}{int x;{extern int x;static int*p=&x;}}}",
            false,
        ),
        (
            "extern int x;void a(void){{__declspec(dllimport) extern int x;}{int x;{extern int x;static int*p=&x;}}}",
            true,
        ),
        (
            "__declspec(dllimport) extern int x;void a(void){static int x;static int*p=&x;}",
            true,
        ),
        (
            "__declspec(dllimport) extern int x;int x;static int*p=&x;",
            false,
        ),
        (
            "__declspec(dllimport) extern int x;int x=1;static int*p=&x;",
            false,
        ),
        (
            "void a(void){__declspec(dllimport) extern int x;} extern int x;static int*p=&x;",
            true,
        ),
        (
            "void a(void){__declspec(dllimport) extern int x;} void b(void){__declspec(dllexport) extern int x;static int*p=&x;}",
            false,
        ),
        (
            "__declspec(dllimport) extern int x;__declspec(dllimport) extern int x;extern int x;static int*p=&x;",
            false,
        ),
        (
            "void a(void){__declspec(dllimport) extern int x;} __declspec(dllimport) extern int x;extern int x;static int*p=&x;",
            false,
        ),
    ] {
        cases.push((source.into(), accepted));
    }
    // Constant addresses use the first linked declaration, even if later
    // declarations acquire or replace a DLL annotation.
    for first in [
        "",
        "extern int x;",
        "__declspec(dllimport) extern int x;",
        "__declspec(dllexport) extern int x;",
    ] {
        for second in [
            "extern int x;",
            "__declspec(dllimport) extern int x;",
            "__declspec(dllexport) extern int x;",
        ] {
            let imported = if first.is_empty() { second } else { first }.contains("dllimport");
            let canceled = !first.is_empty() && second == "extern int x;";
            cases.push((
                format!("{first}{second}static int*p=&x;"),
                !imported || canceled,
            ));
            cases.push((
                format!("{first}void g(void){{{second}static int*p=&x;}}"),
                !imported,
            ));
        }
    }
    for (name, result, parameters, arguments) in [
        ("malloc", "void *", "unsigned long long", "(1)"),
        (
            "calloc",
            "void *",
            "unsigned long long,unsigned long long",
            "(1,2)",
        ),
        (
            "realloc",
            "void *",
            "void*,unsigned long long",
            "((void*)0,2)",
        ),
        ("free", "void", "void*", "((void*)0)"),
        ("prefetch", "void", "const void*,...", "((void*)0)"),
    ] {
        for builtin in [false, true] {
            if name == "prefetch" && !builtin {
                continue;
            }
            let name = if builtin {
                format!("__builtin_{name}")
            } else {
                name.into()
            };
            for declaration in [
                String::new(),
                format!("{result} {name}({parameters});"),
                format!("{result} {name}({parameters}) __asm__(\"alias\");"),
            ] {
                for (body, evaluated) in [
                    ("(void)CALL;", true),
                    ("(void)sizeof((CALL,1));", false),
                    ("(void)sizeof(int[(CALL,n)]);", true),
                    ("(void)sizeof(sizeof(int[(CALL,n)]));", true),
                    ("(void)_Generic((CALL,1),int:0);", false),
                    ("if(0){(void)CALL;}", true),
                ] {
                    let accepted = if declaration.is_empty() {
                        builtin
                    } else {
                        !evaluated
                    };
                    let body = body.replace("CALL", &format!("{name}{arguments}"));
                    cases.push((format!("{declaration}void g(int n){{{body}}} __declspec(dllexport) {result} {name}({parameters});"), accepted));
                }
            }
        }
    }
    cases
}

fn compare(source: &str, accepted: bool) {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    let ordinary = toucan::semantic::analyze_with_profile(source, profile, &Default::default());
    let retained = toucan::semantic::analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    assert_eq!(ordinary.is_ok(), accepted, "{source}: {ordinary:?}");
    match (ordinary, retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            serde_json::to_value(a.unit()).unwrap(),
            serde_json::to_value(b.unit()).unwrap()
        ),
        (Err(a), Err(b)) => assert_eq!((a.offset, a.message), (b.offset, b.message), "{source}"),
        result => panic!("retention changes {source}: {result:?}"),
    }
}

#[test]
fn dll_scopes_constant_addresses_and_builtin_uses() {
    for (source, accepted) in cases() {
        compare(&source, accepted);
    }
}

#[test]
#[ignore = "requires Clang with the Microsoft C ABI"]
fn native_dll_composition_oracle() {
    let compiler = std::env::var_os("TOUCAN_CLANG").unwrap_or_else(|| "clang".into());
    let file = tempfile::NamedTempFile::new().unwrap();
    for (source, accepted) in cases() {
        std::fs::write(file.path(), &source).unwrap();
        let output = Command::new(&compiler)
            .args([
                "--target=x86_64-pc-windows-msvc",
                "-std=gnu11",
                "-fsyntax-only",
                "-x",
                "c",
            ])
            .arg(file.path())
            .output()
            .unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output).unwrap(),
            accepted,
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        compare(&source, accepted);
    }
}

#[test]
fn hidden_linked_functions_keep_site_storage_and_body_ownership() {
    for scope in [
        "void g(int x){",
        "void g(void){int x;",
        "void g(void){typedef int x;",
    ] {
        for weak in [false, true] {
            let attribute = if weak { "__attribute__((weak))" } else { "" };
            let source = format!(
                "__declspec(dllimport) {attribute} __inline__ int x(void){{return 1;}} {scope} {{extern int x(void);}} }} int(*p)(void)=x;"
            );
            for mode in toucan::LanguageMode::ALL {
                let analysis = toucan::semantic::analyze_with_profile(
                    &source,
                    CompilerProfile::default_for(Target::X86_64PcWindowsMsvc)
                        .with_language_mode(mode),
                    &AnalysisOptions {
                        retain_code: true,
                        ..Default::default()
                    },
                )
                .unwrap();
                let expected = if weak {
                    FunctionDefinitionKind::WeakInline
                } else {
                    FunctionDefinitionKind::InlineOnly
                };
                let declaration = analysis
                    .unit()
                    .declarations
                    .iter()
                    .find(|d| d.name == "x")
                    .unwrap();
                assert_eq!(declaration.function_definition_kind, Some(expected));
                let code = analysis.checked().unwrap();
                let (id, entity) = code
                    .entities()
                    .find(|(_, e)| {
                        e.name() == Some("x")
                            && e.kind() == toucan::semantic::checked::EntityKind::Function
                    })
                    .unwrap();
                assert_eq!(entity.dll_storage_class(), Some(DllStorageClass::Import));
                let sites = code
                    .declarations()
                    .filter(|(_, s)| s.entity() == id)
                    .collect::<Vec<_>>();
                assert_eq!(sites.len(), 2);
                assert!(
                    sites
                        .iter()
                        .all(|(_, s)| s.dll_storage_class() == Some(DllStorageClass::Import))
                );
                assert_eq!(
                    code.body(entity.body().unwrap()).unwrap().definition_kind(),
                    expected
                );
            }
        }
    }
}
