use std::path::Path;

use toucan::{BindingOptions, Config, Target};
use toucan_semantic::{Type, TypeKind};

#[test]
fn microsoft_arm64_header_preserves_target_layout_and_callback_abi() {
    let target = Target::Aarch64PcWindowsMsvc;
    let mut config = Config::new(target);
    config.preprocessor.allow_filesystem = false;
    let header = r#"
        #if !defined(_M_ARM64) || !defined(_WIN64) || defined(_M_X64)
        #error wrong architecture
        #endif
        struct Big { char first; __int128 second; unsigned __int128 third; };
        #pragma pack(push, 8)
        struct Packed { char first; __int128 second; };
        #pragma pack(pop)
        typedef long arm_long;
        typedef int (__attribute__((ms_abi)) *ArmCallback)(arm_long);
        __declspec(dllimport) int call_arm(ArmCallback callback, struct Big *data);
        __attribute__((sysv_abi)) int ignored_sysv(void);
    "#;
    let parsed = toucan::parse_source(Path::new("arm64_api.h"), header, &config).unwrap();
    for (name, bytes, alignment) in [("Big", 48, 16), ("Packed", 24, 8)] {
        let id = parsed
            .unit()
            .records
            .iter()
            .position(|record| record.name.as_deref() == Some(name))
            .unwrap();
        let layout = parsed
            .unit()
            .layout(&Type::new(TypeKind::Record(id)))
            .unwrap();
        assert_eq!(
            (layout.size_bytes(), layout.alignment_bytes()),
            (bytes, alignment),
            "{name}"
        );
    }
    let source = parsed.bindings(&BindingOptions::default()).unwrap().0;
    assert!(
        source
            .contains("target_arch = \"aarch64\", target_os = \"windows\", target_env = \"msvc\"")
    );
    assert!(source.contains("pub fn call_arm("));
    assert!(source.contains("pub fn ignored_sysv("));
    assert!(source.contains("unsafe extern \"C\" fn("), "{source}");
    assert!(!source.contains("extern \"win64\""), "{source}");
    assert!(!source.contains("extern \"sysv64\""), "{source}");
}

#[test]
#[ignore = "requires Clang 18 with Windows x64 and ARM64 backends"]
fn padded_transparent_union_has_different_arm64_and_x64_call_abis() {
    use std::process::Command;

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("transparent_union.c");
    std::fs::write(
        &input,
        "typedef union __attribute__((aligned(16), transparent_union)) { int first; unsigned bits; } Padded;\nint call_padded(Padded value) { return value.first; }\n",
    )
    .unwrap();
    for (target, signature) in [
        (
            "aarch64-pc-windows-msvc",
            "define dso_local i32 @call_padded(i32 %0, [12 x i8] %1)",
        ),
        (
            "x86_64-pc-windows-msvc",
            "define dso_local i32 @call_padded(ptr noundef %0)",
        ),
    ] {
        let output = Command::new("clang")
            .args([
                "-target",
                target,
                "-std=c11",
                "-S",
                "-emit-llvm",
                "-O0",
                "-o",
                "-",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = String::from_utf8(output.stdout).unwrap();
        assert!(ir.contains(signature), "{target}: {ir}");
    }
}
