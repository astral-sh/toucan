use std::process::Command;

use toucan_bindings::{EnumConstantStyle, Options, RustTarget, generate};
use toucan_semantic::{TagLexicalOrigin, analyze};
use toucan_target::Target;
use toucan_test_support::{compiler_acceptance, link_c_object};

fn options() -> Options {
    Options {
        enum_constant_style: EnumConstantStyle::Bindgen,
        prepend_enum_name: true,
        rust_target: RustTarget::RUST_1_64,
        ..Options::default()
    }
}

#[test]
fn selectors_use_qualified_names_and_shared_anonymous_order() {
    let unit = analyze("struct Outer { enum { FIRST=1 } first; struct { enum Inner { VALUE=2 } field; } second; };", Target::X86_64UnknownLinuxGnu).unwrap();
    for (selector, expected) in [("Inner", false), ("Outer__bindgen_ty_2_Inner", true)] {
        let result = generate(
            &unit,
            &Options {
                rustified_enum_patterns: vec![selector.into()],
                ..options()
            },
        )
        .unwrap();
        assert_eq!(
            result.source.contains("pub enum Outer__bindgen_ty_2_Inner"),
            expected
        );
        assert!(result.source.contains("pub struct Outer__bindgen_ty_2"));
        if !expected {
            assert!(
                result
                    .source
                    .contains("pub const Outer__bindgen_ty_2_Inner_VALUE:")
            );
        }
    }
    let legacy = generate(
        &unit,
        &Options {
            rustified_enum_patterns: vec!["Inner".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(legacy.source.contains("pub enum Inner"));
    assert!(!legacy.source.contains("Outer__bindgen_ty_2_Inner"));
}

#[test]
fn forward_declarations_and_later_aliases_keep_the_native_owner() {
    for (prefix, expected) in [("enum Inner;", "Inner_VALUE"), ("", "Outer_Inner_VALUE")] {
        let unit = analyze(&format!("{prefix} struct Outer {{ enum Inner {{ VALUE=1 }} field; }}; typedef enum Inner Alias;"), Target::X86_64UnknownLinuxGnu).unwrap();
        let result = generate(&unit, &options()).unwrap();
        assert!(result.source.contains(&format!("pub const {expected}:")));
    }
    let unit = analyze("struct Outer { enum { VALUE=1 } field; }; typedef __typeof__(((struct Outer*)0)->field) Alias;", Target::X86_64UnknownLinuxGnu).unwrap();
    let result = generate(
        &unit,
        &Options {
            blocklist_types: vec!["Alias".into()],
            ..options()
        },
    )
    .unwrap();
    assert!(result.source.contains("pub const Outer_VALUE:"));
    assert!(!result.source.contains("pub const Alias_VALUE:"));
    let result = generate(
        &unit,
        &Options {
            rustified_enum_patterns: vec!["VALUE".into()],
            prepend_enum_name: false,
            ..options()
        },
    )
    .unwrap();
    assert!(result.source.contains("pub enum Outer__bindgen_ty_1"));
    assert!(
        result
            .source
            .contains("pub const Outer_VALUE: Outer__bindgen_ty_1")
    );
}

#[test]
fn qualified_type_collisions_and_bad_owner_graphs_diagnose() {
    let unit = analyze(
        "struct Outer { enum Inner { VALUE=1 } field; }; struct Outer_Inner { int field; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    assert!(
        generate(&unit, &options())
            .unwrap_err()
            .to_string()
            .contains("type name `Outer_Inner` conflicts")
    );
    generate(
        &unit,
        &Options {
            allowlist: vec!["Outer".into()],
            ..options()
        },
    )
    .unwrap();
    generate(&unit, &Options::default()).unwrap();
    let mut unit = analyze(
        "struct First { int field; }; struct Second { int field; };",
        Target::X86_64UnknownLinuxGnu,
    )
    .unwrap();
    let first = unit
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("First"))
        .unwrap();
    let second = unit
        .records
        .iter()
        .position(|record| record.name.as_deref() == Some("Second"))
        .unwrap();
    let origin = |record| TagLexicalOrigin {
        record: Some(record),
        order: 0,
        prior_file_declaration: false,
        typedef_declaration: None,
    };
    unit.lexical_tags.records.insert(first, origin(second));
    unit.lexical_tags.records.insert(second, origin(first));
    assert!(
        generate(&unit, &options())
            .unwrap_err()
            .to_string()
            .contains("cyclic lexical")
    );
    unit.lexical_tags.records.insert(first, origin(usize::MAX));
    assert!(
        generate(&unit, &options())
            .unwrap_err()
            .to_string()
            .contains("invalid lexical owner")
    );
}

#[test]
#[ignore = "requires native C and Rust; TOUCAN_TEST_RUST_TOOLCHAIN selects Rust 1.64"]
fn nested_enum_names_compile_with_c_layouts_and_calls() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let header = "struct Outer { enum Inner { VALUE=1 } field; struct { enum { ANON=2 } value; } nested; }; enum Inner echo(enum Inner); int read_outer(const struct Outer *);";
    let unit = analyze(header, target).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let c = directory.path().join("probe.c");
    std::fs::write(&c, format!("{header}\nenum Inner echo(enum Inner value) {{ return value; }} int read_outer(const struct Outer *value) {{ return value->field + value->nested.value; }}")).unwrap();
    for rustified in [false, true] {
        for prefix in [false, true] {
            let result = generate(
                &unit,
                &Options {
                    rustified_enums: rustified,
                    prepend_enum_name: prefix,
                    ..options()
                },
            )
            .unwrap();
            let value = if rustified {
                "Outer_Inner::VALUE"
            } else if prefix {
                "Outer_Inner_VALUE"
            } else {
                "VALUE"
            };
            let anonymous = if rustified || prefix {
                "Outer__bindgen_ty_1_ANON"
            } else {
                "ANON"
            };
            let input = directory.path().join("main.rs");
            std::fs::write(&input, format!("#![allow(dead_code,non_camel_case_types,non_upper_case_globals)]\n{}\nfn main() {{ let value=Outer {{ field:{value}, nested:Outer__bindgen_ty_1 {{ value:{anonymous} }} }}; assert_eq!(unsafe{{read_outer(&value)}},3); assert_eq!(unsafe{{echo({value})}},{value}); }}", result.source)).unwrap();
            for compiler in [
                std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
                "clang".into(),
            ] {
                let object = directory.path().join("probe.o");
                let output = Command::new(compiler)
                    .args(["-std=c11", "-O2", "-c"])
                    .arg(&c)
                    .arg("-o")
                    .arg(&object)
                    .output()
                    .unwrap();
                assert_eq!(
                    compiler_acceptance(&output),
                    Ok(true),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
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
