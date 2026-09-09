use std::path::Path;

use toucan_preprocessor::{Config, Preprocessor};

const SOURCE: &str = r#"
#define S(x) #x
#define E(x) S(x)
#define EMPTY
#define INFIX(x) a x+b
#define TAIL(x) a x
#define PASTED(x) a x##x+b
#define PASTED_TAIL(x) a x##x
#define LEFT(x) a x##b
#define RIGHT(x) a b##x
#define PASTE_SPACING(x) a+x ## b
#define FIRST(x) x+b
#define G(v) v
#define CALL(x) G x(1)
#define ALL(x) x x
#define V(...) a __VA_ARGS__+b
#define GROUP(x,y) a(x y)+b
#define NESTED(x) G(a x)
#define TAIL_CALL(x) G x
E(INFIX())
E(INFIX(EMPTY))
E(TAIL()+b)
E(PASTED())
E(PASTED_TAIL()+b)
E(LEFT())
E(RIGHT())
E(PASTE_SPACING())
E(a+FIRST())
E(CALL())
E(a+ALL()+b)
E(TAIL())
E(INFIX(v))
E(V())
E(GROUP(,))
E(NESTED()+b)
E(TAIL_CALL()+b)
E(TAIL_CALL()(1))
"#;

const EXPECTED: &str = concat!(
    "\"a +b\"\n",
    "\"a +b\"\n",
    "\"a +b\"\n",
    "\"a +b\"\n",
    "\"a +b\"\n",
    "\"a b\"\n",
    "\"a b\"\n",
    "\"a+b\"\n",
    "\"a++b\"\n",
    "\"1\"\n",
    "\"a+ +b\"\n",
    "\"a\"\n",
    "\"a v+b\"\n",
    "\"a +b\"\n",
    "\"a( )+b\"\n",
    "\"a+b\"\n",
    "\"G +b\"\n",
    "\"1\"\n",
);

#[test]
fn empty_parameters_preserve_replacement_whitespace() {
    let output = Preprocessor::new(Config::default())
        .preprocess_str(Path::new("empty_parameters.h"), SOURCE)
        .unwrap();
    assert_eq!(
        output.source,
        format!("{}\n", EXPECTED.lines().collect::<Vec<_>>().join(" "))
    );
}

#[test]
#[ignore = "requires a native C compiler (set CC)"]
fn native_empty_parameter_whitespace() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
        .args(["-E", "-P", "-std=gnu11", "-x", "c", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the native test requires a C compiler (set CC)");
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
    let expected = EXPECTED.lines().collect::<Vec<_>>();
    let source = String::from_utf8(output.stdout).unwrap();
    let actual = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
