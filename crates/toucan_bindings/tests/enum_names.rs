use std::collections::BTreeMap;
use std::process::Command;

use toucan_bindings::{
    BindingSelection, EnumConstantStyle, MacroValue, Options, RustTarget, generate,
    generate_with_macros,
};
use toucan_semantic::{IntegerValue, analyze};
use toucan_target::Target;
use toucan_test_support::{compiler_acceptance, link_c_object};

const SOURCE: &str = "enum Named { A=1, B=1 }; typedef enum { C=2, D=2 } First; typedef First Later; enum { FREE=3, SAME=3 };";

fn options() -> Options {
    Options {
        prepend_enum_name: true,
        enum_constant_style: EnumConstantStyle::Bindgen,
        rust_target: RustTarget::RUST_1_64,
        ..Options::default()
    }
}

#[test]
fn enum_names_and_metadata_preserve_c_selection() {
    let unit = analyze(SOURCE, Target::X86_64UnknownLinuxGnu).unwrap();
    for prefix in [true, false] {
        let options = Options {
            prepend_enum_name: prefix,
            ..options()
        };
        let result = generate(&unit, &options).unwrap();
        let names: Vec<_> = result
            .enum_constants
            .iter()
            .flat_map(|item| item.emitted.iter().map(|value| value.rust_name.as_str()))
            .collect();
        assert_eq!(
            names,
            if prefix {
                vec!["Named_A", "Named_B", "First_C", "First_D", "FREE", "SAME"]
            } else {
                vec!["A", "B", "C", "D", "FREE", "SAME"]
            }
        );
        assert_eq!(result.enum_constants[0].emitted[0].c_name, "A");
        let result = generate(
            &unit,
            &Options {
                selection: Some(Box::new(BindingSelection {
                    constants: ["A".into()].into(),
                    ..BindingSelection::default()
                })),
                ..options
            },
        )
        .unwrap();
        assert_eq!(result.enum_constants.len(), 1);
        assert_eq!(result.enum_constants[0].emitted.len(), 1);
    }
    let core = generate(&unit, &Options::default()).unwrap();
    assert!(core.source.contains("pub const A:"));
    assert!(!core.source.contains("pub const Named_A:"));
}

#[test]
fn scoped_rust_enums_keep_only_anonymous_typed_globals() {
    let unit = analyze(SOURCE, Target::X86_64UnknownLinuxGnu).unwrap();
    let result = generate(
        &unit,
        &Options {
            rustified_enums: true,
            ..options()
        },
    )
    .unwrap();
    assert!(!result.source.contains("pub const A:"));
    assert!(!result.source.contains("pub const Named_A:"));
    assert!(!result.source.contains("pub const First_C:"));
    assert!(
        result
            .source
            .contains("pub const FREE: __toucan_enum_2 = __toucan_enum_2::FREE;")
    );
    assert!(
        result
            .source
            .contains("pub const SAME: __toucan_enum_2 = __toucan_enum_2::SAME;")
    );
    assert_eq!(result.enum_constants.len(), 1);
    let core = generate(
        &unit,
        &Options {
            rustified_enums: true,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(
        core.source
            .contains("pub const A: ::core::primitive::u32 = 1;")
    );
}

#[test]
fn prefixes_preserve_macro_values_and_reject_actual_collisions() {
    let unit = analyze(
        "enum Named { A=1 }; int other(void);",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let integer = || {
        Some(MacroValue::Integer(IntegerValue {
            value: 2,
            bits: 32,
            signed: true,
            rank: 3,
        }))
    };
    let macros = BTreeMap::from([("A".into(), integer())]);
    let result = generate_with_macros(&unit, &options(), &macros).unwrap();
    assert!(
        result
            .source
            .contains("pub const Named_A: ::core::primitive::u32 = 1;")
    );
    assert!(
        result
            .source
            .contains("pub const A: ::core::primitive::i32 = 2;")
    );
    assert_eq!(result.enum_constants[0].emitted[0].rust_name, "Named_A");
    for options in [
        Options {
            prepend_enum_name: false,
            ..options()
        },
        Options {
            generated_names: [("other".into(), "Named_A".into())].into(),
            ..options()
        },
    ] {
        assert!(
            generate_with_macros(&unit, &options, &macros)
                .unwrap_err()
                .to_string()
                .contains("conflicts")
        );
    }
    let macros = BTreeMap::from([("Named_A".into(), integer())]);
    assert!(
        generate_with_macros(&unit, &options(), &macros)
            .unwrap_err()
            .to_string()
            .contains("conflicts")
    );
    let macros = BTreeMap::from([("A".into(), None)]);
    assert!(
        generate_with_macros(&unit, &options(), &macros)
            .unwrap()
            .source
            .contains("pub const Named_A:")
    );
    let result = generate_with_macros(
        &unit,
        &Options {
            blocklist_types: vec!["Named".into()],
            ..options()
        },
        &macros,
    )
    .unwrap();
    assert!(!result.source.contains("pub const Named_A:"));
}

#[test]
fn keyword_parts_are_escaped_before_prefixing() {
    let unit = analyze(
        "enum type { match=1, self=2 }; enum bool { str=3 };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let result = generate(&unit, &options()).unwrap();
    for name in ["type__match_", "type__self_", "bool__str_"] {
        assert!(result.source.contains(&format!("pub const {name}:")));
    }
}

#[test]
#[ignore = "requires native GCC/Clang and Rust; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust 1.64"]
fn generated_names_and_scoped_constants_compile_and_call_c() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let header = format!(
        "{SOURCE}\nenum Named echo_named(enum Named); First echo_first(First); int original_a(void); int macro_a(void);"
    );
    let unit = analyze(&header, target).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let c = directory.path().join("probe.c");
    std::fs::write(&c, format!("{header}\nstatic const int original = A;\n#define A 2\nenum Named echo_named(enum Named value) {{ return value; }} First echo_first(First value) {{ return value; }} int original_a(void) {{ return original; }} int macro_a(void) {{ return A; }}")).unwrap();
    for rustified in [true, false] {
        for prefix in [true, false] {
            let macros = if prefix {
                BTreeMap::from([(
                    "A".into(),
                    Some(MacroValue::Integer(IntegerValue {
                        value: 2,
                        bits: 32,
                        signed: true,
                        rank: 3,
                    })),
                )])
            } else {
                BTreeMap::new()
            };
            let source = generate_with_macros(
                &unit,
                &Options {
                    rustified_enums: rustified,
                    prepend_enum_name: prefix,
                    ..options()
                },
                &macros,
            )
            .unwrap()
            .source;
            let (named, first) = if rustified {
                ("Named::A", "First::C")
            } else if prefix {
                ("Named_A", "First_C")
            } else {
                ("A", "C")
            };
            let macro_check = if prefix {
                "assert_eq!(A, unsafe { macro_a() });"
            } else {
                ""
            };
            let input = directory.path().join("main.rs");
            std::fs::write(&input, format!("#![allow(dead_code, non_camel_case_types, non_upper_case_globals)]\n{source}\nfn main() {{ assert_eq!(unsafe {{ echo_named({named}) }}, {named}); assert_eq!({named} as i32, unsafe {{ original_a() }}); {macro_check} assert_eq!(unsafe {{ echo_first({first}) }}, {first}); assert_eq!(FREE, SAME); }}")).unwrap();
            for compiler in [
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
                "clang".into(),
            ] {
                let object = directory.path().join("probe.o");
                let c_output = Command::new(compiler)
                    .args(["-std=c11", "-O2", "-c"])
                    .arg(&c)
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert_eq!(
                    compiler_acceptance(&c_output),
                    Ok(true),
                    "{}",
                    String::from_utf8_lossy(&c_output.stderr)
                );
                let executable = directory.path().join("probe");
                let mut rustc = Command::new("rustc");
                if let Some(toolchain) = std::env::var_os("TOUCAN_TEST_RUST_TOOLCHAIN") {
                    rustc.arg(format!("+{}", toolchain.to_string_lossy()));
                }
                rustc
                    .args(["--edition=2021", "-Dwarnings"])
                    .arg(&input)
                    .arg("-o")
                    .arg(&executable);
                link_c_object(&mut rustc, &object);
                let output = rustc.output().unwrap();
                assert_eq!(
                    compiler_acceptance(&output),
                    Ok(true),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(Command::new(executable).status().unwrap().success());
            }
        }
    }
}
