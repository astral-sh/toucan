use std::io::Write;
use std::process::{Command, Stdio};

use toucan_semantic::checked::{EntityKind, Linkage, Storage};
use toucan_semantic::{AnalysisOptions, analyze, analyze_with_options};
use toucan_target::Target;

const VALID: &[&str] = &[
    "_Thread_local int x;",
    "static _Thread_local int x=3; extern _Thread_local int x;",
    "_Thread_local static int x; _Thread_local extern int y;",
    "extern _Thread_local int x; _Thread_local int x=7; extern _Thread_local int x;",
    "_Thread_local int x[]; extern _Thread_local int x[3];",
    "extern _Thread_local struct Incomplete x;",
    "_Thread_local struct S { int x; };",
    "void f(void) { _Thread_local struct S { int x; }; }",
    "void f(void) { static _Thread_local int x=1; x++; }",
    "void f(void) { extern _Thread_local int x; extern _Thread_local int x; x++; }",
    "void f(void) { extern _Thread_local int x; } _Thread_local int x=1;",
    "static _Thread_local int x; void f(void) { extern _Thread_local int x; x++; }",
    "_Thread_local int x; void f(void) { static int x; x++; }",
    "int x; void f(void) { static _Thread_local int x; x++; }",
    "_Thread_local int x; void f(void) { static int x; { extern _Thread_local int x; x++; } }",
    "void f(void) { static _Thread_local int x; { extern int x; x++; } }",
    "int x; _Thread_local int *p=&x;",
    "_Thread_local const char *p=\"hello\";",
    "_Thread_local int *p=(int[]){1,2};",
    "_Thread_local int x; int *f(void) { int *p=&x; return p; }",
    "void f(int n) { static _Thread_local int (*p)[n]; p=0; }",
    "_Thread_local int x; _Static_assert(sizeof x == sizeof(int), \"size\");",
    "_Thread_local int x; _Static_assert(_Alignof(__typeof__(x)) == _Alignof(int), \"align\");",
    "extern _Thread_local int x __attribute__((weak));",
    "__thread int x; static __thread int y; extern __thread int z;",
    "extern const __thread int x;",
    "_Thread_local int x; extern __thread int x;",
    "void f(void) { static __thread int x; extern __thread int y; x=y; }",
];
const INVALID: &[&str] = &[
    "_Thread_local int f(void);",
    "void f(_Thread_local int x);",
    "void f(__thread int x);",
    "typedef _Thread_local int T;",
    "typedef __thread int T;",
    "_Thread_local _Thread_local int x;",
    "__thread _Thread_local int x;",
    "auto _Thread_local int x;",
    "register _Thread_local int x;",
    "void f(void) { _Thread_local int x; }",
    "void f(void) { __thread int x; }",
    "void f(void) { static _Thread_local int g(void); }",
    "void f(void) { extern _Thread_local int g(void); }",
    "void f(void) { for (static _Thread_local int i=0; i<2; i++) {} }",
    "int x; _Thread_local int x;",
    "_Thread_local int x; int x;",
    "_Thread_local int x; extern int x;",
    "_Thread_local int x; void f(void) { extern int x; }",
    "int x; void f(void) { extern _Thread_local int x; }",
    "void f(void) { extern _Thread_local int x; } int x;",
    "void f(void) { extern int x; } _Thread_local int x;",
    "void f(void) { extern _Thread_local int x; } void g(void) { extern int x; }",
    "void f(void) { static _Thread_local int x; extern _Thread_local int x; }",
    "void f(void) { extern _Thread_local int x; } static _Thread_local int x;",
    "void f(void) { extern _Thread_local int x=1; }",
    "int f(void); _Thread_local int x=f();",
    "int f(void); void g(void) { static _Thread_local int x=f(); }",
    "_Thread_local int x; static int *p=&x;",
    "_Thread_local int x; _Thread_local int *p=&x;",
    "_Thread_local int x[3]; int *p=x;",
    "_Thread_local struct S { int x; } x; int *p=&x.x;",
    "_Thread_local int x; void f(void) { static int *p=&x; }",
    "void f(void) { static _Thread_local int x; static int *p=&x; }",
    "void f(void) { static _Thread_local int *p=(int*)&p; }",
    "void f(int n) { static _Thread_local int x[n]; }",
    "struct S { _Thread_local int x; };",
];

#[test]
fn thread_storage_constraints_preserve_analysis_parity() {
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
        // Clang accepts reversed GNU spelling as an extension, with a warning.
        let clang = !matches!(
            target,
            Target::X86_64UnknownLinuxGnu
                | Target::X86_64UnknownLinuxMusl
                | Target::Aarch64UnknownLinuxGnu
                | Target::Aarch64UnknownLinuxMusl
        );
        assert_eq!(analyze("__thread static int x;", target).is_ok(), clang);
    }
}

#[test]
fn explicit_tls_lowering_models_remain_unsupported() {
    for target in Target::ALL {
        let error = analyze(
            "extern __thread int x __attribute__((tls_model(\"initial-exec\")));",
            target,
        )
        .unwrap_err();
        assert!(
            error
                .message
                .contains("unsupported C attribute `tls_model`"),
            "{error}"
        );
    }
}

#[test]
fn thread_storage_is_an_object_property_independent_of_linkage_and_type() {
    let source = r#"
        extern _Thread_local int shared;
        static _Thread_local int internal=2;
        _Thread_local const char *text="hello";
        void f(int n) {
            extern _Thread_local int shared;
            extern _Thread_local int internal;
            static _Thread_local int local[3]={1,2};
            static _Thread_local int (*vla_pointer)[n];
            int *runtime=&shared;
            { static int shared=4; shared++; }
            *runtime=local[0]; vla_pointer=0;
        }
        _Thread_local int shared=1;
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
        let code = analysis.checked().unwrap();
        let variables: Vec<_> = code
            .entities()
            .filter(|(_, entity)| entity.kind() == EntityKind::Variable)
            .collect();
        for name in ["shared", "internal", "text", "local", "vla_pointer"] {
            let (_, entity) = variables
                .iter()
                .find(|(_, e)| e.name() == Some(name) && e.storage() == Storage::Thread)
                .unwrap();
            assert_eq!(
                entity.linkage(),
                match name {
                    "internal" => Linkage::Internal,
                    "shared" | "text" => Linkage::External,
                    _ => Linkage::None,
                }
            );
        }
        assert!(variables.iter().any(|(_, e)| e.name() == Some("shared")
            && e.storage() == Storage::Static
            && e.linkage() == Linkage::None));
        for (_, site) in code.declarations() {
            let entity = code.entity(site.entity()).unwrap();
            assert_eq!(site.storage(), entity.storage());
            if site.storage() == Storage::Thread {
                let owner = code
                    .occurrence(site.occurrence())
                    .unwrap()
                    .type_owner()
                    .unwrap();
                let range = code.occurrence(owner).unwrap().source().range();
                assert!(source[range].contains("_Thread_local"));
            }
            if let Some(initializer) = site.initializer() {
                assert_eq!(
                    code.initializer(initializer).unwrap().requires_constant(),
                    matches!(site.storage(), Storage::Static | Storage::Thread)
                );
            }
            if let Some(index) = entity.declaration() {
                assert_eq!(
                    analysis.unit().declarations[index].is_thread_local,
                    site.storage() == Storage::Thread
                );
            }
        }
        assert!(
            analysis
                .unit()
                .declarations
                .iter()
                .filter(|d| d.is_thread_local)
                .all(|d| analysis.unit().resolve(&d.ty).is_ok())
        );
    }
}

fn compiler(command: &str) -> String {
    std::env::var(if command == "gcc" {
        "TOUCAN_GCC"
    } else {
        "TOUCAN_CLANG"
    })
    .unwrap_or_else(|_| command.into())
}
fn compile(command: &str, target: Option<Target>, source: &str, accepted: bool, strict: bool) {
    let mut cc = Command::new(command);
    if let Some(target) = target {
        cc.args(["-target", target.triple()]);
        if matches!(
            target,
            Target::X86_64AppleDarwin | Target::Aarch64AppleDarwin
        ) {
            cc.arg("-mmacosx-version-min=11.0");
        }
    }
    cc.args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"]);
    if strict {
        cc.arg("-pedantic-errors");
    }
    let mut child = cc
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{source}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(
        toucan_test_support::compiler_acceptance(&output),
        Ok(accepted),
        "{cc:?}: {source}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires genuine GNU GCC and target-capable Clang; run with --include-ignored"]
fn thread_storage_matches_gcc_and_clang_on_five_targets() {
    let gcc = compiler("gcc");
    let clang = compiler("clang");
    for (cc, is_clang) in [(&gcc, false), (&clang, true)] {
        let version = Command::new(cc).arg("--version").output().unwrap();
        assert!(version.status.success());
        assert_eq!(
            String::from_utf8_lossy(&version.stdout)
                .to_ascii_lowercase()
                .contains("clang"),
            is_clang,
            "{cc}"
        );
    }
    for (source, accepted) in VALID
        .iter()
        .map(|s| (*s, true))
        .chain(INVALID.iter().map(|s| (*s, false)))
    {
        let strict = true;
        compile(&gcc, None, source, accepted, strict);
        for target in Target::ALL {
            compile(&clang, Some(target), source, accepted, strict);
        }
    }
    compile(&gcc, None, "__thread static int x;", false, false);
    for target in Target::ALL {
        compile(&clang, Some(target), "__thread static int x;", true, false);
    }
}
