use std::process::Command;

use toucan_semantic::{Analysis, AnalysisOptions, Error, analyze_with_profile, checked::ExprKind};
use toucan_target::{Compiler, CompilerProfile, LanguageMode, Target};

const QUALIFIERS: &[&str] = &[
    "",
    "const",
    "volatile",
    "const volatile",
    "_Atomic",
    "const _Atomic",
    "volatile _Atomic",
    "const volatile _Atomic",
];

fn source(kind: &str, qualifier: &str, profile: CompilerProfile) -> String {
    let gnu_atomic = profile.compiler() == Compiler::Gnu && qualifier.contains("_Atomic");
    let (size, alignment, offset) = match (kind, gnu_atomic) {
        ("union", _) => (8, 4, 4),
        (_, true) => (16, 8, 8),
        _ => (12, 4, 8),
    };
    format!(
        "struct Owner {{{qualifier} {kind} {{int value; int other;}}; int field;}};\n\
         _Static_assert(sizeof(struct Owner)=={size},\"size\");\n\
         _Static_assert(__alignof__(struct Owner)=={alignment},\"alignment\");\n\
         _Static_assert(__builtin_offsetof(struct Owner,field)=={offset},\"offset\");\n"
    )
}

fn parity(source: &str, profile: CompilerProfile) -> Result<Analysis, Error> {
    let ordinary = analyze_with_profile(source, profile, &AnalysisOptions::default());
    let retained = analyze_with_profile(
        source,
        profile,
        &AnalysisOptions {
            retain_code: true,
            ..Default::default()
        },
    );
    match (&ordinary, &retained) {
        (Ok(a), Ok(b)) => assert_eq!(
            serde_json::to_value(a.unit()).unwrap(),
            serde_json::to_value(b.unit()).unwrap()
        ),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        outcomes => panic!("{profile:?}: {source}: {outcomes:?}"),
    }
    retained
}

#[test]
fn direct_qualifiers_preserve_compiler_storage_rules_or_return_a_diagnostic() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for kind in ["struct", "union"] {
                for qualifier in QUALIFIERS {
                    let source = source(kind, qualifier, profile);
                    let result = parity(&source, profile);
                    if profile.compiler() == Compiler::Gnu && qualifier.contains("_Atomic") {
                        let error = result.unwrap_err();
                        assert_eq!(
                            error.message,
                            "GNU atomic anonymous record members are unsupported"
                        );
                        assert_eq!(error.offset, source.find(qualifier).unwrap());
                    } else {
                        result.unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
                    }
                }
            }
        }
    }
}

#[test]
fn promoted_members_keep_gnu_const_volatile_and_drop_clang_qualifiers() {
    for profile in CompilerProfile::ALL {
        for kind in ["struct", "union"] {
            for qualifier in QUALIFIERS {
                if profile.compiler() == Compiler::Gnu && qualifier.contains("_Atomic") {
                    continue;
                }
                let mut source = source(kind, qualifier, profile);
                source.push_str("int read(struct Owner *p) {return p->value;}\n");
                let analysis = parity(&source, profile).unwrap();
                let code = analysis.checked().unwrap();
                let member = code
                    .expressions()
                    .find_map(|(_, expression)| {
                        matches!(expression.kind(), ExprKind::Member { .. }).then_some(expression)
                    })
                    .unwrap();
                let ty = code.ty(member.ty()).unwrap();
                assert_eq!(
                    ty.qualifiers.is_const,
                    profile.compiler() == Compiler::Gnu && qualifier.contains("const")
                );
                assert_eq!(
                    ty.qualifiers.is_volatile,
                    profile.compiler() == Compiler::Gnu && qualifier.contains("volatile")
                );
                let ExprKind::Member { fields, .. } = member.kind() else {
                    unreachable!()
                };
                assert_eq!(fields, &[0, 0]);
                source.push_str("void assign(struct Owner *p) {p->value=7;}\n");
                assert_eq!(
                    parity(&source, profile).is_ok(),
                    profile.compiler() != Compiler::Gnu || !qualifier.contains("const"),
                    "{profile:?}: {source}"
                );
            }
        }
    }
}

#[test]
fn dropping_outer_qualifiers_preserves_member_qualification_and_restrict_errors() {
    for profile in CompilerProfile::ALL {
        for source in [
            "struct Owner {const struct {const int value;};}; void f(struct Owner *p){p->value=1;}",
            "struct Owner {restrict struct {int value;}; int field;};",
            "struct Owner {restrict union {int value;}; int field;};",
        ] {
            assert!(parity(source, profile).is_err(), "{profile:?}: {source}");
        }
    }
}

#[test]
#[ignore = "requires native GCC/Clang and Clang Windows cross-target support"]
fn native_direct_qualifier_layouts_and_assignments() {
    for (compiler, profile) in [
        (
            "gcc",
            CompilerProfile::default_for(Target::X86_64UnknownLinuxGnu),
        ),
        (
            "clang",
            CompilerProfile::new(Target::X86_64UnknownLinuxGnu, Compiler::Clang).unwrap(),
        ),
        (
            "clang",
            CompilerProfile::default_for(Target::X86_64PcWindowsMsvc),
        ),
    ] {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for kind in ["struct", "union"] {
                for qualifier in QUALIFIERS {
                    let mut source = source(kind, qualifier, profile);
                    native(compiler, profile, &source, true);
                    if profile.compiler() == Compiler::Gnu && qualifier.contains("_Atomic") {
                        // GNU's atomic storage is accepted by GCC but outside
                        // Toucan's anonymous-member support. Do not erase it.
                        assert_eq!(
                            parity(&source, profile).unwrap_err().message,
                            "GNU atomic anonymous record members are unsupported"
                        );
                        continue;
                    }
                    source.push_str("void assign(struct Owner *p) {p->value=7;}\n");
                    let accepted =
                        profile.compiler() != Compiler::Gnu || !qualifier.contains("const");
                    native(compiler, profile, &source, accepted);
                    assert_eq!(parity(&source, profile).is_ok(), accepted);
                }
            }
        }
    }
}

fn native(compiler: &str, profile: CompilerProfile, source: &str, accepted: bool) {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("qualifiers.c");
    std::fs::write(&input, source).unwrap();
    let mut command = Command::new(compiler);
    if profile.compiler() == Compiler::Clang {
        command.arg(format!("--target={}", profile.target().triple()));
    }
    let output = command
        .arg(format!("-std={}", profile.language_mode()))
        .arg("-fsyntax-only")
        .arg(&input)
        .output()
        .unwrap();
    assert_eq!(
        toucan_test_support::compiler_acceptance(&output),
        Ok(accepted),
        "{profile:?}: {source}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
