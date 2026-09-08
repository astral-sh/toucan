use std::path::Path;
use std::process::Command;

use toucan::{BindingOptions, Config, Target};

fn parse(source: &str) -> toucan::Compilation {
    let mut config = Config::new(Target::X86_64UnknownLinuxGnu);
    config.preprocessor.allow_filesystem = false;
    config.preprocessor.defines.clear();
    toucan::parse_source(Path::new("macros.h"), source, &config).unwrap()
}

#[test]
fn macros_replace_enumerators_without_changing_semantic_values() {
    let compilation = parse(
        "enum Named { REPLACED = 1, SELF = 3, OMITTED = 4, STRING = 5 };\n\
         #define REPLACED 4294967295U\n\
         #define SELF SELF\n\
         #define OMITTED unknown()\n\
         #define STRING \"ok\"\n\
         #define ALIAS REPLACED\n",
    );
    let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(source.contains("pub const REPLACED: ::core::primitive::u32 = 4294967295;"));
    assert!(source.contains("pub const ALIAS: ::core::primitive::u32 = 4294967295;"));
    assert!(source.contains("pub const SELF: ::core::primitive::u32 = 3;"));
    assert!(source.contains("pub const STRING: &[::core::primitive::u8; 3]"));
    assert!(!source.contains("pub const OMITTED:"));
    assert_eq!(report.integer_macros, 2);
    assert_eq!(report.string_macros, 1);
    assert_eq!(report.skipped_macros.len(), 1);
    assert_eq!(report.skipped_macros[0].name, "OMITTED");
    assert_eq!(report.enum_constants.len(), 1);
    assert_eq!(report.enum_constants[0].emitted.len(), 1);
    assert_eq!(report.enum_constants[0].emitted[0].c_name, "SELF");
    assert_eq!(compilation.unit.constants["REPLACED"].value, 1);
}

#[test]
fn function_macros_leave_bare_enumerators_available() {
    let compilation = parse("enum { VALUE = 1 };\n#define VALUE(x) (x)\n");
    let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(source.contains("pub const VALUE: ::core::primitive::u32 = 1;"));
    assert_eq!(report.enum_constants[0].emitted[0].c_name, "VALUE");
    assert_eq!(report.skipped_macros[0].reason, "function-like macro");
}

#[test]
fn reserved_macro_names_are_checked_when_shadowing_or_explicitly_selected() {
    let compilation = parse("enum { __VALUE = 1 };\n#define __VALUE 2\n#define __EXPLICIT 3\n");
    let (source, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert!(source.contains("pub const __VALUE: ::core::primitive::i32 = 2;"));
    assert!(!source.contains("pub const __EXPLICIT:"));
    assert!(report.enum_constants.is_empty());
    for pattern in ["__EXPLICIT", "*"] {
        let (source, _) = compilation
            .bindings(&BindingOptions {
                allowlist: vec![pattern.into()],
            })
            .unwrap();
        assert!(source.contains("pub const __EXPLICIT: ::core::primitive::i32 = 3;"));
    }
}

#[test]
fn declaration_collisions_fail_before_emitting_rust() {
    for declaration in [
        "extern int value;",
        "int value(void);",
        "typedef int value;",
        "struct value { int member; };",
        "enum value { MEMBER };",
    ] {
        let compilation = parse(&format!("{declaration}\n#define value 1\n"));
        let error = compilation
            .bindings(&BindingOptions::default())
            .unwrap_err();
        assert!(error.to_string().contains("macro `value` conflicts"));
    }
}

#[test]
#[ignore = "requires native C compilers and rustc; run with --include-ignored"]
fn emitted_macro_values_and_names_match_c_and_compile_as_rust() {
    let target = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => Target::X86_64UnknownLinuxGnu,
        ("aarch64", "linux") => Target::Aarch64UnknownLinuxGnu,
        ("x86_64", "macos") => Target::X86_64AppleDarwin,
        ("aarch64", "macos") => Target::Aarch64AppleDarwin,
        _ => return,
    };
    let header = "enum { VALUE = 1, self = 2, __toucan_self = 3 };\n\
                  #define VALUE 4294967295U\n\
                  #define self 7\n";
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::new(target);
    config.preprocessor.allow_filesystem = false;
    let compilation = toucan::parse_source(Path::new("api.h"), header, &config).unwrap();
    let (bindings, report) = compilation.bindings(&BindingOptions::default()).unwrap();
    assert_eq!(report.enum_constants[0].emitted.len(), 1);
    assert_eq!(report.enum_constants[0].emitted[0].c_name, "__toucan_self");
    assert_eq!(report.renamed_macros["__toucan_self_"], "self");
    let rust = format!(
        "#![allow(dead_code, non_upper_case_globals)]\n{bindings}\n\
         fn main() {{\n\
             let _: u32 = VALUE;\n\
             println!(\"{{}} {{}} {{}}\", VALUE, __toucan_self_, __toucan_self);\n\
         }}\n"
    );
    std::fs::write(directory.path().join("probe.rs"), rust).unwrap();
    std::fs::write(
        directory.path().join("probe.c"),
        format!(
            "#include <stdio.h>\n{header}\n\
             _Static_assert(_Generic(VALUE, unsigned int: 1, default: 0), \"macro type\");\n\
             int main(void) {{ printf(\"%u %d %d\\n\", VALUE, self, __toucan_self); }}\n"
        ),
    )
    .unwrap();
    let rust_executable = directory.path().join("rust-probe");
    let output = Command::new("rustc")
        .args(["--edition=2024", "probe.rs", "-o"])
        .arg(&rust_executable)
        .current_dir(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = Command::new(rust_executable).output().unwrap();
    assert!(expected.status.success());
    let mut checked = 0;
    for compiler in ["gcc", "clang"] {
        if Command::new(compiler).arg("--version").output().is_err() {
            continue;
        }
        let executable = directory.path().join(compiler);
        let output = Command::new(compiler)
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "probe.c", "-o"])
            .arg(&executable)
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = Command::new(executable).output().unwrap();
        assert!(actual.status.success());
        assert_eq!(actual.stdout, expected.stdout);
        checked += 1;
    }
    assert!(
        checked > 0,
        "a C compiler is required for this differential test"
    );
}
