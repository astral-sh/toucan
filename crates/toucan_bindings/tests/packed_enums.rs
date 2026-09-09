use toucan_bindings::{Options, RustTarget, generate};
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};
const API: &str = include_str!("fixtures/packed_enums/api.h");
fn bindings(profile: CompilerProfile, rustified_enums: bool) -> String {
    let analysis = analyze_with_profile(API, profile, &AnalysisOptions::default()).unwrap();
    generate(
        analysis.unit(),
        &Options {
            rustified_enums,
            rust_target: if profile.target().is_armv7() {
                RustTarget::stable(78).unwrap()
            } else {
                RustTarget::RUST_1_64
            },
            ..Default::default()
        },
    )
    .unwrap()
    .source
}
#[test]
fn packed_enums_use_compatible_primitives_and_rust_enum_representations() {
    for profile in CompilerProfile::ALL {
        let microsoft = profile.target().is_windows();
        for rustified in [false, true] {
            let source = bindings(profile, rustified);
            for (name, kind) in [
                ("Byte", if microsoft { "i32" } else { "u8" }),
                ("SignedByte", if microsoft { "i32" } else { "i8" }),
                ("Word", if microsoft { "i32" } else { "u16" }),
                ("SignedWord", if microsoft { "i32" } else { "i16" }),
            ] {
                if rustified {
                    assert!(source.contains(&format!("#[repr({kind})]")));
                    assert!(source.contains(&format!("pub enum {name}")));
                } else {
                    assert!(
                        source.contains(&format!("pub type {name} = ::core::primitive::{kind};"))
                    );
                }
            }
            assert!(source.contains("pub struct Packet"));
            assert!(source.contains("pub union Value"));
        }
    }
}
#[test]
fn packed_atomic_enum_storage_and_call_guards_use_the_new_width() {
    let source =
        "enum __attribute__((packed)) E{LOW=-128,HIGH=127};typedef _Atomic(enum E) A;A f(A);";
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(source, profile, &Default::default()).unwrap();
        for rustified_enums in [false, true] {
            let output = generate(
                analysis.unit(),
                &Options {
                    rustified_enums,
                    ..Default::default()
                },
            );
            if profile.compiler() == Compiler::Clang && !profile.target().is_windows() {
                assert!(
                    output
                        .unwrap_err()
                        .0
                        .contains("narrow atomic scalar calls under Clang")
                );
            } else {
                let source = output.unwrap().source;
                assert!(source.contains(if profile.target().is_windows() {
                    "AtomicI32"
                } else {
                    "AtomicI8"
                }));
            }
            let output = generate(
                analysis.unit(),
                &Options {
                    rustified_enums,
                    blocklist_functions: vec!["f".into()],
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(
                output
                    .source
                    .contains("pub type A = ::core::sync::atomic::AtomicI")
            );
        }
    }
}
fn rustc() -> String {
    std::env::var("TOUCAN_TEST_RUSTC").unwrap_or_else(|_| "rustc".into())
}
#[test]
#[ignore = "requires native GNU GCC, Clang and Rust"]
fn generated_packed_enum_calls_and_callbacks_match_native_c() {
    use std::process::Command;
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        ("api.h", API),
        ("native.c", include_str!("fixtures/packed_enums/native.c")),
        (
            "consumer.rs",
            include_str!("fixtures/packed_enums/consumer.rs"),
        ),
    ] {
        std::fs::write(dir.path().join(name), source).unwrap();
    }
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let identity = Command::new(&gcc).arg("--version").output().unwrap();
    assert!(
        identity.status.success() && !String::from_utf8_lossy(&identity.stdout).contains("clang")
    );
    for compiler in [gcc, "clang".into()] {
        let profile = if compiler == "clang" {
            CompilerProfile::new(target, Compiler::Clang).unwrap()
        } else {
            CompilerProfile::default_for(target)
        };
        for c_opt in ["0", "2"] {
            let out = Command::new(&compiler)
                .current_dir(dir.path())
                .args([
                    "-std=gnu11",
                    &format!("-O{c_opt}"),
                    "-c",
                    "native.c",
                    "-o",
                    "native.o",
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            let out = Command::new("ar")
                .current_dir(dir.path())
                .args(["crs", "libnative.a", "native.o"])
                .output()
                .unwrap();
            assert!(out.status.success());
            for rustified in [false, true] {
                std::fs::write(dir.path().join("bindings.rs"), bindings(profile, rustified))
                    .unwrap();
                for rust_opt in ["0", "3"] {
                    let mut command = Command::new(rustc());
                    command.current_dir(dir.path()).args([
                        "--edition=2021",
                        "-C",
                        &format!("opt-level={rust_opt}"),
                        "-L",
                        "native=.",
                        "-l",
                        "static=native",
                        "consumer.rs",
                        "-o",
                        "consumer",
                    ]);
                    if rustified {
                        command.args(["--cfg", "rustified_enums"]);
                    }
                    let out = command.output().unwrap();
                    assert!(
                        out.status.success(),
                        "{}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                    let out = Command::new(dir.path().join("consumer")).output().unwrap();
                    assert!(
                        out.status.success(),
                        "{compiler} -O{c_opt}, Rust opt{rust_opt}, rustified={rustified}: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
            }
        }
    }
}
#[test]
#[ignore = "requires Rust; TOUCAN_TEST_ALL_RUST_TARGETS checks installed cross libraries"]
fn generated_packed_enums_compile_for_rust_targets() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let identity = Command::new(rustc()).arg("-vV").output().unwrap();
    let identity = String::from_utf8(identity.stdout).unwrap();
    let host = identity
        .lines()
        .find_map(|s| s.strip_prefix("host: "))
        .unwrap();
    let all = std::env::var("TOUCAN_TEST_ALL_RUST_TARGETS").as_deref() == Ok("1");
    for profile in CompilerProfile::ALL {
        if !all && profile.target().triple() != host {
            continue;
        }
        for rustified in [false, true] {
            std::fs::write(dir.path().join("bindings.rs"), bindings(profile, rustified)).unwrap();
            std::fs::write(
                dir.path().join("consumer.rs"),
                include_str!("fixtures/packed_enums/consumer.rs"),
            )
            .unwrap();
            let mut command = Command::new(rustc());
            command.current_dir(dir.path()).args([
                "--edition=2021",
                "--target",
                profile.target().triple(),
                "--emit=metadata",
                "consumer.rs",
                "-o",
                "consumer.rmeta",
            ]);
            if rustified {
                command.args(["--cfg", "rustified_enums"]);
            }
            let out = command.output().unwrap();
            assert!(
                out.status.success(),
                "{profile:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
