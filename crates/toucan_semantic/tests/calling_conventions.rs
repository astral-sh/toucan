use toucan_semantic::{CallingConvention, FunctionType, Type, TypeKind, analyze};
use toucan_target::Target;

fn function(ty: &Type) -> &FunctionType {
    match &ty.kind {
        TypeKind::Function(function) => function,
        TypeKind::Pointer(inner) => function(inner),
        _ => panic!("expected function: {ty:?}"),
    }
}

#[test]
fn conventions_follow_the_function_selected_by_attribute_placement() {
    let source = r#"
        __attribute__((ms_abi)) int prefix(int);
        int suffix(int) __attribute__((ms_abi));
        int (__attribute__((ms_abi)) *pointer)(int);
        int (*pointer_suffix)(int) __attribute__((ms_abi));
        typedef int __attribute__((ms_abi)) Function(int);
        typedef int (__attribute__((ms_abi)) *Callback)(int);
        struct S { int (__attribute__((ms_abi)) *member)(int); };
        int (*outer(void))(void) __attribute__((ms_abi));
        int (__attribute__((ms_abi)) *inner(void))(void);
        void parameter(int (__attribute__((ms_abi)) *callback)(int));
        typedef int Integer;
        Integer __attribute__((ms_abi)) (*integer_alias)(int);
    "#;
    for target in [
        Target::X86_64UnknownLinuxGnu,
        Target::X86_64AppleDarwin,
        Target::X86_64PcWindowsMsvc,
    ] {
        let unit = analyze(source, target).unwrap();
        for name in [
            "prefix",
            "suffix",
            "pointer",
            "pointer_suffix",
            "Function",
            "Callback",
            "integer_alias",
        ] {
            let ty = &unit
                .declarations
                .iter()
                .find(|decl| decl.name == name)
                .unwrap()
                .ty;
            assert_eq!(
                function(unit.resolve(ty).unwrap()).calling_convention,
                CallingConvention::Win64,
                "{target:?}: {name}"
            );
        }
        assert_eq!(
            function(
                &unit
                    .records
                    .iter()
                    .find(|record| record.name.as_deref() == Some("S"))
                    .unwrap()
                    .fields
                    .as_ref()
                    .unwrap()[0]
                    .ty
            )
            .calling_convention,
            CallingConvention::Win64
        );
        let outer = function(
            &unit
                .declarations
                .iter()
                .find(|decl| decl.name == "outer")
                .unwrap()
                .ty,
        );
        assert_eq!(outer.calling_convention, CallingConvention::Win64);
        assert_eq!(
            function(&outer.return_type).calling_convention,
            CallingConvention::C
        );
        let inner = function(
            &unit
                .declarations
                .iter()
                .find(|decl| decl.name == "inner")
                .unwrap()
                .ty,
        );
        assert_eq!(inner.calling_convention, CallingConvention::C);
        assert_eq!(
            function(&inner.return_type).calling_convention,
            CallingConvention::Win64
        );
        let parameter = function(
            &unit
                .declarations
                .iter()
                .find(|decl| decl.name == "parameter")
                .unwrap()
                .ty,
        );
        assert_eq!(
            function(&parameter.parameters[0].ty).calling_convention,
            CallingConvention::Win64
        );
    }
}

const CASES: &[(&str, bool)] = &[
    (
        "typedef int (*P)(void); P __attribute__((ms_abi)) p __attribute__((sysv_abi));",
        false,
    ),
    (
        "int __attribute__((ms_abi)) (*p)(void) __attribute__((sysv_abi));",
        false,
    ),
    (
        "typedef int (*P)(void); P p __attribute__((ms_abi)); void g(void) { P q; q = p; }",
        false,
    ),
    (
        "void g(void) { typedef int (*P)(void); P p __attribute__((ms_abi)); P q; q = p; }",
        false,
    ),
    (
        "typedef int F(void); typedef int __attribute__((sysv_abi)) F(void);",
        true,
    ),
    ("typedef int F(int x); typedef int F(int y);", true),
    ("typedef int F(); typedef int F(void);", false),
    ("typedef int A[]; typedef int A[3];", false),
    (
        "void g(void) { typedef int F(void); typedef int __attribute__((sysv_abi)) F(void); }",
        true,
    ),
    (
        "void g(void) { typedef int F(); typedef int F(void); }",
        false,
    ),
    (
        "int __attribute__((ms_abi)) f(int); int __attribute__((ms_abi)) f(int);",
        true,
    ),
    ("int __attribute__((sysv_abi)) f(int); int f(int);", true),
    ("int f(int); int __attribute__((sysv_abi)) f(int);", true),
    (
        "int __attribute__((ms_abi)) f(int); int (__attribute__((ms_abi)) *p)(int) = f;",
        true,
    ),
    (
        "int __attribute__((ms_abi)) f(int); int (*p)(int) = f;",
        false,
    ),
    (
        "int f(int); int (__attribute__((ms_abi)) *p)(int) = f;",
        false,
    ),
    ("int __attribute__((ms_abi, sysv_abi)) f(int);", false),
    (
        "int __attribute__((ms_abi)) f(int); int __attribute__((sysv_abi)) f(int);",
        false,
    ),
    ("int f(int); int __attribute__((ms_abi)) f(int);", false),
    ("int __attribute__((ms_abi(1))) f(int);", false),
    ("int __attribute__((stdcall(1))) f(int);", false),
    ("int __attribute__((ms_abi)) value;", true),
    ("typedef int __attribute__((ms_abi)) Integer;", true),
    (
        "typedef int F(void); F __attribute__((ms_abi)) f; int (__attribute__((ms_abi)) *p)(void) = f;",
        true,
    ),
    (
        "void f(void) { int __attribute__((ms_abi)) g(int); int (__attribute__((ms_abi)) *p)(int) = g; }",
        true,
    ),
    (
        "void f(void) { int __attribute__((ms_abi)) g(int); int (*p)(int) = g; }",
        false,
    ),
    (
        "int f(void); int g(void) { return ((int (__attribute__((stdcall)) *)(void))f)(); }",
        true,
    ),
    (
        "int __attribute__((ms_abi)) f(void); int g(void) { return ((int (__attribute__((ms_abi)) *)(void))f)(); }",
        true,
    ),
    (
        "struct S { int (__attribute__((ms_abi)) *p)(int); }; int __attribute__((ms_abi)) f(int); struct S s = { f };",
        true,
    ),
    (
        "struct S { int (__attribute__((ms_abi)) *p)(int); }; int f(int); struct S s = { f };",
        false,
    ),
    (
        "typedef int __attribute__((sysv_abi)) F(void); int __attribute__((ms_abi)) f(void); F f;",
        false,
    ),
];
const PROFILE_CASES: &[(&str, bool, bool)] = &[
    (
        "typedef int (*P)(void); P *p __attribute__((ms_abi)); void g(void) { P q; q = *p; }",
        true,
        false,
    ),
    (
        "typedef int (*P)(void); P array[2] __attribute__((ms_abi)); void g(void) { P q; q = array[0]; }",
        true,
        false,
    ),
    (
        "typedef int (__attribute__((ms_abi)) *P)(void); P p __attribute__((sysv_abi)); int f(void); void g(void) { p = f; }",
        false,
        true,
    ),
    (
        "void g(void) { typedef int (__attribute__((ms_abi)) *P)(void); P p __attribute__((sysv_abi)); int f(void); p = f; }",
        false,
        true,
    ),
    (
        "typedef int (*P)(void); P __attribute__((ms_abi)) p; int f(void); P q = f; void g(void) { q = p; }",
        false,
        false,
    ),
    (
        "void g(void) { typedef int (*P)(void); P __attribute__((ms_abi)) p; P q; q = p; }",
        false,
        false,
    ),
    (
        "int __attribute__((ms_abi)) f(int); int f(int);",
        false,
        true,
    ),
    (
        "int __attribute__((ms_abi)) f(int); int f(int x) { return x; }",
        false,
        true,
    ),
    (
        "int __attribute__((ms_abi)) f(int); void g(void) { int f(int); }",
        false,
        true,
    ),
    ("int __attribute__((cdecl,ms_abi)) f(int);", true, false),
    (
        "typedef int __attribute__((ms_abi)) F(void); typedef F *P; P __attribute__((sysv_abi)) p;",
        false,
        true,
    ),
];

#[test]
fn calling_conventions_participate_in_type_constraints() {
    for (target, clang) in [
        (Target::X86_64UnknownLinuxGnu, false),
        (Target::X86_64AppleDarwin, true),
    ] {
        for (source, accepted) in CASES.iter().copied().chain(
            PROFILE_CASES
                .iter()
                .map(|&(source, gcc, apple)| (source, if clang { apple } else { gcc })),
        ) {
            let result = analyze(source, target);
            assert_eq!(result.is_ok(), accepted, "{target:?}: {source}: {result:?}");
        }
    }
}

#[test]
fn legacy_conventions_have_the_platform_abi_on_supported_targets() {
    for target in Target::ALL {
        for convention in ["cdecl", "stdcall", "fastcall", "thiscall"] {
            let source = format!(
                "int __attribute__(({convention})) f(int value) {{ return value; }} int (*p)(int) = f;"
            );
            if target == Target::I686UnknownLinuxGnu
                && matches!(convention, "stdcall" | "fastcall" | "thiscall")
            {
                assert!(analyze(&source, target).is_err(), "{target}: {convention}");
                continue;
            }
            let unit = analyze(&source, target).unwrap();
            assert_eq!(
                function(&unit.declarations[0].ty)
                    .calling_convention
                    .for_target(target)
                    .unwrap(),
                CallingConvention::C
            );
        }
    }
    for target in [Target::Aarch64UnknownLinuxGnu, Target::Aarch64AppleDarwin] {
        for convention in ["ms_abi", "sysv_abi"] {
            let error = analyze(
                &format!("int __attribute__(({convention})) f(int);"),
                target,
            )
            .unwrap_err();
            assert!(error.message.contains("unsupported on this target"));
        }
    }
}

#[test]
#[ignore = "requires GCC and Clang; run with --include-ignored"]
fn convention_constraints_match_native_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if std::env::consts::ARCH != "x86_64" || std::env::consts::OS == "windows" {
        return;
    }
    for (compiler, target, clang) in [
        (
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            Target::X86_64UnknownLinuxGnu,
            false,
        ),
        ("clang".into(), Target::X86_64AppleDarwin, true),
    ] {
        for (source, accepted) in CASES.iter().copied().chain(
            PROFILE_CASES
                .iter()
                .map(|&(source, gcc, apple)| (source, if clang { apple } else { gcc })),
        ) {
            let mut child = Command::new(&compiler)
                .args([
                    "-std=gnu11",
                    "-Werror=incompatible-pointer-types",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ])
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
                "{compiler}: {source}\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                analyze(source, target).is_ok(),
                accepted,
                "{compiler}: {source}"
            );
        }
    }
}

#[test]
#[ignore = "requires Clang with all supported targets; run with --include-ignored"]
fn conventions_match_clang_target_ir() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for target in Target::ALL {
        for convention in [
            "cdecl", "stdcall", "fastcall", "thiscall", "ms_abi", "sysv_abi",
        ] {
            let source = format!(
                "int __attribute__(({convention})) function(int value) {{ return value; }}\n"
            );
            let mut child = Command::new("clang")
                .args([
                    "-target",
                    target.triple(),
                    "-x",
                    "c",
                    "-S",
                    "-emit-llvm",
                    "-o",
                    "-",
                    "-",
                ])
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
            assert!(
                output.status.success(),
                "{target:?}: {convention}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let ir = String::from_utf8(output.stdout).unwrap();
            let definition = ir.lines().find(|line| line.starts_with("define ")).unwrap();
            // Windows ARM64 uses the default C ABI for ms_abi, like Windows x64;
            // i686 GNU Linux does not provide a distinct win64 calling ABI.
            let expected = if convention == "ms_abi"
                && !target.is_windows()
                && target != Target::I686UnknownLinuxGnu
                && !target.is_armv7()
            {
                "win64cc"
            } else if convention == "sysv_abi" && target == Target::X86_64PcWindowsMsvc {
                "x86_64_sysvcc"
            } else {
                ""
            };
            assert_eq!(
                definition.contains("win64cc"),
                expected == "win64cc",
                "{target:?}: {definition}"
            );
            assert_eq!(
                definition.contains("x86_64_sysvcc"),
                expected == "x86_64_sysvcc",
                "{target:?}: {definition}"
            );
            if target == Target::I686UnknownLinuxGnu {
                let x86_32_cc = match convention {
                    "stdcall" => "x86_stdcallcc",
                    "fastcall" => "x86_fastcallcc",
                    "thiscall" => "x86_thiscallcc",
                    _ => "",
                };
                if !x86_32_cc.is_empty() {
                    assert!(definition.contains(x86_32_cc), "{target:?}: {definition}");
                }
            }
            let result = analyze(&source, target);
            if matches!(
                target,
                Target::Aarch64UnknownLinuxGnu
                    | Target::Aarch64UnknownLinuxMusl
                    | Target::Aarch64AppleDarwin
            ) && matches!(convention, "ms_abi" | "sysv_abi")
                || target == Target::I686UnknownLinuxGnu
                    && matches!(convention, "stdcall" | "fastcall" | "thiscall" | "ms_abi")
            {
                assert!(result.unwrap_err().message.contains("unsupported on"));
            } else {
                let unit = result.unwrap();
                let convention = function(&unit.declarations[0].ty)
                    .calling_convention
                    .for_target(target)
                    .unwrap();
                assert_eq!(
                    convention,
                    match expected {
                        "win64cc" => CallingConvention::Win64,
                        "x86_64_sysvcc" => CallingConvention::SysV64,
                        _ => CallingConvention::C,
                    }
                );
            }
        }
    }
}

#[test]
#[ignore = "requires native GCC and Clang; run with --include-ignored"]
fn legacy_conventions_match_native_c_compilers() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for convention in ["cdecl", "stdcall", "fastcall", "thiscall"] {
            let source = format!(
                "int __attribute__(({convention})) f(int value) {{ return value; }} int (*p)(int) = f;\n"
            );
            let mut child = Command::new(&compiler)
                .args([
                    "-std=gnu11",
                    "-Werror=incompatible-pointer-types",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
                ])
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
            assert!(
                output.status.success(),
                "{compiler}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
