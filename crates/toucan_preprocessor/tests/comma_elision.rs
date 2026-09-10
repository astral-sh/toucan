use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const SOURCE: &str = r#"
#define V(...) pre,##__VA_ARGS__
#define FIXED(x,...) x,##__VA_ARGS__
#define NAMED(args...) pre,##args
#define ALIAS V
#define CALL(x) V(x)
#define MUTUAL(...) other,V(__VA_ARGS__)
#define ID(x) x
#define EMPTY
#define VALUE 9
V()
V( )
V(,)
FIXED(0)
FIXED(0,)
V(EMPTY)
V(V(1))
V(ALIAS(1))
V(CALL(1))
V(MUTUAL(1))
ID(V(V(1)))
FIXED(0,FIXED(1,2))
NAMED(NAMED(1))
V(VALUE)
#define BOTH(...) __VA_ARGS__,##__VA_ARGS__
BOTH(__COUNTER__)
#define LAST(...) pre,##__VA_ARGS__,__VA_ARGS__
LAST(__COUNTER__)
#define DUP(...) pre,##__VA_ARGS__,##__VA_ARGS__
DUP(__COUNTER__)
__COUNTER__
"#;

const EXPECTED: &str = concat!(
    "pre pre pre,, 0 0, pre, ",
    "pre,V(1) pre,V(1) pre,V(1) pre,other,V(1) pre,V(1) ",
    "0,FIXED(1,2) pre,NAMED(1) pre,9 ",
    "0,1 pre,3,2 pre,4,5 6",
);

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn comma_elision_rescans_raw_variadic_arguments() {
    let result = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("input.h"), SOURCE)
        .unwrap();
    assert_eq!(compact(&result.source), compact(EXPECTED));
}

#[test]
#[ignore = "requires a native C compiler (set CC)"]
fn native_comma_elision_matches() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-E", "-P", "-std=gnu11", "-x", "c", "-"])
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
    assert_eq!(
        toucan_test_support::compiler_acceptance(&output),
        Ok(true),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        compact(&String::from_utf8(output.stdout).unwrap()),
        compact(EXPECTED)
    );
}
