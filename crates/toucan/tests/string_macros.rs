use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use toucan::{BindingOptions, Config, RustTarget, Target};

struct Case {
    name: &'static str,
    literal: &'static str,
    c_type: &'static str,
    rust_type: &'static str,
    units: Vec<u32>,
}

fn cases(target: Target) -> Vec<Case> {
    let wide16 = target.wchar_width() == 16;
    let wide_type = if wide16 {
        "u16"
    } else if target.wchar_is_signed() {
        "i32"
    } else {
        "u32"
    };
    vec![
        Case {
            name: "M_BYTES",
            literal: r#""\xff\0\x40""#,
            c_type: "char",
            rust_type: "u8",
            units: vec![255, 0, 64, 0],
        },
        Case {
            name: "M_UTF8",
            literal: r#"u8"é\u4f60\U0001f600""#,
            c_type: "char",
            rust_type: "u8",
            units: "é你😀\0".bytes().map(u32::from).collect(),
        },
        Case {
            name: "M_OCTAL",
            literal: r#""\0123" "\x41""#,
            c_type: "char",
            rust_type: "u8",
            units: vec![10, 51, 65, 0],
        },
        Case {
            name: "M_JOIN",
            literal: r#""\x1234" L"""#,
            c_type: "__WCHAR_TYPE__",
            rust_type: wide_type,
            units: vec![0x1234, 0],
        },
        Case {
            name: "M_U16",
            literal: r#"u"é😀\xd800""#,
            c_type: "unsigned short",
            rust_type: "u16",
            units: vec![233, 0xd83d, 0xde00, 0xd800, 0],
        },
        Case {
            name: "M_U32",
            literal: r#"U"é😀\xffffffff""#,
            c_type: "unsigned int",
            rust_type: "u32",
            units: vec![233, 0x1f600, u32::MAX, 0],
        },
        Case {
            name: "M_WIDE",
            literal: r#"L"é😀""#,
            c_type: "__WCHAR_TYPE__",
            rust_type: wide_type,
            units: if wide16 {
                vec![233, 0xd83d, 0xde00, 0]
            } else {
                vec![233, 0x1f600, 0]
            },
        },
        Case {
            name: "M_WIDE_BITS",
            literal: if wide16 {
                r#"L"\xffff""#
            } else {
                r#"L"\xffffffff""#
            },
            c_type: "__WCHAR_TYPE__",
            rust_type: wide_type,
            units: vec![if wide16 { 65535 } else { u32::MAX }, 0],
        },
        Case {
            name: "M_EMPTY",
            literal: r#"u"""#,
            c_type: "unsigned short",
            rust_type: "u16",
            units: vec![0],
        },
    ]
}

fn header(cases: &[Case]) -> String {
    cases
        .iter()
        .map(|case| format!("#define {} {}\n", case.name, case.literal))
        .collect()
}

fn parse(source: &str, target: Target) -> toucan::Compilation {
    let mut config = Config::new(target);
    config.preprocessor.defines.clear();
    toucan::parse_source(Path::new("strings.h"), source, &config).unwrap()
}

fn options() -> BindingOptions {
    BindingOptions {
        allowlist: vec!["M_*".into(), "BAD_*".into(), "C_*".into()],
        rust_target: RustTarget::RUST_1_64,
        ..Default::default()
    }
}

#[test]
fn strings_preserve_target_types_and_all_code_units() {
    for target in Target::ALL {
        let cases = cases(target);
        let compilation = parse(&header(&cases), target);
        let (source, report) = compilation.bindings(&options()).unwrap();
        assert_eq!(report.string_macros, cases.len());
        assert_eq!(report.integer_macros, 0);
        assert!(
            report.skipped_macros.is_empty(),
            "{:?}",
            report.skipped_macros
        );
        for case in cases {
            let values: Vec<_> = case
                .units
                .iter()
                .map(|unit| {
                    if case.rust_type == "i32" {
                        (*unit as i32).to_string()
                    } else {
                        unit.to_string()
                    }
                })
                .collect();
            assert!(
                source.contains(&format!(
                    "pub const {}: &[::core::primitive::{}; {}] = &[{}];",
                    case.name,
                    case.rust_type,
                    values.len(),
                    values.join(", ")
                )),
                "{target}: {}: {source}",
                case.name
            );
        }
    }
}

#[test]
fn invalid_string_macros_report_literal_errors_and_cstr_keeps_its_byte_contract() {
    let compilation = parse(
        "#define BAD_RANGE u\"\\x10000\"\n#define BAD_UCN \"\\u0041\"\n#define BAD_MIX u8\"x\" L\"y\"\n",
        Target::X86_64UnknownLinuxGnu,
    );
    let (_, report) = compilation.bindings(&options()).unwrap();
    assert_eq!(report.string_macros, 0);
    assert_eq!(report.skipped_macros.len(), 3);
    assert!(
        report
            .skipped_macros
            .iter()
            .any(|item| item.reason.contains("code-unit range"))
    );
    assert!(
        report
            .skipped_macros
            .iter()
            .any(|item| item.reason.contains("universal character name"))
    );
    assert!(
        report
            .skipped_macros
            .iter()
            .any(|item| item.reason.contains("incompatible adjacent"))
    );
    let compilation = parse(
        "#define C_TEXT u8\"é\\xff\"\n#define C_WIDE u\"é\\0\"\n",
        Target::X86_64UnknownLinuxGnu,
    );
    let (source, report) = compilation
        .bindings(&BindingOptions {
            generate_cstr: true,
            ..options()
        })
        .unwrap();
    assert_eq!(report.string_macros, 2);
    assert!(source.contains("pub const C_TEXT: &::core::ffi::CStr"));
    assert!(source.contains("from_bytes_with_nul_unchecked(&[195, 169, 255, 0])"));
    assert!(source.contains("pub const C_WIDE: &[::core::primitive::u16; 3] = &[233, 0, 0];"));
    let bad = parse("#define C_BAD u8\"a\\0\"\n", Target::X86_64UnknownLinuxGnu);
    assert!(
        bad.bindings(&BindingOptions {
            generate_cstr: true,
            ..options()
        })
        .unwrap_err()
        .to_string()
        .contains("interior NUL")
    );
}

#[test]
fn public_wide_macro_values_cannot_truncate_or_claim_another_element_type() {
    use toucan::semantic::IntegerKind;
    use toucan_bindings::MacroValue;
    let unit = toucan::semantic::analyze("", Target::X86_64UnknownLinuxGnu).unwrap();
    for (element_type, code_units) in [
        (IntegerKind::UnsignedShort, vec![65536]),
        (IntegerKind::UnsignedLong, vec![1]),
    ] {
        let macros = [(
            "M_BAD".into(),
            Some(MacroValue::WideString {
                element_type,
                code_units,
            }),
        )]
        .into();
        assert!(toucan_bindings::generate_with_macros(&unit, &options(), &macros).is_err());
    }
}

#[test]
fn i686_gcc_wide_long_macros_keep_signed_code_units() {
    use toucan::semantic::IntegerKind;
    use toucan_bindings::MacroValue;

    let macros = [(
        "M_WIDE_LONG".into(),
        Some(MacroValue::WideString {
            element_type: IntegerKind::Long,
            code_units: vec![u32::MAX],
        }),
    )]
    .into();
    let i686 = toucan::semantic::analyze("", Target::I686UnknownLinuxGnu).unwrap();
    let bindings = toucan_bindings::generate_with_macros(&i686, &options(), &macros).unwrap();
    assert!(
        bindings
            .source
            .contains("pub const M_WIDE_LONG: &[::core::primitive::i32; 2] = &[-1, 0];"),
        "{}",
        bindings.source
    );

    let x86_64 = toucan::semantic::analyze("", Target::X86_64UnknownLinuxGnu).unwrap();
    assert!(toucan_bindings::generate_with_macros(&x86_64, &options(), &macros).is_err());
}

fn c_source(cases: &[Case]) -> String {
    let mut source = header(cases);
    for case in cases {
        source.push_str(&format!(
            "{} {}_object[] = {};\n",
            case.c_type, case.name, case.name
        ));
    }
    source
}

fn c_compile(compiler: &str, source: &str, arguments: &[&str]) -> std::process::Output {
    let mut child = Command::new(compiler)
        .args(["-std=c11", "-pedantic-errors", "-x", "c", "-"])
        .args(arguments)
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
    child.wait_with_output().unwrap()
}

/// LLVM preserves initializer code units without depending on object-file format
/// or the host's endianness. Integer operands may print as signed bit patterns.
fn llvm_units(line: &str) -> (usize, Vec<u32>) {
    let (_, array) = line.split_once("global [").unwrap();
    let (shape, value) = array.split_once("] ").unwrap();
    let (length, bits) = shape.split_once(" x i").unwrap();
    let length: usize = length.parse().unwrap();
    let bits: usize = bits.parse().unwrap();
    let units = if value.starts_with("zeroinitializer") {
        vec![0; length]
    } else if let Some(value) = value.strip_prefix("c\"") {
        let value = value.split('"').next().unwrap().as_bytes();
        let mut units = Vec::new();
        let mut index = 0;
        while index < value.len() {
            if value[index] == b'\\' {
                units.push(
                    u32::from_str_radix(
                        std::str::from_utf8(&value[index + 1..index + 3]).unwrap(),
                        16,
                    )
                    .unwrap(),
                );
                index += 3;
            } else {
                units.push(u32::from(value[index]));
                index += 1;
            }
        }
        units
    } else {
        value
            .strip_prefix('[')
            .unwrap()
            .split(']')
            .next()
            .unwrap()
            .split(',')
            .map(|unit| {
                let value: i64 = unit.split_whitespace().last().unwrap().parse().unwrap();
                (value as u64 & ((1u64 << bits) - 1)) as u32
            })
            .collect()
    };
    assert_eq!(units.len(), length);
    (bits, units)
}

#[test]
#[ignore = "requires Clang with all five targets; run with --include-ignored"]
fn macro_code_units_match_clang_on_every_target() {
    for target in Target::ALL {
        let cases = cases(target);
        let output = c_compile(
            "clang",
            &c_source(&cases),
            &["-target", target.triple(), "-S", "-emit-llvm", "-o", "-"],
        );
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let llvm = String::from_utf8(output.stdout).unwrap();
        for case in cases {
            let line = llvm
                .lines()
                .find(|line| line.starts_with(&format!("@{}_object =", case.name)))
                .unwrap();
            let (bits, units) = llvm_units(line);
            let expected_bits = match case.rust_type {
                "u8" => 8,
                "u16" => 16,
                _ => 32,
            };
            assert_eq!(bits, expected_bits, "{target}: {}", case.name);
            assert_eq!(units, case.units, "{target}: {}", case.name);
        }
    }
}

fn rust_run(source: &str) {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("strings.rs");
    let executable = directory.path().join("strings");
    std::fs::write(&input, source).unwrap();
    let mut command = match std::env::var("TOUCAN_TEST_RUST_TOOLCHAIN") {
        Ok(toolchain) => {
            let mut command = Command::new("rustup");
            command.args(["run", &toolchain, "rustc"]);
            command
        }
        Err(_) => Command::new("rustc"),
    };
    let output = command
        .args(["--edition=2021", "-D", "warnings"])
        .arg(input)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(Command::new(executable).status().unwrap().success());
}

#[test]
#[ignore = "requires native GCC, Clang, rustc; TOUCAN_TEST_RUST_TOOLCHAIN selects rustup toolchain"]
fn emitted_arrays_and_cstr_match_native_c_and_compile_as_rust() {
    for target in Target::ALL {
        let cases = cases(target);
        let (bindings, _) = parse(&header(&cases), target).bindings(&options()).unwrap();
        // Constant arrays are portable Rust. Check each target's projection on
        // the host independently of the full bindings' deliberate target guard.
        let mut rust = bindings
            .lines()
            .filter(|line| line.starts_with("pub const M_"))
            .collect::<Vec<_>>()
            .join("\n");
        rust.push_str("\nfn main() {\n");
        for case in &cases {
            rust.push_str(&format!(
                "let _: &[{}; {}] = {};\n",
                case.rust_type,
                case.units.len(),
                case.name
            ));
            rust.push_str(&format!(
                "assert_eq!({}.iter().map(|unit| *unit as u32).collect::<Vec<_>>(), {:?});\n",
                case.name, case.units
            ));
        }
        rust.push_str("}\n");
        rust_run(&rust);
    }
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        host => panic!("unsupported native C test host: {host:?}"),
    };
    let cases = cases(target);
    let mut c = c_source(&cases);
    c.push_str("int main(void) {\n");
    for case in &cases {
        let unsigned_type = match case.rust_type {
            "u8" => "unsigned char",
            "u16" => "unsigned short",
            _ => "unsigned int",
        };
        c.push_str(&format!(
            "if (sizeof({0}_object)/sizeof({0}_object[0]) != {1}) return 1;\n",
            case.name,
            case.units.len()
        ));
        for (index, unit) in case.units.iter().enumerate() {
            c.push_str(&format!(
                "if (({unsigned_type}){}_object[{index}] != {unit}u) return 2;\n",
                case.name
            ));
        }
    }
    c.push_str("return 0; }\n");
    for compiler in ["gcc", "clang"] {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("strings");
        let output = c_compile(compiler, &c, &["-o", executable.to_str().unwrap()]);
        assert!(
            output.status.success(),
            "{compiler}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(Command::new(executable).status().unwrap().success());
    }
    let compilation = parse(
        "#define C_TEXT u8\"é\\xff\"\n#define C_WIDE u\"é\\0\"\n",
        target,
    );
    let (mut rust, _) = compilation
        .bindings(&BindingOptions {
            generate_cstr: true,
            ..options()
        })
        .unwrap();
    rust.push_str("\nfn main() { assert_eq!(C_TEXT.to_bytes_with_nul(), &[195, 169, 255, 0]); assert_eq!(C_WIDE, &[233, 0, 0]); }\n");
    rust_run(&rust);
}
