use std::path::{Path, PathBuf};
use std::process::Command;
use toucan::{Compiler, CompilerProfile, Config, Target, parse_source};

#[test]
fn predefined_integer_and_lock_free_contracts() {
    for profile in CompilerProfile::ALL {
        let config = Config::with_profile(profile);
        let gnu = profile.compiler() == Compiler::Gnu;
        let prefix = if gnu { "__GCC" } else { "__CLANG" };
        let mut source = String::new();
        for width in [8, 16, 32, 64] {
            for family in ["LEAST", "FAST"] {
                let bits = if gnu && family == "FAST" && matches!(width, 16 | 32) {
                    64
                } else {
                    width
                };
                source.push_str(&format!("_Static_assert(sizeof(__INT_{family}{width}_TYPE__)*8=={bits},\"signed size\");_Static_assert(sizeof(__UINT_{family}{width}_TYPE__)*8=={bits},\"unsigned size\");_Static_assert(__INT_{family}{width}_WIDTH__=={bits},\"width\");_Static_assert((__UINT_{family}{width}_TYPE__)__INT_{family}{width}_MAX__*2+1==__UINT_{family}{width}_MAX__,\"max\");"));
            }
        }
        for ty in [
            "BOOL", "CHAR", "CHAR16_T", "CHAR32_T", "WCHAR_T", "SHORT", "INT", "LONG", "LLONG",
            "POINTER",
        ] {
            source.push_str(&format!(
                "_Static_assert({prefix}_ATOMIC_{ty}_LOCK_FREE==2,\"lock free\");"
            ));
        }
        parse_source(Path::new("predefs.h"), &source, &config).unwrap();
    }
}

const OPERATIONS: &str = r#"
#include <stdatomic.h>
_Static_assert(ATOMIC_BOOL_LOCK_FREE == 2, "bool");
_Static_assert(ATOMIC_LLONG_LOCK_FREE == 2, "long long");
_Static_assert(ATOMIC_POINTER_LOCK_FREE == 2, "pointer");
int atomic_operations(void) {
    atomic_int n = ATOMIC_VAR_INIT(1); atomic_init(&n,2);
    atomic_store_explicit(&n,3,memory_order_release);
    int old = atomic_exchange(&n,4), expected=4;
    if (!atomic_compare_exchange_strong(&n,&expected,5)) return 1;
    expected=5; atomic_compare_exchange_weak_explicit(&n,&expected,6,memory_order_acq_rel,memory_order_acquire);
    atomic_fetch_add(&n,1); atomic_fetch_sub(&n,1);
    atomic_fetch_and(&n,7); atomic_fetch_or(&n,8); atomic_fetch_xor(&n,2);
    atomic_thread_fence(memory_order_seq_cst); atomic_signal_fence(memory_order_acquire);
    atomic_flag flag=ATOMIC_FLAG_INIT;
    atomic_flag_test_and_set(&flag); atomic_flag_clear_explicit(&flag,memory_order_release);
    return atomic_load(&n)+kill_dependency(old)+atomic_is_lock_free(&n);
}
"#;

fn include_dir(compiler: &str, arg: &str) -> PathBuf {
    let output = Command::new(compiler).arg(arg).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}
#[test]
#[ignore = "requires installed Clang resource headers and target backends"]
fn unchanged_clang_stdatomic_header_and_operations() {
    let directory = include_dir("clang", "-print-resource-dir").join("include");
    for target in Target::ALL {
        let mut config =
            Config::with_profile(CompilerProfile::new(target, Compiler::Clang).unwrap());
        config.preprocessor.include_dirs.push(directory.clone());
        config
            .preprocessor
            .defines
            .insert("__STDC_HOSTED__".into(), "0".into());
        config.analysis.retain_code = true;
        let parsed = parse_source(Path::new("stdatomic.c"), OPERATIONS, &config)
            .unwrap_or_else(|e| panic!("{target}: {e}"));
        assert!(
            parsed
                .checked()
                .unwrap()
                .expressions()
                .any(|(_, e)| matches!(
                    e.kind(),
                    toucan::semantic::checked::ExprKind::BuiltinCall {
                        builtin: toucan::semantic::checked::Builtin::C11Atomic(_),
                        ..
                    }
                ))
        );
        let mut ordinary = config.clone();
        ordinary.analysis.retain_code = false;
        let ordinary = parse_source(Path::new("stdatomic.c"), OPERATIONS, &ordinary).unwrap();
        assert_eq!(
            format!("{:?}", parsed.unit()),
            format!("{:?}", ordinary.unit())
        );
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), OPERATIONS).unwrap();
        let out = Command::new("clang")
            .args([
                "-target",
                target.triple(),
                "-std=c11",
                "-ffreestanding",
                "-nostdinc",
                "-isystem",
            ])
            .arg(&directory)
            .args(["-fsyntax-only", "-x", "c"])
            .arg(file.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
#[test]
#[ignore = "requires an installed GNU GCC stdatomic header"]
fn unchanged_gcc_stdatomic_header_and_operations() {
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    let directory = include_dir(&gcc, "-print-file-name=include");
    let source = format!(
        "{OPERATIONS}\n_Static_assert(sizeof(atomic_int_fast16_t)==8,\"GNU fast16\");_Static_assert(sizeof(atomic_int_fast32_t)==8,\"GNU fast32\");_Static_assert(ATOMIC_INT_LOCK_FREE==2,\"int\");"
    );
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        let mut config = Config::new(target);
        config.preprocessor.include_dirs.push(directory.clone());
        let ordinary = parse_source(Path::new("stdatomic.c"), &source, &config).unwrap();
        config.analysis.retain_code = true;
        let retained = parse_source(Path::new("stdatomic.c"), &source, &config).unwrap();
        assert_eq!(
            format!("{:?}", ordinary.unit()),
            format!("{:?}", retained.unit())
        );
        assert!(retained.checked().unwrap().expressions().any(|(_, e)| {
            matches!(
                e.kind(),
                toucan::semantic::checked::ExprKind::BuiltinCall {
                    builtin: toucan::semantic::checked::Builtin::Atomic(_),
                    ..
                }
            )
        }));
    }
}

#[test]
fn c11_lock_free_macro_preserves_evaluator_boolean_metadata() {
    for target in Target::ALL {
        let config = Config::with_profile(CompilerProfile::new(target, Compiler::Clang).unwrap());
        let parsed = parse_source(
            Path::new("query.h"),
            "#define LOCK_FREE __c11_atomic_is_lock_free(0)\n",
            &config,
        )
        .unwrap();
        let (bindings, report) = parsed
            .bindings(&toucan::BindingOptions {
                allowlist: vec!["LOCK_FREE".into()],
                ..Default::default()
            })
            .unwrap();
        assert!(
            bindings.contains("pub const LOCK_FREE: ::core::primitive::u8 = 1;"),
            "{bindings}"
        );
        let value =
            toucan::semantic::evaluate_integer(parsed.unit(), "__c11_atomic_is_lock_free(0)")
                .unwrap();
        assert_eq!(
            (value.rank, value.bits, value.signed, value.value),
            (0, 8, false, 1)
        );
        // Integer macro emission currently preserves width/sign as u8; this
        // assertion does not claim a Rust bool projection.
        assert_eq!(report.integer_macros, 1);
    }
}
