extern crate toucan_parser;

use std::process::Command;
use toucan_parser::driver::{parse_preprocessed, parse_preprocessed_with_limits, Config};
use toucan_parser::limits::{ParseLimits, ResourceKind};

#[test]
fn limits_are_exact_and_independent_of_previous_invocations() {
    let source = "typedef int T; int f(int x) { T a[3] = {1,2,3}; return a[x] + x; }";
    let config = Config::with_gcc();
    let parsed = parse_preprocessed(&config, source.into()).unwrap();
    let stats = parsed.statistics;
    for (kind, limits) in [
        (
            ResourceKind::InputBytes,
            ParseLimits {
                max_input_bytes: source.len() - 1,
                ..ParseLimits::default()
            },
        ),
        (
            ResourceKind::Work,
            ParseLimits {
                max_work: stats.work - 1,
                ..ParseLimits::default()
            },
        ),
        (
            ResourceKind::BacktrackingSteps,
            ParseLimits {
                max_backtracking_steps: stats.maximum_backtracking_steps - 1,
                ..ParseLimits::default()
            },
        ),
        (
            ResourceKind::RuleDepth,
            ParseLimits {
                max_rule_depth: stats.maximum_rule_depth - 1,
                ..ParseLimits::default()
            },
        ),
        (
            ResourceKind::AstDepth,
            ParseLimits {
                max_ast_depth: stats.maximum_ast_depth - 1,
                ..ParseLimits::default()
            },
        ),
        (
            ResourceKind::CacheBytes,
            ParseLimits {
                max_cache_bytes: stats.cache_bytes - 1,
                ..ParseLimits::default()
            },
        ),
        (
            ResourceKind::MetadataEntries,
            ParseLimits {
                max_metadata_entries: stats.maximum_metadata_entries - 1,
                ..ParseLimits::default()
            },
        ),
    ] {
        let error = parse_preprocessed_with_limits(&config, source.into(), limits).unwrap_err();
        let resource = error.resource.as_ref().expect("resource error");
        assert_eq!(resource.kind, kind, "{}", error);
        assert!(error.expected.is_empty());
        assert!(source.is_char_boundary(error.offset));
        assert_eq!(resource.offset, error.offset);
        assert_eq!(
            parse_preprocessed(&config, source.into())
                .unwrap()
                .statistics,
            stats
        );
    }
    let exact = ParseLimits {
        max_input_bytes: source.len(),
        max_work: stats.work,
        max_backtracking_steps: stats.maximum_backtracking_steps,
        max_rule_depth: stats.maximum_rule_depth,
        max_ast_depth: stats.maximum_ast_depth,
        max_cache_bytes: stats.cache_bytes,
        max_metadata_entries: stats.maximum_metadata_entries,
    };
    let replay = parse_preprocessed_with_limits(&config, source.into(), exact).unwrap();
    assert!(parsed.ast().structural_eq(replay.ast()));
}

fn c11_minimum_nesting_cases() -> Vec<String> {
    let parentheses = format!(
        "int f(int x) {{ return {}x{}; }}",
        "(".repeat(63),
        ")".repeat(63)
    );
    let declarator = format!("int {}x{};", "(".repeat(63), ")".repeat(63));
    let blocks = format!(
        "int f(void) {{ {}int x = 0;{} return 0; }}",
        "{".repeat(126),
        "}".repeat(126)
    );
    let records = format!(
        "{}int x;{};",
        "struct S {".to_owned() + &"struct {".repeat(62),
        "} x;".repeat(62) + "}"
    );
    let pointer = format!("int {}p;", "*".repeat(12));
    vec![parentheses, declarator, blocks, records, pointer]
}

#[test]
fn default_limits_accept_c11_minimum_nesting() {
    for source in c11_minimum_nesting_cases() {
        parse_preprocessed(&Config::with_gcc(), source.clone())
            .unwrap_or_else(|error| panic!("{}\n{}", error, source));
    }
}

fn hostile(case: &str) -> String {
    let count = 10_000;
    match case {
        "unary" => format!("int f(int x) {{ return {}x; }}", "~".repeat(count)),
        "postfix" => format!("int f(int x) {{ return x{}; }}", "[0]".repeat(count)),
        "binary" => format!("int f(int x) {{ return {}x; }}", "x+".repeat(count)),
        "declarator" => format!("int {}x{};", "(".repeat(count), ")".repeat(count)),
        "control" => format!("int f(int x) {{ {}return x; }}", "if(x)".repeat(count)),
        "conditional" => format!("int f(int x) {{ return {}x; }}", "x?x:".repeat(count)),
        "near_match" => format!("typedef int T; int f(void) {{ {}", "(T(".repeat(count)),
        _ => panic!("unknown case"),
    }
}

#[test]
fn hostile_input_fails_in_worker_processes() {
    for case in [
        "unary",
        "postfix",
        "binary",
        "declarator",
        "control",
        "conditional",
        "near_match",
        "clone_drop",
    ] {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "resource_worker", "--nocapture"])
            .env("TOUCAN_PARSER_RESOURCE_CASE", case)
            .status()
            .unwrap();
        assert!(status.success(), "worker failed for {}: {}", case, status);
    }
}

#[test]
fn resource_worker() {
    let Ok(case) = std::env::var("TOUCAN_PARSER_RESOURCE_CASE") else {
        return;
    };
    // This is the ordinary Rust worker stack; parsing runs on its own
    // bounded stack. Successful AST cloning and dropping run on this caller.
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            if case == "clone_drop" {
                for source in [
                    format!("int f(int x) {{ return {}x; }}", "x+".repeat(120)),
                    format!(
                        "int f(void) {{ {}int x;{} return 0; }}",
                        "{".repeat(126),
                        "}".repeat(126)
                    ),
                ] {
                    let parsed = parse_preprocessed(&Config::with_gcc(), source).unwrap();
                    drop(parsed.ast().to_owned());
                    drop(parsed);
                }
            } else {
                let error = parse_preprocessed(&Config::with_gcc(), hostile(&case)).unwrap_err();
                assert!(error.resource.is_some(), "{}", error);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn resource_errors_keep_line_marker_locations_and_concurrent_state() {
    let source = "# 90 \"nested.h\"\nint f(int x) { return x + x; }";
    let work = parse_preprocessed(&Config::with_gcc(), source.into())
        .unwrap()
        .statistics
        .work;
    let error = parse_preprocessed_with_limits(
        &Config::with_gcc(),
        source.into(),
        ParseLimits {
            max_work: work / 2,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("nested.h"), "{}", error);
    std::thread::scope(|scope| {
        let workers = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    let parsed = parse_preprocessed(&Config::with_gcc(), source.into()).unwrap();
                    parsed.statistics
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            assert_eq!(worker.join().unwrap().work, work);
        }
    });
}

#[test]
fn sessions_reuse_the_stack_and_propagate_errors_and_panics() {
    use toucan_parser::driver::with_parser_stack;
    let caller = std::thread::current().id();
    let first = with_parser_stack(|| {
        let worker = std::thread::current().id();
        assert_ne!(caller, worker);
        assert_eq!(
            with_parser_stack(|| std::thread::current().id()).unwrap(),
            worker
        );
        let error = parse_preprocessed_with_limits(
            &Config::with_gcc(),
            "int x;".into(),
            ParseLimits {
                max_work: 1,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
        parse_preprocessed(&Config::with_gcc(), "int x;".into())
            .unwrap()
            .statistics
    })
    .unwrap();
    assert!(std::panic::catch_unwind(|| with_parser_stack(|| panic!("session panic"))).is_err());
    assert_eq!(
        with_parser_stack(|| parse_preprocessed(&Config::with_gcc(), "int x;".into())
            .unwrap()
            .statistics)
        .unwrap(),
        first
    );
}

#[test]
fn exhaustion_is_terminal_through_optional_and_backtracking_rules() {
    use toucan_parser::driver::with_parser_stack;
    let source = "typedef int T; int f(int T); T x; int g(void) { T a = 1; if (a) { return (T)(a + 1); } return 0; }";
    with_parser_stack(|| {
        let total = parse_preprocessed(&Config::with_gcc(), source.into())
            .unwrap()
            .statistics
            .work;
        for work in (0..total).step_by((total / 200).max(1) as usize) {
            let error = parse_preprocessed_with_limits(
                &Config::with_gcc(),
                source.into(),
                ParseLimits {
                    max_work: work,
                    ..ParseLimits::default()
                },
            )
            .unwrap_err();
            assert_eq!(error.resource.as_ref().unwrap().kind, ResourceKind::Work);
            assert_eq!(
                parse_preprocessed(&Config::with_gcc(), source.into())
                    .unwrap()
                    .statistics
                    .work,
                total
            );
        }
    })
    .unwrap();
}

#[test]
fn malformed_prefixes_do_not_backtrack_and_padding_does_not_reset_limits() {
    let config = Config::with_gcc();
    let malformed = format!("signed long size[{}@];", "+".repeat(32));
    let baseline = parse_preprocessed(&config, malformed.clone()).unwrap_err();
    assert!(baseline.resource.is_none(), "{}", baseline);
    for prefix in [String::new(), " ".repeat(1_000_000)] {
        let error = parse_preprocessed(&config, prefix + &malformed).unwrap_err();
        assert!(error.resource.is_none(), "{}", error);
        assert_eq!(
            error.statistics.maximum_backtracking_steps,
            baseline.statistics.maximum_backtracking_steps
        );
        assert!(error.statistics.maximum_backtracking_steps < 100);
    }
}

#[test]
#[ignore = "requires native GCC/Clang; run with --include-ignored"]
fn minimum_nesting_matches_native_c11_compilers() {
    use std::io::Write;
    use std::process::Stdio;
    for compiler in [
        std::env::var("TOUCAN_GCC").unwrap_or_else(|_| "gcc".into()),
        "clang".into(),
    ] {
        for source in c11_minimum_nesting_cases() {
            let mut child = Command::new(&compiler)
                .args([
                    "-std=c11",
                    "-pedantic-errors",
                    "-fsyntax-only",
                    "-x",
                    "c",
                    "-",
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
                .write_all(format!("{source}\n").as_bytes())
                .unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}: {}\n{}",
                compiler,
                String::from_utf8_lossy(&result.stderr),
                source
            );
            parse_preprocessed(&Config::with_gcc(), source).unwrap();
        }
    }
}

#[test]
fn nested_introspection_respects_recursion_and_owned_tree_limits() {
    let source = format!(
        "int x={}0{};",
        "__builtin_choose_expr(1,".repeat(2000),
        ",0)".repeat(2000)
    );
    let error = parse_preprocessed(&Config::with_gcc(), source).unwrap_err();
    assert!(matches!(
        error.resource.unwrap().kind,
        ResourceKind::RuleDepth | ResourceKind::AstDepth
    ));
}

#[test]
fn inferred_initializers_use_the_same_deterministic_work_and_depth_limits() {
    let source = "int f(void){__auto_type x=({__auto_type y=1;y;});return x;}";
    let config = Config::with_gcc();
    let parsed = parse_preprocessed(&config, source.to_owned()).unwrap();
    let error = parse_preprocessed_with_limits(
        &config,
        source.to_owned(),
        ParseLimits {
            max_work: parsed.statistics.work - 1,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
    let nested = format!(
        "void f(void){{{}1{};}}",
        "__auto_type x=({".repeat(2000),
        ";x;})".repeat(2000)
    );
    let error = parse_preprocessed(&config, nested).unwrap_err();
    assert_eq!(error.resource.unwrap().kind, ResourceKind::RuleDepth);
}
