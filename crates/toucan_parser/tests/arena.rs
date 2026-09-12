extern crate toucan_parser;

use std::fs;
use std::path::Path;
use std::process::Command;
use toucan_parser::driver::{
    parse_expression, parse_expression_arena, parse_expression_arena_with_limits,
    with_parser_stack, Config, Flavor,
};
use toucan_parser::limits::{ParseLimits, ResourceKind};

fn compare(source: &str, config: &Config, typedefs: &[&str]) {
    let owned = parse_expression(config, source.into(), |name| typedefs.contains(&name));
    let arena = parse_expression_arena(config, source.into(), |name| typedefs.contains(&name));
    match (owned, arena) {
        (Ok(owned), Ok(arena)) => {
            assert_eq!(arena.source, source);
            assert_eq!(
                owned.statistics.maximum_ast_depth, arena.statistics.maximum_ast_depth,
                "{}",
                source
            );
            let converted = arena.expression.into_owned();
            assert_eq!(owned.expression, converted, "{}", source);
            // Include every span, since Span::none() has wildcard equality.
            assert_eq!(
                format!("{:?}", owned.expression),
                format!("{:?}", converted)
            );
        }
        (Err(owned), Err(arena)) => {
            assert_eq!(owned.offset, arena.offset, "{}", source);
            assert_eq!(owned.expected, arena.expected, "{}", source);
            assert_eq!(owned.resource, arena.resource, "{}", source);
        }
        results => panic!("different acceptance for {}: {:?}", source, results),
    }
}

#[test]
fn expression_reference_cases_round_trip_with_exact_spans() {
    with_parser_stack(|| {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("reftests");
        let mut count = 0;
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if !path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("expression-")
            {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let mut config = Config::with_gcc();
            config.flavor = Flavor::StdC11;
            config.gnu_keywords = false;
            let mut source = String::new();
            let mut typedefs = Vec::new();
            for line in text.split("/*===").next().unwrap().lines() {
                match line.trim() {
                    "#pragma gnu" => config = Config::with_gcc(),
                    "#pragma clang" => config = Config::with_clang(),
                    line if line.starts_with("#pragma typedef ") => {
                        typedefs.push(line.trim_start_matches("#pragma typedef "));
                    }
                    line if line.starts_with("#pragma") => panic!("unknown pragma: {}", line),
                    line => {
                        source.push_str(line);
                        source.push('\n');
                    }
                }
            }
            compare(&source, &config, &typedefs);
            count += 1;
        }
        assert!(count > 0);
    })
    .unwrap();
}

#[test]
fn operators_owned_leaves_and_errors_share_the_grammar() {
    with_parser_stack(|| {
        for source in [
            "  a + b * c - d / e  ",
            "a = b += c ? d + e : f * g, h, i ?: j",
            "a ? b ? c : d : e ? f : g",
            "(a + b) * -c + array[i]++ + f(1, 2)",
            "(T)1 + sizeof(T[3]) + _Alignof(T)",
            "({ int T; (T)++; }) + (T)1",
            "_Generic(a, int: b + c, default: d) + __builtin_choose_expr(1, 2, 3)",
            "sizeof((struct { int x; }){ .x = 1 }) + 2",
            "\"a\" \"b\"",
            "",
            "(1",
            "a +",
            "a ? b",
            "a ? :",
            "a + b = c",
            "(T)a = b",
            "a; int injected",
            "a b",
            "1\n#define X 2",
            "# 20 \"injected\"\n1",
        ] {
            for config in [Config::with_gcc(), Config::with_clang()] {
                compare(source, &config, &["T"]);
            }
        }
    })
    .unwrap();
}

#[test]
fn arena_limits_are_exact_and_exhaustion_is_terminal() {
    with_parser_stack(|| {
        let config = Config::with_gcc();
        let source = "a = b ? (T)1 + c * d : e + f, g + h";
        let parsed = parse_expression_arena(&config, source.into(), |name| name == "T").unwrap();
        let stats = parsed.statistics;
        let exact = ParseLimits {
            max_input_bytes: source.len(),
            max_work: stats.work,
            max_backtracking_steps: stats.maximum_backtracking_steps,
            max_rule_depth: stats.maximum_rule_depth,
            max_ast_depth: stats.maximum_ast_depth,
            max_cache_bytes: stats.cache_bytes,
            max_metadata_entries: stats.maximum_metadata_entries,
        };
        let replay =
            parse_expression_arena_with_limits(&config, source.into(), |name| name == "T", exact)
                .unwrap();
        assert_eq!(
            parsed.expression.into_owned(),
            replay.expression.into_owned()
        );
        for (kind, limits) in [
            (
                ResourceKind::InputBytes,
                ParseLimits {
                    max_input_bytes: source.len() - 1,
                    ..exact
                },
            ),
            (
                ResourceKind::Work,
                ParseLimits {
                    max_work: stats.work - 1,
                    ..exact
                },
            ),
            (
                ResourceKind::BacktrackingSteps,
                ParseLimits {
                    max_backtracking_steps: stats.maximum_backtracking_steps - 1,
                    ..exact
                },
            ),
            (
                ResourceKind::RuleDepth,
                ParseLimits {
                    max_rule_depth: stats.maximum_rule_depth - 1,
                    ..exact
                },
            ),
            (
                ResourceKind::AstDepth,
                ParseLimits {
                    max_ast_depth: stats.maximum_ast_depth - 1,
                    ..exact
                },
            ),
            (
                ResourceKind::CacheBytes,
                ParseLimits {
                    max_cache_bytes: stats.cache_bytes - 1,
                    ..exact
                },
            ),
            (
                ResourceKind::MetadataEntries,
                ParseLimits {
                    max_metadata_entries: stats.maximum_metadata_entries - 1,
                    ..exact
                },
            ),
        ] {
            let error = parse_expression_arena_with_limits(
                &config,
                source.into(),
                |name| name == "T",
                limits,
            )
            .unwrap_err();
            let resource = error.resource.unwrap();
            assert_eq!(resource.kind, kind);
            assert!(source.is_char_boundary(resource.offset));
        }
        for work in (0..stats.work).step_by((stats.work / 100).max(1) as usize) {
            let error = parse_expression_arena_with_limits(
                &config,
                source.into(),
                |name| name == "T",
                ParseLimits {
                    max_work: work,
                    ..exact
                },
            )
            .unwrap_err();
            assert_eq!(error.resource.unwrap().kind, ResourceKind::Work);
        }
    })
    .unwrap();
}

#[test]
fn hostile_inputs_and_conversion_fit_a_small_caller_stack() {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "arena_resource_worker", "--nocapture"])
        .env("TOUCAN_ARENA_RESOURCE_WORKER", "1")
        .status()
        .unwrap();
    assert!(status.success(), "arena resource worker: {}", status);
}

#[test]
fn arena_resource_worker() {
    if std::env::var_os("TOUCAN_ARENA_RESOURCE_WORKER").is_none() {
        return;
    }
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let config = Config::with_gcc();
            for source in [
                "a+".repeat(10_000) + "a",
                "a=".repeat(10_000) + "a",
                "a?b:".repeat(10_000) + "a",
                "-".repeat(10_000) + "a",
            ] {
                let error = parse_expression_arena(&config, source, |_| false).unwrap_err();
                assert!(error.resource.is_some());
            }
            for source in ["a+".repeat(160) + "a", "a,".repeat(10_000) + "a"] {
                let parsed = parse_expression_arena(&config, source, |_| false).unwrap();
                drop(parsed.source);
                let expression = parsed.expression.into_owned();
                drop(expression.clone());
                drop(expression);
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
