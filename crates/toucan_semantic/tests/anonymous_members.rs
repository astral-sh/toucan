use std::process::Command;
use toucan_semantic::{Type, TypeKind, analyze_with_profile};
use toucan_target::{Compiler, CompilerProfile};
use toucan_test_support::compiler_acceptance;

const SOURCE: &str = "struct { int member; } source; struct Owner { __typeof__(source); int field; }; struct Direct { struct { int member; }; int field; }; _Static_assert(sizeof(struct Owner)==sizeof(int),\"ignored declarations\"); _Static_assert(__builtin_offsetof(struct Owner,field)==0,\"field offset\"); _Static_assert(sizeof(struct Direct)==2*sizeof(int),\"anonymous member\");";

#[test]
fn only_direct_record_specifiers_introduce_anonymous_members() {
    for profile in CompilerProfile::ALL {
        let analysis = analyze_with_profile(SOURCE, profile, &Default::default()).unwrap();
        let unit = analysis.unit();
        let owner = unit
            .records
            .iter()
            .position(|item| item.name.as_deref() == Some("Owner"))
            .unwrap();
        let direct = unit
            .records
            .iter()
            .position(|item| item.name.as_deref() == Some("Direct"))
            .unwrap();
        assert_eq!(unit.records[owner].fields.as_ref().unwrap().len(), 1);
        assert_eq!(unit.records[direct].fields.as_ref().unwrap().len(), 2);
        assert_eq!(
            unit.layout(&Type::new(TypeKind::Record(owner)))
                .unwrap()
                .size_bits,
            32
        );
        assert_eq!(
            unit.layout(&Type::new(TypeKind::Record(direct)))
                .unwrap()
                .size_bits,
            64
        );
    }
}

#[test]
#[ignore = "requires native GCC and Clang cross-target parsing"]
fn ignored_typeof_declarations_match_c_layouts() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("probe.c");
    std::fs::write(&source, SOURCE).unwrap();
    let gcc = std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into());
    let output = Command::new(gcc)
        .args(["-std=gnu11", "-fsyntax-only"])
        .arg(&source)
        .output()
        .unwrap();
    assert_eq!(
        compiler_acceptance(&output),
        Ok(true),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for profile in CompilerProfile::ALL
        .into_iter()
        .filter(|profile| profile.compiler() == Compiler::Clang)
    {
        let output = Command::new("clang")
            .args([
                "-std=gnu11",
                "-fsyntax-only",
                "-target",
                profile.target().triple(),
            ])
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            compiler_acceptance(&output),
            Ok(true),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
