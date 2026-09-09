use std::process::Command;
use toucan_semantic::{AnalysisOptions, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile, Target};

const HEADER: &str = r#"
#pragma pack(push,2)
struct Bits { unsigned char a:1; unsigned int b:1 __attribute__((aligned(4))); char tail; };
#pragma pack(pop)
void write_bits(struct Bits *value,unsigned a,unsigned b,char tail);
unsigned read_bits(const struct Bits *value);
"#;

#[test]
fn armv7_hard_float_bindings_keep_the_exact_rust_target_guard() {
    let target = Target::Armv7UnknownLinuxGnueabihf;
    let unit = analyze_with_profile(
        "typedef struct { float x; float y; } Pair; Pair fold(Pair input, float scale);",
        CompilerProfile::default_for(target),
        &AnalysisOptions::default(),
    )
    .unwrap()
    .into_unit();
    let source = toucan_bindings::generate(&unit, &toucan_bindings::Options::default())
        .unwrap()
        .source;
    assert!(source.contains("target_arch = \"arm\", target_os = \"linux\", target_env = \"gnu\", target_abi = \"eabihf\""));
    assert!(source.contains("pub fn fold("));
    assert!(source.contains("unsafe extern \"C\""));
    assert!(!source.contains("extern \"win64\""));
    let error = toucan_bindings::generate(
        &unit,
        &toucan_bindings::Options {
            rust_target: "1.64".parse().unwrap(),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("require Rust 1.78 or newer"));
}

#[test]
#[ignore = "requires rustc target configuration queries; run with --include-ignored"]
fn target_guards_reject_incompatible_pointer_width_and_byte_order() {
    let directory = tempfile::tempdir().unwrap();
    let mismatches = [
        (Target::X86_64UnknownLinuxGnu, "x86_64-unknown-linux-gnux32"),
        (
            Target::Aarch64UnknownLinuxGnu,
            "aarch64-unknown-linux-gnu_ilp32",
        ),
        (
            Target::Aarch64UnknownLinuxGnu,
            "aarch64_be-unknown-linux-gnu",
        ),
        (
            Target::Aarch64UnknownLinuxMusl,
            "aarch64_be-unknown-linux-musl",
        ),
        (
            Target::Armv7UnknownLinuxGnueabihf,
            "armv7-unknown-linux-gnueabi",
        ),
    ];
    let cases = Target::ALL
        .into_iter()
        .map(|target| (target, target.triple(), true))
        .chain(
            mismatches
                .into_iter()
                .map(|(target, actual)| (target, actual, false)),
        );
    for (target, actual, accepted) in cases {
        let unit = toucan_semantic::analyze("long roundtrip(long value);", target).unwrap();
        let source = toucan_bindings::generate(&unit, &Default::default())
            .unwrap()
            .source;
        let guard = source
            .lines()
            .skip_while(|line| !line.starts_with("#[cfg("))
            .take(2)
            .collect::<Vec<_>>()
            .join("\n");
        let config = Command::new("rustc")
            .args(["--print", "cfg", "--target", actual])
            .output()
            .unwrap();
        assert!(config.status.success(), "{actual}: {config:?}");
        // Evaluate the emitted predicate with rustc's real target configuration.
        // Renaming the cfg keys lets the host compile this guard without requiring
        // standard libraries for every cross target or overriding built-in cfgs.
        std::fs::write(
            directory.path().join("guard.rs"),
            guard.replace("target_", "probe_target_"),
        )
        .unwrap();
        let mut command = Command::new("rustc");
        command.current_dir(directory.path()).args([
            "--edition=2024",
            "--crate-type=lib",
            "guard.rs",
        ]);
        for line in std::str::from_utf8(&config.stdout).unwrap().lines() {
            if line.starts_with("target_") {
                command.arg("--cfg").arg(format!("probe_{line}"));
            }
        }
        let output = command.output().unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(accepted),
            "bindings for {target}, compiled for {actual}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if !accepted {
            assert!(String::from_utf8_lossy(&output.stderr).contains("different target"));
        }
    }
}

#[test]
#[ignore = "requires native Linux GCC/Clang and rustc; supports TOUCAN_TEST_RUST_TOOLCHAIN"]
fn generated_bitfield_accessors_match_both_linux_compilers() {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Target::X86_64UnknownLinuxGnu,
        ("linux", "aarch64") => Target::Aarch64UnknownLinuxGnu,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("api.c"),format!("{HEADER}\nvoid write_bits(struct Bits *v,unsigned a,unsigned b,char tail){{v->a=a;v->b=b;v->tail=tail;}}\nunsigned read_bits(const struct Bits *v){{return v->a+2*v->b+4*(unsigned char)v->tail;}}\n")).unwrap();
    std::fs::write(
        directory.path().join("main.rs"),
        r#"
#![allow(non_snake_case, non_camel_case_types, dead_code)]
include!("bindings.rs");
fn main(){unsafe {
  for a in 0..2 {for b in 0..2 {for tail in [0,1,17,127] {
    let mut v:Bits=core::mem::zeroed(); write_bits(&mut v,a,b,tail);
    assert_eq!(v.a(),a as u8); assert_eq!(v.b(),b); assert_eq!(v.tail,tail);
    v.set_a(1-a as u8); v.set_b(1-b); v.tail=127-tail;
    assert_eq!(read_bits(&v),1-a+2*(1-b)+4*(127-tail) as u32);
  }}}
}}
"#,
    )
    .unwrap();
    for compiler in [Compiler::Gnu, Compiler::Clang] {
        let analysis = analyze_with_profile(
            HEADER,
            CompilerProfile::new(target, compiler).unwrap(),
            &AnalysisOptions::default(),
        )
        .unwrap();
        let bindings = toucan_bindings::generate(
            analysis.unit(),
            &toucan_bindings::Options {
                rust_target: "1.64".parse().unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(directory.path().join("bindings.rs"), bindings.source).unwrap();
        let cc = if compiler == Compiler::Gnu {
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into())
        } else {
            "clang".into()
        };
        let identity = Command::new(&cc).arg("--version").output().unwrap();
        assert!(identity.status.success());
        if compiler == Compiler::Gnu {
            assert!(!String::from_utf8_lossy(&identity.stdout).contains("clang"));
        }
        let output = Command::new(cc)
            .current_dir(directory.path())
            .args(["-std=gnu11", "-O2", "-Werror", "-c", "api.c", "-o", "api.o"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut rustc = Command::new("rustc");
        if let Ok(toolchain) = std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
            rustc.arg(format!("+{toolchain}"));
        }
        let output = rustc
            .current_dir(directory.path())
            .args([
                "--edition=2021",
                "main.rs",
                "-C",
                "link-arg=api.o",
                "-o",
                "probe",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = Command::new(directory.path().join("probe"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
