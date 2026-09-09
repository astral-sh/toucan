use std::process::Command;

use toucan_bindings::{DeriveOptions, Options, RustTarget, generate};
use toucan_semantic::analyze;
use toucan_target::Target;
use toucan_test_support::compiler_acceptance;

const SOURCE: &str = "\
enum NonZero { NONZERO_TWO=2, NONZERO_FOUR=4 };
enum Zero { ZERO_ZERO=0, ZERO_ONE=1 };
struct Scalars { int integer; _Bool boolean; };
struct Float { float value; };
struct Pointer { void *value; };
struct Callback { int (*value)(void); };
struct Callback13 { int (*value)(int,int,int,int,int,int,int,int,int,int,int,int,int); };
struct CallbackVariadic { int (*value)(int,...); };
struct Large { int value[33]; };
struct EnumField { enum NonZero value; };
struct ZeroField { enum Zero value; };
struct EnumArray { enum NonZero value[0]; };
union EnumUnion { enum NonZero value; };
union Union { int integer; float value; };
struct UnionParent { union Union value; };
struct Nested { struct Scalars value; };
struct Packed { int value; } __attribute__((packed));
struct PackedNested { struct Scalars value; } __attribute__((packed));
struct Incomplete;
struct Flexible { int count; int values[]; };
struct Bitfields { unsigned first:3; unsigned second:5; };
struct PaddedBits { char prefix; unsigned first:3; unsigned :0; int suffix; };
struct Atomic { _Atomic(int) value; };
";

fn options(copy: bool) -> Options {
    Options {
        derives: DeriveOptions {
            copy,
            debug: Some(true),
            default: true,
            eq: true,
            ..DeriveOptions::default()
        },
        rustified_enum_patterns: vec!["NonZero".into(), "Zero".into()],
        rust_target: RustTarget::RUST_1_64,
        ..Options::default()
    }
}

#[test]
fn defaults_require_zero_validity_independently_of_field_default_traits() {
    for target in Target::ALL {
        let unit = analyze(SOURCE, target).unwrap();
        for copy in [true, false] {
            let mut options = options(copy);
            if target.is_armv7() {
                options.rust_target = RustTarget::stable(78).unwrap();
            }
            let source = generate(&unit, &options).unwrap().source;
            for name in [
                "Scalars",
                "Float",
                "Pointer",
                "Callback",
                "Callback13",
                "Large",
                "ZeroField",
                "EnumArray",
                "EnumUnion",
                "Union",
                "UnionParent",
                "Packed",
                "Flexible",
                "Bitfields",
                "PaddedBits",
            ] {
                assert!(
                    source.contains(&format!("impl ::core::default::Default for {name} {{")),
                    "{target:?}, copy={copy}, {name}"
                );
            }
            for name in ["NonZero", "Zero", "EnumField", "Incomplete", "Atomic"] {
                assert!(
                    !source.contains(&format!("impl ::core::default::Default for {name} {{")),
                    "{target:?}, copy={copy}, {name}"
                );
            }
            if !copy {
                assert!(source.contains("pub value: ::core::mem::ManuallyDrop<NonZero>,"));
            }
        }
    }
}

#[test]
fn shared_record_graphs_and_pointer_cycles_keep_field_trait_constraints() {
    let mut source = String::from("struct Leaf { int value; struct Leaf *next; };\n");
    let mut previous = "Leaf".to_owned();
    for index in 0..24 {
        use std::fmt::Write;
        let name = format!("Node{index}");
        writeln!(
            source,
            "struct {name} {{ struct {previous} left, right; }};"
        )
        .unwrap();
        previous = name;
    }
    source.push_str("struct Bad { enum NonZero { VALUE=2 } value; }; struct BadOuter { struct Bad left, right; };\n");
    let unit = analyze(&source, Target::X86_64UnknownLinuxGnu).unwrap();
    let source = generate(&unit, &options(true)).unwrap().source;
    assert!(source.contains("impl ::core::default::Default for Node23 {"));
    assert!(!source.contains("impl ::core::default::Default for BadOuter {"));
    assert_eq!(source.matches("pub struct Node").count(), 24);
}

#[test]
fn invalid_public_type_ids_fail_before_derive_cache_indexing() {
    use toucan_semantic::{Type, TypeKind};
    for kind in [TypeKind::Record(usize::MAX), TypeKind::Enum(usize::MAX)] {
        let mut unit = analyze(
            "struct Value { int field; };",
            Target::X86_64UnknownLinuxGnu,
        )
        .unwrap();
        unit.records[0].fields.as_mut().unwrap()[0].ty = Type::new(kind);
        let ordinary = generate(&unit, &Options::default())
            .unwrap_err()
            .to_string();
        let requested = generate(&unit, &options(true)).unwrap_err().to_string();
        assert_eq!(ordinary, requested);
        assert!(requested.contains("invalid"));
    }
}

#[test]
fn callback_traits_follow_the_emitted_calling_convention() {
    let input = "struct SysV { int (__attribute__((sysv_abi)) *value)(int); }; struct Win64 { int (__attribute__((ms_abi)) *value)(int); };";
    for (target, ordinary, other) in [
        (Target::X86_64UnknownLinuxGnu, "SysV", "Win64"),
        (Target::X86_64AppleDarwin, "SysV", "Win64"),
        (Target::X86_64PcWindowsMsvc, "Win64", "SysV"),
    ] {
        let unit = analyze(input, target).unwrap();
        let source = generate(&unit, &options(true)).unwrap().source;
        assert!(source.contains(&format!(
            "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct {ordinary}"
        )));
        assert!(source.contains(&format!("#[derive(Clone, Copy)]\npub struct {other}")));
        assert!(source.contains(&format!("impl ::core::default::Default for {other}")));
    }
}

#[test]
fn caller_owned_types_keep_their_trait_boundary_through_aliases() {
    let unit = analyze(
        "typedef unsigned External; typedef External Alias; typedef Alias OtherAlias; struct Owner { OtherAlias value; }; struct ArrayOwner { OtherAlias values[2]; }; union AliasedUnion { OtherAlias value; }; typedef OtherAlias *Pointer; struct PointerOwner { Pointer value; }; struct ExternalRecord { unsigned value; }; typedef struct ExternalRecord RecordAlias; struct RecordOwner { RecordAlias value; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    for copy in [true, false] {
        let source = generate(
            &unit,
            &Options {
                blocklist_types: vec!["External".into(), "ExternalRecord".into()],
                ..options(copy)
            },
        )
        .unwrap()
        .source;
        for name in ["Owner", "ArrayOwner", "AliasedUnion", "RecordOwner"] {
            assert!(!source.contains(&format!("impl ::core::default::Default for {name} {{")));
        }
        assert!(source.contains("pub value: ::core::mem::ManuallyDrop<OtherAlias>,"));
        assert!(source.contains("impl ::core::default::Default for PointerOwner {"));
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

fn compile(source: &str, consumer: &str, expected: bool, diagnostic: &str) {
    compile_with_c(source, consumer, expected, diagnostic, None);
}

fn compile_with_c(
    source: &str,
    consumer: &str,
    expected: bool,
    diagnostic: &str,
    native: Option<(&str, &str)>,
) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("bindings.rs"), source).unwrap();
    std::fs::write(directory.path().join("main.rs"), format!("#![allow(unknown_lints, dead_code, non_camel_case_types, non_upper_case_globals, unpredictable_function_pointer_comparisons)]\ninclude!(\"bindings.rs\");\n{consumer}")).unwrap();
    let executable = directory.path().join("probe");
    let mut command = Command::new("rustc");
    if let Some(toolchain) = std::env::var_os("TOUCAN_TEST_RUST_TOOLCHAIN") {
        command.arg(format!("+{}", toolchain.to_string_lossy()));
    }
    command
        .args(["--edition=2021", "-Dwarnings"])
        .arg(directory.path().join("main.rs"))
        .arg("-o")
        .arg(&executable);
    if let Some((compiler, c_source)) = native {
        let input = directory.path().join("probe.c");
        let object = directory.path().join("probe.o");
        std::fs::write(&input, c_source).unwrap();
        let output = Command::new(compiler)
            .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c"])
            .arg(&input)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert_eq!(
            compiler_acceptance(&output),
            Ok(true),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        toucan_test_support::link_c_object(&mut command, &object);
    }
    let output = command.output().unwrap();
    assert_eq!(
        compiler_acceptance(&output),
        Ok(expected),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if expected {
        assert!(Command::new(executable).status().unwrap().success());
    } else {
        assert!(String::from_utf8_lossy(&output.stderr).contains(diagnostic));
    }
}

#[test]
#[ignore = "requires native C compilers and Rust; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust 1.64"]
fn native_c_reads_valid_zero_defaults() {
    let Some(target) = native_target() else {
        return;
    };
    let prototype = "int default_values_match(const struct Scalars *, const struct Large *, const struct Pointer *, const struct Callback *, const struct ZeroField *, const union Union *);";
    let unit = analyze(&format!("{SOURCE}\n{prototype}"), target).unwrap();
    let c_source = format!(
        "{SOURCE}\nint default_values_match(const struct Scalars *s, const struct Large *a, const struct Pointer *p, const struct Callback *c, const struct ZeroField *e, const union Union *u) {{\nif (s->integer != 0 || s->boolean != 0 || p->value != 0 || c->value != 0 || e->value != ZERO_ZERO || u->integer != 0) return 0;\nfor (int i=0; i<33; ++i) if (a->value[i] != 0) return 0;\nreturn 1;\n}}\n"
    );
    for copy in [true, false] {
        let source = generate(&unit, &options(copy)).unwrap().source;
        for compiler in [
            std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
            "clang".into(),
        ] {
            compile_with_c(
                &source,
                "fn main() { assert_eq!(unsafe { default_values_match(&Scalars::default(), &Large::default(), &Pointer::default(), &Callback::default(), &ZeroField::default(), &Union::default()) }, 1); }",
                true,
                "",
                Some((&compiler, &c_source)),
            );
        }
    }
}

#[test]
#[ignore = "requires native Rust; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust 1.64"]
fn generated_traits_compile_and_safe_defaults_execute() {
    let Some(target) = native_target() else {
        return;
    };
    let unit = analyze(SOURCE, target).unwrap();
    for copy in [true, false] {
        let source = generate(&unit, &options(copy)).unwrap().source;
        compile(
            &source,
            r#"
fn comparable<T: core::fmt::Debug + PartialEq + Eq>() {}
fn main() {
    comparable::<Scalars>();
    comparable::<Pointer>();
    comparable::<Callback>();
    comparable::<CallbackVariadic>();
    comparable::<Large>();
    comparable::<EnumField>();
    let scalar = Scalars::default();
    assert_eq!(scalar.integer, 0);
    assert!(!scalar.boolean);
    assert_eq!(Float::default().value.to_bits(), 0);
    assert_ne!(Float { value: f32::NAN }, Float { value: f32::NAN });
    assert!(Pointer::default().value.is_null());
    assert!(Callback::default().value.is_none());
    assert!(Callback13::default().value.is_none());
    assert_eq!(Large::default().value, [0; 33]);
    assert_eq!(ZeroField::default().value, Zero::ZERO_ZERO);
    assert_eq!(EnumArray::default().value.len(), 0);
    let _no_member_read = EnumUnion::default();
    assert_eq!(unsafe { Union::default().integer }, 0);
    assert_eq!(Nested::default().value.integer, 0);
    assert_eq!(Flexible::default().values.len(), 0);
    assert_eq!(Bitfields::default().first(), 0);
    assert_eq!(PaddedBits::default().first(), 0);
    let packed_value = Packed::default().value;
    assert_eq!(packed_value, 0);
}
"#,
            true,
            "",
        );
        compile(
            &source,
            "fn main() { let _ = EnumField::default(); }",
            false,
            "EnumField",
        );
        compile(
            &source,
            "fn needs_eq<T: Eq>() {} fn main() { needs_eq::<Float>(); }",
            false,
            "Eq",
        );
        compile(
            &source,
            "fn needs_copy<T: Copy>() {} fn main() { needs_copy::<Scalars>(); }",
            copy,
            "Copy",
        );
    }
}

#[test]
#[ignore = "requires native Rust; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust 1.64"]
fn suppression_and_caller_owned_storage_do_not_assume_traits() {
    let Some(target) = native_target() else {
        return;
    };
    let unit = analyze("typedef unsigned External; typedef External Alias; typedef Alias OtherAlias; struct Direct { External value; }; struct Owner { OtherAlias value; }; struct ArrayOwner { OtherAlias value[2]; }; union ExternalUnion { OtherAlias value; }; typedef OtherAlias *Pointer; struct PointerOwner { Pointer value; };", target).unwrap();
    for copy in [true, false] {
        let mut external_options = options(copy);
        external_options.derives.debug = Some(false);
        external_options.blocklist_types.push("External".into());
        external_options
            .raw_lines
            .push("#[repr(transparent)] pub struct External(core::num::NonZeroU32);".into());
        let source = generate(&unit, &external_options).unwrap().source;
        compile(
            &source,
            "fn main() { assert!(PointerOwner::default().value.is_null()); }",
            true,
            "",
        );
        for name in ["Direct", "Owner", "ArrayOwner", "ExternalUnion"] {
            assert!(!source.contains(&format!("impl ::core::default::Default for {name} {{")));
        }
    }
    let unit = analyze(SOURCE, target).unwrap();
    let source = generate(
        &unit,
        &Options {
            derives: DeriveOptions {
                copy: false,
                debug: Some(false),
                ..DeriveOptions::default()
            },
            ..options(false)
        },
    )
    .unwrap()
    .source;
    compile(
        &source,
        "fn main() { let _ = NonZero::NONZERO_TWO.clone(); }",
        true,
        "",
    );
    compile(
        &source,
        "fn needs_debug<T: core::fmt::Debug>() {} fn main() { needs_debug::<NonZero>(); }",
        false,
        "Debug",
    );
}
