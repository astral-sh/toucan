use std::path::Path;

use toucan::{Config, Target};

const SOURCE: &str = r#"
#define DO(x) _Pragma(#x)
DO(pack(push, 1))
typedef struct Packed { char byte; int value; } Packed;
DO(pack(push, 2))
typedef struct TwoByte { char byte; int value; } TwoByte;
DO(pack(pop))
typedef struct Restored { char byte; int value; } Restored;
DO(pack(pop))
typedef struct Natural { char byte; int value; } Natural;
_Static_assert(sizeof(Packed) == 5, "packed size");
_Static_assert(_Alignof(Packed) == 1, "packed alignment");
_Static_assert(sizeof(TwoByte) == 6, "nested size");
_Static_assert(_Alignof(TwoByte) == 2, "nested alignment");
_Static_assert(sizeof(Restored) == 5, "restored size");
_Static_assert(sizeof(Natural) == 8, "natural size");
_Static_assert(_Alignof(Natural) == 4, "natural alignment");
"#;

#[test]
fn pragma_operators_control_record_layout() {
    for target in Target::ALL {
        let mut config = Config::new(target);
        config.preprocessor.allow_filesystem = false;
        toucan::parse_source(Path::new("pragma.h"), SOURCE, &config).unwrap();
    }
}

#[test]
#[ignore = "requires a native C compiler (CC or cc); run with --include-ignored"]
fn pragma_operator_layout_matches_native_c() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-x", "c", "-std=c11", "-Werror", "-fsyntax-only", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("native C compiler must be available");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SOURCE.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
