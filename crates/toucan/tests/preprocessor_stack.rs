use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};

use toucan::{
    FeatureQueries, FeatureQuery, FeatureQueryProvider, Preprocessor, PreprocessorConfig,
    QueryDialect, semantic, with_preprocessor_stack,
};

fn nested_macro(depth: usize) -> String {
    format!("{}0{}", "F(".repeat(depth), ")".repeat(depth))
}

#[test]
fn combined_recursion_and_final_macro_queries_work_on_small_caller_stacks() {
    thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            for (includes, expansions) in [(64, 128), (256, 256)] {
                let mut config = PreprocessorConfig {
                    max_include_depth: includes,
                    max_expansion_depth: expansions,
                    ..Default::default()
                };
                for index in 0..includes - 1 {
                    let source = if index == includes - 2 {
                        format!(
                            "#define F(x) x\n#define VALUE {}\n{}\n",
                            nested_macro(expansions - 2),
                            nested_macro(expansions - 1),
                        )
                    } else {
                        format!("#include <h{}.h>\n", index + 1)
                    };
                    config.virtual_headers.insert(format!("h{index}.h"), source);
                }
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("main.h");
                let source = "#include <h0.h>\n";
                std::fs::write(&path, source).unwrap();
                let mut preprocessor = Preprocessor::new(config);
                for input in 0..3 {
                    let output = match input {
                        0 => preprocessor.preprocess_str(&path, source),
                        1 => preprocessor.preprocess(&path),
                        _ => preprocessor.preprocess_files(std::slice::from_ref(&path)),
                    }
                    .unwrap();
                    assert_eq!(output.source.trim(), "0");
                    assert_eq!(
                        output.expand_object_macro("VALUE").unwrap().as_deref(),
                        Some("0")
                    );
                }
                let error = preprocessor
                    .preprocess_str(
                        &path,
                        &format!("#define F(x) x\n{}", nested_macro(expansions)),
                    )
                    .unwrap_err();
                assert!(error.message.contains("depth limit"), "{error}");
                assert_eq!(error.path, path);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn unsupported_recursion_settings_fail_before_input_processing() {
    for (includes, expansions) in [(257, 128), (64, 257), (usize::MAX, usize::MAX)] {
        let mut preprocessor = Preprocessor::new(PreprocessorConfig {
            max_include_depth: includes,
            max_expansion_depth: expansions,
            allow_filesystem: false,
            ..Default::default()
        });
        let path = Path::new("missing.h");
        for error in [
            preprocessor.preprocess_str(path, "").unwrap_err(),
            preprocessor.preprocess(path).unwrap_err(),
            preprocessor
                .preprocess_files(&[path.to_owned()])
                .unwrap_err(),
        ] {
            assert!(
                error.message.contains("supported maximum of 256"),
                "{error}"
            );
            assert_eq!(error.path, path);
        }
    }
}

#[derive(Debug)]
struct ThreadRecorder(Mutex<Vec<ThreadId>>);

impl FeatureQueryProvider for ThreadRecorder {
    fn query(&self, _: FeatureQuery, _: Option<&str>, _: &str) -> u64 {
        self.0.lock().unwrap().push(thread::current().id());
        1
    }
}

#[test]
fn preprocessing_and_macro_queries_reuse_parser_sessions() {
    let recorder = Arc::new(ThreadRecorder(Mutex::new(Vec::new())));
    let caller = thread::current().id();
    semantic::with_parser_stack(|| {
        let worker = thread::current().id();
        assert_ne!(worker, caller);
        assert_eq!(
            with_preprocessor_stack(|| thread::current().id()).unwrap(),
            worker
        );
        let mut preprocessor = Preprocessor::new(PreprocessorConfig {
            feature_queries: Some(FeatureQueries::new(QueryDialect::Clang, recorder.clone())),
            ..Default::default()
        });
        let output = preprocessor
            .preprocess_str(
                Path::new("query.h"),
                "__has_builtin(foo)\n#define VALUE __has_builtin(bar)\n",
            )
            .unwrap();
        assert_eq!(output.source.trim(), "1");
        assert_eq!(
            output.expand_object_macro("VALUE").unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(*recorder.0.lock().unwrap(), [worker, worker]);
    })
    .unwrap();
}
