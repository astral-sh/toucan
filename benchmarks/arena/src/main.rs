//! Compare complete default-parser and frontend pipelines across two revisions.
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::path::Path;
#[cfg(not(feature = "allocation-counting"))]
use std::time::Instant;

use toucan_parser::driver::{Config, parse_preprocessed, with_parser_stack};

#[cfg(feature = "allocation-counting")]
mod allocations;
mod spans;

/// Adapt the inherited syntax printer to an output file without a second tree buffer.
struct TreeWriter(BufWriter<std::fs::File>);

impl std::fmt::Write for TreeWriter {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0
            .write_all(text.as_bytes())
            .map_err(|_| std::fmt::Error)
    }
}

fn parser_tree(parsed: &toucan_parser::driver::Parse, path: &Path) {
    let mut writer = TreeWriter(BufWriter::new(std::fs::File::create(path).unwrap()));
    #[cfg(not(feature = "full-arena"))]
    toucan_parser::visit::Visit::visit_translation_unit(
        &mut toucan_parser::print::Printer::new(&mut writer),
        &parsed.unit,
    );
    #[cfg(feature = "full-arena")]
    toucan_parser::visit::Visit::visit_translation_unit(
        &mut toucan_parser::print::Printer::new(&mut writer),
        &parsed.unit,
        &parsed.arena,
    );
    writer.0.flush().unwrap();
    spans::capture(parsed, &path.with_extension("spans"));
}

fn reference_tree(parsed: &lang_c_reference::driver::Parse, path: &Path) {
    let mut writer = TreeWriter(BufWriter::new(std::fs::File::create(path).unwrap()));
    lang_c_reference::visit::Visit::visit_translation_unit(
        &mut lang_c_reference::print::Printer::new(&mut writer),
        &parsed.unit,
    );
    writer.0.flush().unwrap();
}

fn semantic_json(analysis: &toucan_semantic::Analysis, path: &Path) {
    let mut writer = BufWriter::new(std::fs::File::create(path).unwrap());
    serde_json::to_writer(&mut writer, analysis).unwrap();
    writer.flush().unwrap();
}

fn run<P, T>(
    mode: &str,
    mut setup: impl FnMut() -> P,
    mut operation: impl FnMut(P) -> T,
    capture: impl FnOnce(&T, &Path),
    iterations: usize,
    output: &Path,
) -> serde_json::Value {
    #[cfg(feature = "allocation-counting")]
    let _ = iterations;
    match mode {
        "capture" => {
            let result = operation(setup());
            capture(&result, output);
            serde_json::json!({"output_bytes": output.metadata().unwrap().len()})
        }
        "rss" => {
            drop(black_box(operation(setup())));
            serde_json::json!({})
        }
        "allocations" => {
            #[cfg(feature = "allocation-counting")]
            {
                drop(operation(setup()));
                allocations::measure(setup, operation)
            }
            #[cfg(not(feature = "allocation-counting"))]
            panic!("allocations mode requires the allocation-counting build")
        }
        "timing" => {
            #[cfg(feature = "allocation-counting")]
            panic!("timing mode requires an uninstrumented build");
            #[cfg(not(feature = "allocation-counting"))]
            {
                drop(operation(setup()));
                let mut operation_ns = Vec::with_capacity(iterations);
                let mut drop_ns = Vec::with_capacity(iterations);
                for _ in 0..iterations {
                    let input = setup();
                    let started = Instant::now();
                    let parsed = black_box(operation(black_box(input)));
                    let operation = started.elapsed().as_nanos() as u64;
                    let started = Instant::now();
                    drop(parsed);
                    let cleanup = started.elapsed().as_nanos() as u64;
                    operation_ns.push(operation);
                    drop_ns.push(cleanup);
                }
                serde_json::json!({"operation_ns": operation_ns, "drop_ns": drop_ns})
            }
        }
        _ => panic!("unknown measurement mode"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 2 && args[1] == "prepare" {
        let workloads = toucan_benchmark::parser_workloads();
        println!(
            "{}",
            serde_json::json!(
                workloads
                    .iter()
                    .map(|w| (&w.name, w.source.len()))
                    .collect::<Vec<_>>()
            )
        );
        return Ok(());
    }
    if args.len() != 6 {
        return Err("usage: toucan-arena-benchmark MODE ENGINE INPUT ITERATIONS OUTPUT\nMODE: capture|timing|allocations|rss; ENGINE: parser|lang-c|analyze|retained|builder".into());
    }
    let iterations: usize = args[4].parse()?;
    if iterations == 0 {
        return Err("iterations must be positive".into());
    }
    let output = Path::new(&args[5]);
    let result = with_parser_stack(|| {
        let source = (args[2] != "builder")
            .then(|| std::fs::read_to_string(&args[3]).expect("cannot read frozen input"));
        let measurements = match args[2].as_str() {
            "parser" => {
                let source = source.as_ref().unwrap();
                let config = Config::with_gcc();
                run(
                    &args[1],
                    || source.clone(),
                    |input| {
                        parse_preprocessed(&config, input).unwrap_or_else(|error| panic!("{error}"))
                    },
                    parser_tree,
                    iterations,
                    output,
                )
            }
            "lang-c" => {
                let source = source.as_ref().unwrap();
                let config = lang_c_reference::driver::Config::with_gcc();
                run(
                    &args[1],
                    || source.clone(),
                    |input| {
                        lang_c_reference::driver::parse_preprocessed(&config, input)
                            .unwrap_or_else(|error| panic!("{error}"))
                    },
                    reference_tree,
                    iterations,
                    output,
                )
            }
            "analyze" | "retained" => {
                let source = source.as_ref().unwrap();
                let options = toucan_semantic::AnalysisOptions {
                    retain_code: args[2] == "retained",
                    ..Default::default()
                };
                run(
                    &args[1],
                    || (),
                    |()| {
                        toucan_semantic::analyze_with_options(
                            source,
                            toucan_target::Target::X86_64UnknownLinuxGnu,
                            &options,
                        )
                        .unwrap_or_else(|error| panic!("{error}"))
                    },
                    semantic_json,
                    iterations,
                    output,
                )
            }
            "builder" => {
                let workload = toucan_benchmark::workloads()
                    .into_iter()
                    .find(|workload| workload.name == args[3])
                    .expect("unknown Builder workload");
                run(
                    &args[1],
                    || (),
                    |()| {
                        let bindings = workload
                            .builder
                            .clone()
                            .generate()
                            .unwrap_or_else(|error| panic!("{error}"));
                        if args[1] == "capture" {
                            assert!(
                                bindings.report().skipped_declarations.is_empty(),
                                "skipped declarations: {:?}",
                                bindings.report().skipped_declarations
                            );
                            let report =
                                std::fs::File::create(output.with_extension("report.json"))
                                    .unwrap();
                            serde_json::to_writer(report, bindings.report()).unwrap();
                        }
                        let source = bindings.to_string();
                        if args[1] == "capture" {
                            assert!(source.contains(workload.entry_point));
                        }
                        source
                    },
                    |source, path| std::fs::write(path, source).unwrap(),
                    iterations,
                    output,
                )
            }
            _ => panic!("unknown benchmark engine"),
        };
        serde_json::json!({
            "mode": args[1],
            "engine": args[2],
            "input": args[3],
            "source_bytes": source.as_ref().map(String::len),
            "allocation_counting": cfg!(feature = "allocation-counting"),
            "full_arena": cfg!(feature = "full-arena"),
            "measurements": measurements,
        })
    })?;
    println!("{result}");
    Ok(())
}
