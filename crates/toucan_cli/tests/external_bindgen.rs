use std::process::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bindgen"))
}

#[test]
fn version_identifies_toucan_without_claiming_a_libclang_version() {
    let output = command().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("bindgen {} (Toucan)\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn aws_lc_external_invocation_selects_headers_and_prefixes_native_symbols() {
    let directory = tempfile::tempdir().unwrap();
    let openssl = directory.path().join("openssl");
    std::fs::create_dir(&openssl).unwrap();
    let wrapper = directory.path().join("rust_wrapper.h");
    let rust = directory.path().join("bindings.rs");
    std::fs::write(
        openssl.join("crypto.h"),
        "#define HEADER_VALUE 7\ntypedef enum { POINT_ONE = 1, POINT_TWO = 2 } point_conversion_form_t;\nstruct Keep { int value; };\nint ping(struct Keep *);\nextern const int value;\n",
    )
    .unwrap();
    std::fs::write(&wrapper, "#include <openssl/crypto.h>\n").unwrap();
    let output = command()
        .env("PATH", directory.path())
        .args(["--prefix-link-name", "aws_lc_0_44_0_"])
        .args(["--allowlist-file", r".*(/|\\)openssl((/|\\)[^/\\]+)+\.h"])
        .args(["--allowlist-file", r".*(/|\\)rust_wrapper\.h"])
        .args(["--rustified-enum", "point_conversion_form_t"])
        .args(["--default-macro-constant-type", "signed"])
        .args([
            "--with-derive-default",
            "--with-derive-partialeq",
            "--with-derive-eq",
        ])
        .args(["--raw-line", "// Caller owns this line."])
        .args([
            "--generate",
            "functions,types,vars,methods,constructors,destructors",
        ])
        .arg(&wrapper)
        .args(["--rust-target", "1.70", "--output"])
        .arg(&rust)
        .args(["--formatter", "none", "--", "-I"])
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = std::fs::read_to_string(rust).unwrap();
    assert!(output.contains("// Caller owns this line."), "{output}");
    assert!(
        output.contains("pub enum point_conversion_form_t"),
        "{output}"
    );
    assert!(output.contains("pub struct Keep"), "{output}");
    assert!(output.contains("pub fn ping("), "{output}");
    assert!(
        output.contains("#[link_name = \"aws_lc_0_44_0_ping\"]"),
        "{output}"
    );
    assert!(
        output.contains("#[link_name = \"aws_lc_0_44_0_value\"]"),
        "{output}"
    );
    assert!(output.contains("HEADER_VALUE"), "{output}");
}

#[test]
fn unsupported_options_and_frontend_arguments_preserve_existing_bindings() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("input.h");
    let output = directory.path().join("bindings.rs");
    std::fs::write(&header, "int ping(void);\n").unwrap();
    std::fs::write(&output, "existing bindings\n").unwrap();
    for options in [
        vec!["--blocklist-function", "ping"],
        vec!["--generate", "functions,types"],
        vec!["--prefix-link-name", ""],
    ] {
        let result = command()
            .args(options)
            .arg(&header)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            !result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            std::fs::read_to_string(&output).unwrap(),
            "existing bindings\n"
        );
    }
    let unsupported = command()
        .arg(&header)
        .arg("--output")
        .arg(&output)
        .args(["--", "-Wno-imaginary-option"])
        .output()
        .unwrap();
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("unsupported Clang argument"));
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "existing bindings\n"
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn prefixed_generated_bindings_link_and_call_native_c() {
    let directory = tempfile::tempdir().unwrap();
    let header = directory.path().join("abi.h");
    let source = directory.path().join("abi.c");
    let rust = directory.path().join("bindings.rs");
    let object = directory.path().join("abi.o");
    let program = directory.path().join("main.rs");
    let executable = directory.path().join("check");
    std::fs::write(&header, "int add(int value); extern const int number;\n").unwrap();
    std::fs::write(
        &source,
        "int aws_lc_0_44_0_add(int value) { return value + 4; } const int aws_lc_0_44_0_number = 7;\n",
    )
    .unwrap();
    let result = command()
        .args(["--prefix-link-name", "aws_lc_0_44_0_"])
        .arg(&header)
        .args(["--rust-target", "1.70", "--output"])
        .arg(&rust)
        .args(["--formatter", "none"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let c = Command::new("cc")
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
    std::fs::write(
        &program,
        format!(
            "#![allow(dead_code)]\ninclude!({:?});\nfn main() {{ assert_eq!(unsafe {{ add(3) }}, 7); assert_eq!(unsafe {{ number }}, 7); }}\n",
            rust.display().to_string()
        ),
    )
    .unwrap();
    let rustc = Command::new("rustc")
        .args(["--edition=2021", "-C", "opt-level=0"])
        .arg(&program)
        .arg("-C")
        .arg(format!("link-arg={}", object.display()))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let runtime = Command::new(executable).output().unwrap();
    assert!(
        runtime.status.success(),
        "{}",
        String::from_utf8_lossy(&runtime.stderr)
    );
}
