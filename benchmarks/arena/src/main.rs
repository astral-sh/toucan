//! Compare the owned expression AST with the opt-in arena prototype.
use std::hint::black_box;
#[cfg(not(feature = "allocation-counting"))]
use std::time::Instant;

use toucan_parser::driver::{Config, parse_expression, parse_expression_arena, with_parser_stack};

#[cfg(feature = "allocation-counting")]
mod allocations;
mod fixtures;

#[cfg(not(feature = "allocation-counting"))]
fn time<T>(mut operation: impl FnMut() -> T, iterations: usize) -> serde_json::Value {
    let mut parse_ns = Vec::with_capacity(iterations);
    let mut drop_ns = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        let parsed = black_box(operation());
        let parse = started.elapsed().as_nanos() as u64;
        let started = Instant::now();
        drop(parsed);
        let cleanup = started.elapsed().as_nanos() as u64;
        parse_ns.push(parse);
        drop_ns.push(cleanup);
    }
    serde_json::json!({"parse_ns": parse_ns, "drop_ns": drop_ns})
}

fn run<T>(operation: impl FnMut() -> T, iterations: usize) -> serde_json::Value {
    #[cfg(feature = "allocation-counting")]
    {
        let _ = iterations;
        let mut operation = operation;
        allocations::measure(|| drop(black_box(operation())))
    }
    #[cfg(not(feature = "allocation-counting"))]
    {
        time(operation, iterations)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 2 && args[1] == "list" {
        println!("{}", serde_json::to_string(&fixtures::cases())?);
        return Ok(());
    }
    if args.len() != 4 || !matches!(args[1].as_str(), "owned" | "arena" | "arena-owned") {
        return Err("usage: toucan-arena-benchmark owned|arena|arena-owned CASE ITERATIONS".into());
    }
    let iterations: usize = args[3].parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let (_, source) = fixtures::cases()
        .into_iter()
        .find(|(name, _)| *name == args[2])
        .ok_or("unknown case")?;
    let result = with_parser_stack(|| {
        let config = Config::with_gcc();
        let owned = || {
            parse_expression(&config, black_box(source.clone()), |name| name == "size_t").unwrap()
        };
        let arena = || {
            parse_expression_arena(&config, black_box(source.clone()), |name| name == "size_t")
                .unwrap()
        };
        // Compare the complete AST, including spans, before collecting samples.
        // Debug output is also hashed by the runner for cross-process comparison.
        let expected = owned();
        let actual = arena();
        let arena_nodes = actual.expression.iter().count();
        let native_arena_nodes = actual
            .expression
            .iter()
            .filter(|(_, node)| !matches!(node.node, toucan_parser::arena::Expression::Owned(_)))
            .count();
        let canonical = format!("{:?}", expected.expression);
        let converted = actual.expression.into_owned();
        assert_eq!(expected.expression, converted);
        assert_eq!(canonical, format!("{:?}", converted));
        drop(converted);
        drop(expected);
        // Warm the selected operation before allocator accounting or timing.
        let measurements = match args[1].as_str() {
            "owned" => {
                drop(owned());
                run(owned, iterations)
            }
            "arena" => {
                drop(arena());
                run(arena, iterations)
            }
            "arena-owned" => {
                let converted = || {
                    let parsed = arena();
                    (parsed.source, parsed.expression.into_owned())
                };
                drop(converted());
                run(converted, iterations)
            }
            _ => unreachable!(),
        };
        serde_json::json!({
            "engine": args[1],
            "case": args[2],
            "source": source,
            "canonical_ast": canonical,
            "arena_nodes": arena_nodes,
            "native_arena_nodes": native_arena_nodes,
            "allocation_counting": cfg!(feature = "allocation-counting"),
            "measurements": measurements,
        })
    })?;
    println!("{result}");
    Ok(())
}
