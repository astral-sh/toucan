use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use toucan_preprocessor::{Config, Preprocessor};

const SOURCE: &str = concat!(
    "#if 255i8 != 255 || 256i8 != 256 || 65536i16 != 65536 || 4294967296i32 != 4294967296\n#error truncated literal\n#endif\n",
    "#if 0xffffffffffffffffi64 <= 0 || 0xffffffffffffffffui8 != 0xffffffffffffffffULL\n#error incorrect signedness\n#endif\n",
    "#if 1I8 != 1 || 1Ui16 != 1 || 1uI32 != 1 || 1UI64 != 1\n#error case variants\n#endif\nint value;\n"
);

#[test]
fn microsoft_integer_conditions_use_intmax_without_width_truncation() {
    let output = Preprocessor::new(Config {
        ms_extensions: true,
        ..Default::default()
    })
    .preprocess_str(Path::new("msvc.h"), SOURCE)
    .unwrap();
    assert!(output.source.contains("int value"));
    assert!(
        Preprocessor::new(Config::default())
            .preprocess_str(Path::new("msvc.h"), SOURCE)
            .is_err()
    );
    for literal in [
        "1i64u",
        "1i128",
        "18446744073709551616i8",
        "18446744073709551616ui64",
    ] {
        let source = format!("#if {literal}\n#endif\n");
        assert!(
            Preprocessor::new(Config {
                ms_extensions: true,
                ..Default::default()
            })
            .preprocess_str(Path::new("msvc.h"), &source)
            .is_err()
        );
    }
}

#[test]
#[ignore = "requires Clang with the Windows x64 and ARM64 cross targets"]
fn microsoft_integer_conditions_match_clang() {
    for target in ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
        let mut child = Command::new("clang")
            .args([
                "-target", target, "-std=c11", "-Werror", "-E", "-P", "-x", "c", "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(SOURCE.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            "int value;"
        );
    }
}
