use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

const PREFIX: &str = "enum E { VALUE=-1 }; enum Other { OTHER=-1 }; typedef enum E EnumArray[2]; typedef int IntArray[2]; typedef const enum E ConstEnum;\n";

fn queries(compiler: Compiler) -> String {
    let mut source = String::from(PREFIX);
    for (left, right, expected) in [
        ("enum E", "int", true),
        ("const enum E", "const int", true),
        ("const enum E[2]", "const int[2]", true),
        ("const EnumArray", "const IntArray", true),
        ("enum E *", "int *", true),
        ("enum E *const *", "int *const *", true),
        ("enum E *const (*)[2]", "int *const (*)[2]", true),
        ("const enum E *", "const int *", false),
        ("volatile enum E **", "volatile int **", false),
        ("ConstEnum *", "const int *", false),
        ("const EnumArray *", "const IntArray *", false),
        ("const enum E (*)[2]", "const int (*)[2]", false),
        ("ConstEnum (*)[2]", "const int (*)[2]", false),
        ("const enum E (*)[2][3]", "const int (*)[2][3]", false),
        ("const enum E *(*)(void)", "const int *(*)(void)", false),
        ("void(const enum E)", "void(const int)", true),
        ("_Atomic(enum E *)", "_Atomic(int *)", true),
        ("_Atomic(const enum E *)", "_Atomic(const int *)", false),
        ("_Atomic(enum E)", "_Atomic(int)", true),
        ("const _Atomic(enum E)[2]", "const _Atomic(int)[2]", true),
        (
            "_Atomic(enum E) *",
            "_Atomic(int) *",
            compiler == Compiler::Clang,
        ),
        (
            "const _Atomic(enum E) (*)[2]",
            "const _Atomic(int) (*)[2]",
            compiler == Compiler::Clang,
        ),
        (
            "void(_Atomic(enum E))",
            "void(_Atomic(int))",
            compiler == Compiler::Clang,
        ),
        (
            "_Atomic(enum E)(void)",
            "_Atomic(int)(void)",
            compiler == Compiler::Clang,
        ),
        (
            "const enum E(void)",
            "const int(void)",
            compiler == Compiler::Gnu,
        ),
        ("const enum E *", "const enum E *", true),
        ("const enum E *", "const enum Other *", false),
    ] {
        for (left, right) in [(left, right), (right, left)] {
            source.push_str(&format!(
                "_Static_assert(__builtin_types_compatible_p({left},{right})=={},\"{left}/{right}\");\n",
                u8::from(expected)
            ));
        }
    }
    source.push_str(
        "_Static_assert(_Generic((const enum E*)0,const int*:2,default:1)==1,\"generic selection\");
         struct Buffer { char data[1+__builtin_types_compatible_p(const enum E*,const int*)]; };
         _Static_assert(sizeof(struct Buffer)==1,\"query-dependent layout\");\n",
    );
    source
}

const CONFLICTS: &[&str] = &[
    "void consume(const enum E *); void consume(const int *);",
    "extern const enum E *pointer; extern const int *pointer;",
    "extern const EnumArray *array; extern const IntArray *array;",
    "extern const enum E object; extern const int object;",
];

#[test]
fn qualified_enum_identity_survives_array_and_pointer_boundaries() {
    for profile in CompilerProfile::ALL {
        for retain_code in [false, true] {
            let options = AnalysisOptions {
                retain_code,
                ..Default::default()
            };
            analyze_with_profile(&queries(profile.compiler()), profile, &options)
                .unwrap_or_else(|error| panic!("{profile:?}, retained={retain_code}: {error}"));
            for conflict in CONFLICTS {
                let error = analyze_with_profile(&format!("{PREFIX}{conflict}"), profile, &options)
                    .unwrap_err();
                assert!(
                    error.message.contains("conflicting declaration"),
                    "{profile:?}: {conflict}: {error}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native GNU GCC and Clang target backends"]
fn qualified_enum_comparisons_match_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let commands = std::iter::once((gcc.as_str(), None)).chain(
        Target::ALL
            .into_iter()
            .map(|target| ("clang", Some(target))),
    );
    for (compiler, target) in commands {
        let profile = if target.is_none() {
            Compiler::Gnu
        } else {
            Compiler::Clang
        };
        for (source, accepted) in std::iter::once((queries(profile), true)).chain(
            CONFLICTS
                .iter()
                .map(|conflict| (format!("{PREFIX}{conflict}"), false)),
        ) {
            let mut command = Command::new(compiler);
            if let Some(target) = target {
                command.args(["-target", target.triple()]);
            }
            let mut child = command
                .args(["-std=gnu11", "-fsyntax-only", "-x", "c", "-"])
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
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                toucan_test_support::compiler_acceptance(&output),
                Ok(accepted),
                "{compiler} {target:?}: {source}: {stderr}"
            );
            if !accepted {
                assert!(
                    stderr.contains("conflicting") || stderr.contains("with a different type"),
                    "{stderr}"
                );
            }
        }
    }
}
