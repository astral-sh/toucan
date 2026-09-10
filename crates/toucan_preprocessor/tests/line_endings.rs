use std::path::Path;

use toucan_preprocessor::{Config, DocumentationOptions, Preprocessor};

const SOURCE: &str = concat!(
    "#define SIZE 7\n",
    "// ignored comment\n",
    "int values[SIZE];\n",
    "#define TOTAL 1 + \\\n",
    "2\n",
    "int total[TOTAL];\n",
    "int physical = __LINE__;\n",
    "??=define TRI 3 + ??/\n",
    "4\n",
    "int trigraph[TRI];\n",
    "#if SIZE != 7\n#error wrong macro value\n#endif\n",
    "#line 80 \"mapped.h\"\n",
    "int logical = __LINE__;\n",
);

fn sources() -> Vec<String> {
    let mut sources: Vec<_> = ["\n", "\r", "\r\n"]
        .map(|ending| SOURCE.replace('\n', ending))
        .into();
    sources.push(
        SOURCE
            .lines()
            .zip(["\r", "\r\n", "\n"].into_iter().cycle())
            .map(|(line, ending)| format!("{line}{ending}"))
            .collect(),
    );
    sources
}

fn compact(source: &str) -> String {
    source.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn physical_newlines_terminate_directives_comments_and_splices() {
    for source in sources() {
        let result = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("input.h"), &source)
            .unwrap();
        assert_eq!(
            compact(&result.source),
            "intvalues[7];inttotal[1+2];intphysical=7;inttrigraph[3+4];intlogical=80;",
            "{source:?}"
        );
        let physical = result
            .resolve_location(result.source.find("physical").unwrap())
            .unwrap();
        assert_eq!((physical.line, physical.column), (7, 5));
        let logical = result
            .resolve_location(result.source.find("logical").unwrap())
            .unwrap();
        assert_eq!((logical.line, logical.column), (80, 5));
        assert_eq!(logical.path.as_ref(), Path::new("mapped.h"));
    }
}

#[test]
fn physical_newline_conversion_preserves_original_documentation_offsets() {
    for ending in ["\n", "\r", "\r\n"] {
        let comment = format!("/** physical\\{ending} spelling */");
        let source = format!("{comment}{ending}#line 70 \"mapped.h\"{ending}  int value;{ending}");
        let result = Preprocessor::new(Config {
            documentation: Some(DocumentationOptions::default()),
            ..Config::default()
        })
        .preprocess_str(Path::new("input.h"), &source)
        .unwrap();
        let docs = result.documentation().unwrap();
        let (_, file) = docs.sources().next().unwrap();
        assert_eq!(file.comments()[0].text(), comment);
        assert_eq!(file.comments()[0].range(), &(0..comment.len()));
        let offset = result.source.find("value").unwrap();
        let origin = docs.resolve(offset).unwrap().invocation().unwrap();
        assert_eq!(origin.offset(), source.find("value").unwrap());
        assert_eq!(origin.line(), 4);
        let location = result.resolve_location(offset).unwrap();
        assert_eq!((location.line, location.column), (70, 7));

        let source = format!("// ignored{ending}#error failure{ending}");
        let error = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("error.h"), &source)
            .unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("failure"));
    }
}

#[test]
#[ignore = "requires a native C compiler (set CC)"]
fn native_physical_newlines_match() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for source in sources() {
        let mut child = Command::new(std::env::var_os("CC").unwrap_or_else(|| "cc".into()))
            .args(["-E", "-P", "-std=c11", "-trigraphs", "-x", "c", "-"])
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
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            toucan_test_support::compiler_acceptance(&output),
            Ok(true),
            "{source:?}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = Preprocessor::new(Config::default())
            .preprocess_str(Path::new("input.h"), &source)
            .unwrap();
        assert_eq!(
            compact(&result.source),
            compact(&String::from_utf8(output.stdout).unwrap())
        );
    }
}
