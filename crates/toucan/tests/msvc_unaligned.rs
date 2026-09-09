use toucan::{Compiler, CompilerProfile, Config, Target};
use toucan_semantic::{Type, TypeKind};

const HEADER: &str = r#"
typedef unsigned short WCHAR;
typedef WCHAR __unaligned *Loose;
typedef WCHAR *Tight;
typedef _unaligned WCHAR Alias;
struct Fields { char tag; __unaligned WCHAR value; Loose pointer; };
__forceinline int __cdecl ms_inline(int value) { return value + 1; }
_Static_assert(sizeof(Loose) == 8 && _Alignof(Loose) == 8, "pointer ABI");
_Static_assert(_Alignof(Alias) == 1, "unaligned data");
_Static_assert(!__builtin_types_compatible_p(Loose, Tight), "pointer identity");
_Static_assert(sizeof(struct Fields) == 16 && _Alignof(struct Fields) == 8, "record ABI");
_Static_assert(__builtin_offsetof(struct Fields, value) == 2, "field layout");
_Static_assert(__builtin_offsetof(struct Fields, pointer) == 8, "pointer field");
"#;

#[test]
fn microsoft_unaligned_preserves_pointee_identity_and_record_layout() {
    let profile = CompilerProfile::new(Target::Aarch64PcWindowsMsvc, Compiler::Clang).unwrap();
    let analysis =
        toucan::semantic::analyze_with_profile(HEADER, profile, &Default::default()).unwrap();
    let unit = analysis.unit();
    let loose = &unit.typedefs["Loose"];
    let TypeKind::Pointer(pointee) = &unit.resolve(loose).unwrap().kind else {
        panic!("Loose must be a pointer");
    };
    assert!(unit.qualifiers(pointee).unwrap().is_unaligned());
    assert_eq!(unit.layout(loose).unwrap().size_bytes(), 8);
    assert_eq!(unit.alignment(loose).unwrap(), 8);
    assert_eq!(
        unit.alignment(&Type::new(TypeKind::Typedef("Alias".into())))
            .unwrap(),
        1
    );
    let id = unit
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("Fields"))
        .unwrap();
    let layout = unit.layout(&Type::new(TypeKind::Record(id))).unwrap();
    assert_eq!((layout.size_bytes(), layout.alignment_bytes()), (16, 8));
    assert_eq!(layout.fields[1].as_ref().unwrap().offset_bits / 8, 2);
    assert_eq!(layout.fields[2].as_ref().unwrap().offset_bits / 8, 8);

    let mut config = Config::with_profile(profile);
    config.preprocessor.allow_filesystem = false;
    toucan::parse_source(std::path::Path::new("unaligned.h"), HEADER, &config).unwrap();
    assert!(
        toucan::semantic::analyze_with_profile(
            HEADER,
            CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Gnu).unwrap(),
            &Default::default()
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires Clang with the Windows ARM64 target"]
fn microsoft_unaligned_clang_arm64_oracle() {
    use std::process::Command;

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("unaligned.c");
    std::fs::write(&input, HEADER).unwrap();
    for (extensions, accepted) in [("-fms-extensions", true), ("-fno-ms-extensions", false)] {
        let output = Command::new("clang")
            .args([
                "--target=aarch64-pc-windows-msvc",
                extensions,
                "-std=c11",
                "-fsyntax-only",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            accepted,
            "{extensions}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
