use std::process::Command;

use toucan_semantic::{
    Analysis, AnalysisOptions, Error, Type, TypeKind, analyze_with_profile,
    checked::{EntityKind, ExprKind, ReferenceKind},
};
use toucan_target::{CompilerProfile, LanguageMode, Target};

// Prefix, member spelling, Windows size, other-target size. These layouts and
// promoted assignments are checked independently by the native Clang test.
const CASES: &[(&str, &str, u64, u64)] = &[
    ("typedef struct {int value;} Alias;", "Alias;", 8, 4),
    ("typedef struct Named {int value;} Alias;", "Alias;", 8, 4),
    (
        "typedef struct {int value;} Alias; typedef Alias Next;",
        "Next;",
        8,
        4,
    ),
    ("struct Named {int value;};", "struct Named;", 8, 4),
    ("", "struct Named {int value;};", 8, 4),
    (
        "typedef union {int value; unsigned other;} Alias;",
        "Alias;",
        8,
        4,
    ),
    ("typedef int Alias;", "Alias;", 4, 4),
    ("typedef struct {int value;} *Alias;", "Alias;", 4, 4),
    ("typedef struct {int value;} Alias[2];", "Alias;", 4, 4),
    (
        "typedef _Atomic(struct {int value;}) Alias;",
        "Alias;",
        4,
        4,
    ),
    (
        "typedef struct {int value;} Alias;",
        "_Atomic(Alias);",
        4,
        4,
    ),
    ("typedef struct {int value;} Alias;", "_Atomic Alias;", 8, 4),
    ("typedef struct {int value;} Alias;", "const Alias;", 8, 4),
    ("typedef const struct {int value;} Alias;", "Alias;", 8, 4),
    (
        "typedef struct {int value;} Alias __attribute__((aligned(32)));",
        "Alias;",
        8,
        4,
    ),
    (
        "typedef struct {int value;} Alias;",
        "Alias __attribute__((aligned(32)));",
        8,
        4,
    ),
    (
        "typedef struct {int value;} Alias;",
        "Alias __attribute__((packed));",
        8,
        4,
    ),
    ("struct {int value;} source;", "__typeof__(source);", 4, 4),
    ("", "struct {int value;};", 8, 8),
];

fn source(case: (&str, &str, u64, u64), target: Target) -> String {
    let (prefix, member, windows, other) = case;
    let size = if target == Target::X86_64PcWindowsMsvc {
        windows
    } else {
        other
    };
    let mut source = format!(
        "{prefix} struct Owner {{ {member} int field; }};\n\
         _Static_assert(sizeof(struct Owner)=={size},\"size\");\n\
         _Static_assert(__alignof__(struct Owner)==4,\"alignment\");\n\
         _Static_assert(__builtin_offsetof(struct Owner,field)=={},\"offset\");\n",
        size - 4,
    );
    if size == 8 {
        source.push_str("void assign(struct Owner *p) { p->value=3; }\n");
    }
    source
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
            serde_json::to_value(b.unit()).unwrap(),
        ),
        (Err(a), Err(b)) => assert_eq!((a.offset, &a.message), (b.offset, &b.message)),
        result => panic!("{profile:?}: {source}: {result:?}"),
    }
    retained
}

#[test]
fn member_admission_depends_on_target_source_form_and_original_typedef() {
    for profile in CompilerProfile::ALL {
        for mode in LanguageMode::ALL {
            let profile = profile.with_language_mode(mode);
            for &case in CASES {
                let source = source(case, profile.target());
                parity(&source, profile)
                    .unwrap_or_else(|error| panic!("{profile:?}: {source}: {error}"));
            }
        }
    }
}

const CONSTRAINTS: &[(&str, bool)] = &[
    (
        "struct Missing; struct Owner {struct Missing; int field;};",
        false,
    ),
    (
        "typedef struct Missing Alias; struct Owner {Alias; int field;};",
        false,
    ),
    (
        "typedef struct {int field;} Alias; struct Owner {Alias; int field;};",
        false,
    ),
    (
        "typedef struct {int value;} Alias; struct Owner {Alias; Alias; int field;};",
        false,
    ),
    (
        "typedef int Scalar; typedef struct {int value;} Alias; struct Owner {Scalar; Alias; int field;};",
        true,
    ),
];

#[test]
fn admitted_members_keep_completeness_and_promoted_name_constraints() {
    for profile in CompilerProfile::ALL {
        for &(source, windows) in CONSTRAINTS {
            assert_eq!(
                parity(source, profile).is_ok(),
                windows || profile.target() != Target::X86_64PcWindowsMsvc,
                "{profile:?}: {source}",
            );
        }
    }
}

const RETAINED: &str = r#"
typedef struct Inner {int value;} Alias;
struct Owner {Alias; int field;};
struct Owner initialized = {.value=7, .field=4};
int read(struct Owner *p) {return p->value;}
"#;

#[test]
fn checked_members_keep_canonical_fields_and_written_typedef_references() {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    let analysis = parity(RETAINED, profile).unwrap();
    let unit = analysis.unit();
    let owner = unit
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("Owner"))
        .unwrap();
    let fields = unit.records[owner].fields.as_ref().unwrap();
    assert_eq!(fields.len(), 2);
    assert!(fields[0].name.is_none());
    assert!(matches!(fields[0].ty.kind, TypeKind::Record(_)));
    let code = analysis.checked().unwrap();
    assert!(code.expressions().any(|(_, expression)| matches!(
        expression.kind(), ExprKind::Member {fields, ..} if fields == &[0, 0]
    )));
    assert!(code.declarations().any(|(_, site)| {
        code.entity(site.entity()).unwrap().kind()
            == EntityKind::Field {
                record: owner,
                index: 0,
            }
            && site.name_source().is_none()
    }));
    assert!(code.references().iter().any(|reference| {
        reference.kind() == ReferenceKind::Typedef
            && &RETAINED[reference.source().range()] == "Alias"
    }));
    let value_references: Vec<_> = code
        .references()
        .iter()
        .filter(|reference| {
            reference.kind() == ReferenceKind::Field
                && &RETAINED[reference.source().range()] == "value"
        })
        .collect();
    assert_eq!(value_references.len(), 2);
    assert_eq!(value_references[0].target(), value_references[1].target());
}

const STORAGE: &str = r#"
typedef struct __attribute__((aligned(16))) {int value;} Aligned;
struct Owner {Aligned; int field;};
_Static_assert(sizeof(struct Owner)==32,"tag alignment retained");
_Static_assert(__builtin_offsetof(struct Owner,field)==16,"field offset");
typedef struct {int other;} Alias;
struct Packed {char lead; Alias __attribute__((packed)); int field;};
_Static_assert(sizeof(struct Packed)==12,"field attribute ignored");
_Static_assert(__builtin_offsetof(struct Packed,other)==4,"natural offset");
struct Required {__declspec(align(32)) Alias; int field;};
_Static_assert(sizeof(struct Required)==8,"declaration alignment ignored");
typedef int Local;
int local(void) {
    typedef struct {int value;} Local;
    struct LocalOwner {Local; int field;};
    struct LocalOwner owner = {.value=3, .field=4};
    _Static_assert(sizeof(struct LocalOwner)==8,"local record alias");
    return owner.value;
}
typedef struct {int hidden;} Shadow;
int shadow(void) {
    typedef int Shadow;
    struct LocalOwner {Shadow; int field;};
    _Static_assert(sizeof(struct LocalOwner)==4,"local scalar alias");
    return 0;
}
"#;

#[test]
fn canonical_storage_preserves_tag_layout_and_local_typedef_scope() {
    let profile = CompilerProfile::default_for(Target::X86_64PcWindowsMsvc);
    let analysis = parity(STORAGE, profile).unwrap();
    let unit = analysis.unit();
    let required = unit
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("Required"))
        .unwrap();
    assert_eq!(
        unit.layout(&Type::new(TypeKind::Record(required)))
            .unwrap()
            .size_bits,
        64
    );
}

#[test]
#[ignore = "requires native Clang and Windows cross-target syntax support"]
fn native_clang_member_layout_and_constraint_oracle() {
    for target in [Target::X86_64UnknownLinuxGnu, Target::X86_64PcWindowsMsvc] {
        let profile = CompilerProfile::default_for(target);
        for mode in LanguageMode::ALL {
            for &case in CASES {
                let source = source(case, target);
                native(&source, profile.with_language_mode(mode), true);
            }
        }
        for &(source, windows) in CONSTRAINTS {
            native(
                source,
                profile,
                windows || target != Target::X86_64PcWindowsMsvc,
            );
        }
        if target == Target::X86_64PcWindowsMsvc {
            native(RETAINED, profile, true);
            native(STORAGE, profile, true);
        }
    }
}

fn native(source: &str, profile: CompilerProfile, accepted: bool) {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("member.c");
    std::fs::write(&file, source).unwrap();
    let result = Command::new("clang")
        .arg(format!("--target={}", profile.target().triple()))
        .arg(format!("-std={}", profile.language_mode()))
        .arg("-fsyntax-only")
        .arg(&file)
        .output()
        .unwrap();
    assert_eq!(
        toucan_test_support::compiler_acceptance(&result),
        Ok(accepted),
        "{profile:?}: {source}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        parity(source, profile).is_ok(),
        accepted,
        "{profile:?}: {source}"
    );
}
