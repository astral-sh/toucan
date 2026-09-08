use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, Config, Target};

fn check(body: &str, target: Target) -> Result<(), toucan::Error> {
    let config = Config::new(target);
    toucan::parse_source(
        Path::new("asm.h"),
        &format!("void f(int x) {{ {body} }}"),
        &config,
    )
    .map(|_| ())
}

fn cases() -> Vec<(&'static str, bool)> {
    vec![
        (r#"__asm__("nop");"#, true),
        (r#"__asm__ volatile("" ::: "cc", "memory");"#, true),
        (r#"__asm__("" : "+r"(x));"#, true),
        (r#"int y; __asm__("" : [out] "=r"(y) : "[out]"(x));"#, true),
        (r#"int y; __asm__("" : "=r,m"(y) : "r,m"(x));"#, true),
        (r#"__asm__("" :: "i"(1 + 2), "n"(-5));"#, true),
        (r#"__asm__("" :: "ri"(x + 1));"#, true),
        (r#"__asm__("" : "+m"(x));"#, true),
        (r#"int a[2]; __asm__("" : "=m"(a));"#, true),
        (r#"int a[2]; __asm__("" : "=r"(a));"#, true),
        (
            r#"struct { unsigned value:3; } bits; __asm__("" : "=r"(bits.value));"#,
            true,
        ),
        (r#"float value = 1; __asm__("" :: "r"(value));"#, true),
        (r#"__asm__("" : "=r"(x + 1));"#, false),
        (r#"const int value = 1; __asm__("" : "=r"(value));"#, false),
        (r#"const int a[2] = {1}; __asm__("" : "=m"(a));"#, false),
        (r#"__asm__("" :: "r"(missing));"#, false),
        (r#"__asm__("" :: "i"(x));"#, false),
        (r#"__asm__("" :: "m"(1));"#, false),
        (
            r#"struct { unsigned value:3; } bits; __asm__("" : "=m"(bits.value));"#,
            false,
        ),
        (r#"__asm__("" : "r"(x));"#, false),
        (r#"__asm__("" :: "=r"(x));"#, false),
        (r#"__asm__("" : "="(x));"#, false),
        (r#"__asm__("" : "+r"(x) : "0"(x));"#, false),
        (r#"__asm__("" :: "0"(x));"#, false),
        (r#"__asm__("" : "=r"(x) : "[missing]"(x));"#, false),
        (r#"__asm__("" : [same] "=r"(x) : [same] "r"(x));"#, false),
        (r#"__asm__("nop %3" : "=r"(x));"#, false),
        (r#"__asm__("nop %[missing]" : "=r"(x));"#, false),
        (r#"__asm__("nop %" : "=r"(x));"#, false),
        (r#"__asm__("" : "=r,m"(x) : "r"(x));"#, false),
        (r#"__asm__("" ::: "not_a_register");"#, false),
        (r#"__asm__(L"nop");"#, false),
        (r#"__asm__(u8"nop");"#, false),
        (r#"__asm__ const("nop");"#, false),
        (r#"__asm__ restrict("nop");"#, false),
    ]
}

#[test]
fn asm_operands_follow_c_constraints() {
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::Aarch64UnknownLinuxGnu,
    ] {
        for (body, accepted) in cases() {
            let result = check(body, target);
            assert_eq!(result.is_ok(), accepted, "{target}: {body}: {result:?}");
        }
    }
}

#[test]
fn target_constraints_and_unsupported_features_fail_closed() {
    assert!(check(r#"__asm__("" : "=a"(x));"#, Target::X86_64UnknownLinuxGnu).is_ok());
    assert!(check(r#"__asm__("" : "=a"(x));"#, Target::Aarch64UnknownLinuxGnu).is_err());
    assert!(
        check(
            r#"__asm__("" : "=a"(x) :: "rax");"#,
            Target::X86_64UnknownLinuxGnu
        )
        .is_err()
    );
    assert!(
        check(
            r#"__asm__("" : "=ra"(x) :: "rax");"#,
            Target::X86_64UnknownLinuxGnu
        )
        .is_ok()
    );
    for clobber in ["rax", "eax", "ax", "al", "ah", "r8", "xmm15"] {
        assert!(
            check(
                &format!(r#"__asm__("" ::: "{clobber}");"#),
                Target::X86_64UnknownLinuxGnu
            )
            .is_ok()
        );
    }
    for clobber in ["x0", "w0", "d31", "v31", "lr", "fp"] {
        assert!(
            check(
                &format!(r#"__asm__("" ::: "{clobber}");"#),
                Target::Aarch64UnknownLinuxGnu
            )
            .is_ok()
        );
    }
    for (body, target) in [
        (r#"__asm__("" ::: "rsp");"#, Target::X86_64UnknownLinuxGnu),
        (r#"__asm__("" ::: "r8db");"#, Target::X86_64UnknownLinuxGnu),
        (r#"__asm__("" ::: "x00");"#, Target::Aarch64UnknownLinuxGnu),
        (r#"__asm__("" : "=f"(x));"#, Target::X86_64UnknownLinuxGnu),
        (
            r#"__asm__("%Y0" : "=r"(x));"#,
            Target::X86_64UnknownLinuxGnu,
        ),
        (r#"__asm__("nop");"#, Target::X86_64PcWindowsMsvc),
        (r#"__asm__("nop\0ignored");"#, Target::X86_64UnknownLinuxGnu),
    ] {
        assert!(
            check(body, target)
                .unwrap_err()
                .to_string()
                .contains("unsupported"),
            "{body}"
        );
    }
}

#[test]
fn named_operands_and_template_escapes_are_checked() {
    for body in [
        r#"__asm__("mov %1, %0" : "+r"(x));"#,
        r#"__asm__("mov %[in],%[out]" : [out] "=r"(x) : [in] "r"(x));"#,
        r#"__asm__("add %c0,%%eax; label%=; %{ %| %}" :: "i"(2));"#,
        r#"__asm__("add{l %1,%0| %0,%1}" : "+r"(x) : "r"(x));"#,
    ] {
        assert!(check(body, Target::X86_64UnknownLinuxGnu).is_ok(), "{body}");
    }
    for body in [
        r#"__asm__("%c0" :: "r"(x));"#,
        r#"__asm__("%[open" :: "r"(x));"#,
        r#"__asm__("{nested{dialect}}" : "+r"(x));"#,
        r#"__asm__("{unfinished" : "+r"(x));"#,
    ] {
        assert!(
            check(body, Target::X86_64UnknownLinuxGnu).is_err(),
            "{body}"
        );
    }
}

#[test]
#[ignore = "requires native C compilers; run with --include-ignored"]
fn asm_acceptance_matches_native_c_compilers() {
    let Some(target) = native_target() else {
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    for (index, (body, accepted)) in cases().into_iter().enumerate() {
        let input = directory.path().join(format!("case_{index}.c"));
        std::fs::write(&input, format!("void f(int x) {{ {body} }}\n")).unwrap();
        assert_eq!(check(body, target).is_ok(), accepted, "{body}");
        for compiler in ["gcc", "clang"] {
            let result = Command::new(compiler)
                .args(["-std=c11", "-S"])
                .arg(&input)
                .arg("-o")
                .arg(directory.path().join("out.s"))
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result),
                Ok(accepted),
                "{compiler}: {body}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    for (target, accepted, names) in [
        (
            Target::X86_64UnknownLinuxGnu,
            true,
            &[
                "rax", "eax", "ax", "al", "ah", "r8", "xmm15", "flags", "fpsr", "st",
            ][..],
        ),
        (
            Target::X86_64UnknownLinuxGnu,
            false,
            &["eflags", "rflags", "bpl", "r8db"],
        ),
        (
            Target::Aarch64UnknownLinuxGnu,
            true,
            &["x0", "w0", "v31", "s0", "d0", "lr", "fp"],
        ),
        (
            Target::Aarch64UnknownLinuxGnu,
            false,
            &["q31", "b0", "h0", "x00"],
        ),
    ] {
        for name in names {
            let body = format!(r#"__asm__("" ::: "{name}");"#);
            assert_eq!(check(&body, target).is_ok(), accepted, "{target}: {name}");
            let input = directory.path().join("clobber.c");
            std::fs::write(&input, format!("void f(void) {{ {body} }}\n")).unwrap();
            let result = Command::new("clang")
                .arg(format!("--target={}", target.triple()))
                .args(["-std=c11", "-S"])
                .arg(&input)
                .arg("-o")
                .arg(directory.path().join("clobber.s"))
                .output()
                .unwrap();
            assert_eq!(
                toucan_test_support::compiler_acceptance(&result),
                Ok(accepted),
                "{target}: {name}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
}

#[test]
#[ignore = "requires native C compilers and rustc; run with --include-ignored"]
fn inline_asm_byte_swaps_work_through_generated_bindings() {
    let Some(target) = native_target() else {
        return;
    };
    let (swap32, swap64) = if target.triple().starts_with("x86_64") {
        ("bswap %0", "bswap %0")
    } else {
        ("rev %w0,%w0", "rev %0,%0")
    };
    let header = format!(
        "static inline unsigned int inline32(unsigned int value) {{ __asm__(\"{swap32}\" : \"+r\"(value)); return value; }}\nstatic inline unsigned long long inline64(unsigned long long value) {{ __asm__(\"{swap64}\" : \"+r\"(value)); return value; }}\nunsigned int swap32(unsigned int); unsigned long long swap64(unsigned long long);\n"
    );
    let compilation =
        toucan::parse_source(Path::new("swap.h"), &header, &Config::new(target)).unwrap();
    let (bindings, report) = compilation
        .bindings(&BindingOptions {
            allowlist: vec!["swap*".into(), "inline*".into()],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(report.skipped_declarations, ["inline32", "inline64"]);
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("swap.h"), header).unwrap();
    std::fs::write(directory.path().join("swap.c"), "#include \"swap.h\"\nunsigned int swap32(unsigned int value) { return inline32(value); }\nunsigned long long swap64(unsigned long long value) { return inline64(value); }\n").unwrap();
    std::fs::write(directory.path().join("probe.rs"), format!("{bindings}\nfn main() {{ unsafe {{ assert_eq!(swap32(0x01020304),0x04030201); assert_eq!(swap64(0x0102030405060708),0x0807060504030201); }} }}\n")).unwrap();
    for compiler in ["gcc", "clang"] {
        let object = directory.path().join("swap.o");
        let archive = directory.path().join("libswap.a");
        let commands = [
            Command::new(compiler)
                .args(["-std=c11", "-O2", "-c"])
                .arg(directory.path().join("swap.c"))
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap(),
            Command::new("ar")
                .arg("crs")
                .arg(&archive)
                .arg(&object)
                .output()
                .unwrap(),
            Command::new("rustc")
                .args(["--edition=2024", "-O", "-D", "improper_ctypes"])
                .arg(directory.path().join("probe.rs"))
                .arg("-L")
                .arg(directory.path())
                .args(["-l", "static=swap", "-o"])
                .arg(directory.path().join("probe"))
                .output()
                .unwrap(),
        ];
        for result in commands {
            assert!(
                result.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        assert!(
            Command::new(directory.path().join("probe"))
                .status()
                .unwrap()
                .success()
        );
    }
}

fn native_target() -> Option<Target> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Some(Target::X86_64UnknownLinuxGnu),
        ("aarch64", "linux") => Some(Target::Aarch64UnknownLinuxGnu),
        ("x86_64", "macos") => Some(Target::X86_64AppleDarwin),
        ("aarch64", "macos") => Some(Target::Aarch64AppleDarwin),
        _ => None,
    }
}
