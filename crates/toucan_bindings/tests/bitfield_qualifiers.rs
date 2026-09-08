use std::process::Command;

use toucan_bindings::{Options, generate};
use toucan_semantic::analyze;
use toucan_target::Target;

const HEADER: &str = "typedef const unsigned Frozen;\n\
    struct S { const unsigned direct:3; Frozen aliased:5; unsigned mutable:8; };\n\
    union U { const unsigned direct:3; Frozen aliased:5; unsigned mutable:8; };\n";

#[test]
fn volatile_bitfields_require_explicit_unsupported_diagnostics() {
    for kind in ["struct", "union"] {
        for declaration in [
            "volatile unsigned bits:3;",
            "V bits:3;",
            "volatile unsigned :3;",
        ] {
            let source = format!("typedef volatile unsigned V; {kind} S {{ {declaration} }};");
            let unit = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
            let error = generate(&unit, &Options::default()).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(&format!("volatile {kind} bitfield")),
                "{source}: {error}"
            );
        }
    }
}

#[test]
#[ignore = "requires native GCC, Clang and rustc; run with --include-ignored"]
fn const_bitfield_api_matches_c_assignment_constraints() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let directory = tempfile::tempdir().unwrap();
    let unit = analyze(HEADER, target).unwrap();
    std::fs::write(
        directory.path().join("bindings.rs"),
        generate(&unit, &Options::default()).unwrap().source,
    )
    .unwrap();
    let prefix = "#![allow(dead_code, non_camel_case_types)]\ninclude!(\"bindings.rs\");\n";
    std::fs::write(
        directory.path().join("valid.rs"),
        format!(
            r#"{prefix}
        fn main() {{
            let mut s: S = unsafe {{ core::mem::zeroed() }};
            s.set_mutable(17); assert_eq!(s.mutable(),17);
            assert_eq!(s.direct(),0); assert_eq!(s.aliased(),0);
            let mut u: U = unsafe {{ core::mem::zeroed() }};
            unsafe {{
                u.set_mutable(7); assert_eq!(u.mutable(),7);
                assert_eq!(u.direct(),7); assert_eq!(u.aliased(),7);
            }}
        }}
    "#
        ),
    )
    .unwrap();
    let output = Command::new("rustc")
        .current_dir(directory.path())
        .args(["--edition=2021", "-Dwarnings", "valid.rs", "-o", "valid"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        Command::new(directory.path().join("valid"))
            .status()
            .unwrap()
            .success()
    );

    for (kind, name) in [("struct", "S"), ("union", "U")] {
        for member in ["direct", "aliased"] {
            std::fs::write(directory.path().join("invalid.rs"), format!("{prefix}\nfn change(value: &mut {name}) {{ value.set_{member}(1); }}\nfn main() {{}}\n")).unwrap();
            let output = Command::new("rustc")
                .current_dir(directory.path())
                .args(["--edition=2021", "invalid.rs"])
                .output()
                .unwrap();
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("E0599"));
            std::fs::write(
                directory.path().join("invalid.c"),
                format!("{HEADER}\nvoid change({kind} {name} *p) {{ p->{member}=1; }}\n"),
            )
            .unwrap();
            std::fs::write(directory.path().join("valid.c"), format!("{HEADER}\nunsigned change({kind} {name} *p) {{ p->mutable=1; return p->direct + p->aliased + p->mutable; }}\n")).unwrap();
            for compiler in [
                std::env::var_os("TOUCAN_GCC").unwrap_or_else(|| "gcc".into()),
                "clang".into(),
            ] {
                for (file, accepted) in [("valid.c", true), ("invalid.c", false)] {
                    let output = Command::new(&compiler)
                        .current_dir(directory.path())
                        .args([
                            "-std=c11",
                            "-pedantic-errors",
                            "-Werror",
                            "-fsyntax-only",
                            file,
                        ])
                        .output()
                        .unwrap();
                    assert_eq!(
                        output.status.success(),
                        accepted,
                        "{compiler:?}: {kind} {member}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if !accepted {
                        let diagnostic = String::from_utf8_lossy(&output.stderr);
                        assert!(
                            diagnostic.contains("read-only")
                                || diagnostic.contains("const-qualified"),
                            "unexpected C rejection: {diagnostic}"
                        );
                    }
                }
            }
        }
    }
}
