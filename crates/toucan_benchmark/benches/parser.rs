//! Compare AST construction from identical, already-preprocessed C sources.

use std::hint::black_box;

use toucan_benchmark::criterion::{BatchSize, BenchmarkId, Criterion, Throughput};

fn parser(c: &mut Criterion) {
    let toucan_config = toucan_parser::driver::Config::with_gcc();
    let reference_config = lang_c_reference::driver::Config::with_gcc();
    let mut group = c.benchmark_group("parser");

    for workload in toucan_benchmark::parser_workloads() {
        // The AST types have diverged, but their inherited tree printers agree
        // on the shared syntax exercised here. Compare the complete printed
        // trees before timing; matching declaration counts alone is too weak.
        let toucan =
            toucan_parser::driver::parse_preprocessed(&toucan_config, workload.source.clone())
                .unwrap_or_else(|error| panic!("{}: Toucan: {error}", workload.name));
        let reference = lang_c_reference::driver::parse_preprocessed(
            &reference_config,
            workload.source.clone(),
        )
        .unwrap_or_else(|error| panic!("{}: lang-c: {error}", workload.name));
        assert!(!toucan.unit.0.is_empty(), "{}: empty AST", workload.name);
        if workload.name.contains("-adler32-") {
            assert!(
                toucan.unit.0.iter().any(|declaration| matches!(
                    declaration.node,
                    toucan_parser::ast::ExternalDeclaration::FunctionDefinition(_)
                )),
                "{}: missing function bodies",
                workload.name,
            );
        }

        let mut toucan_tree = String::new();
        let mut reference_tree = String::new();
        toucan_parser::visit::Visit::visit_translation_unit(
            &mut toucan_parser::print::Printer::new(&mut toucan_tree),
            &toucan.unit,
            &toucan.arena,
        );
        lang_c_reference::visit::Visit::visit_translation_unit(
            &mut lang_c_reference::print::Printer::new(&mut reference_tree),
            &reference.unit,
        );
        assert!(
            toucan_tree == reference_tree,
            "{}: printed ASTs differ; investigate before comparing performance",
            workload.name,
        );
        drop((toucan, reference, toucan_tree, reference_tree));

        group.throughput(Throughput::Bytes(workload.source.len() as u64));
        group.bench_function(BenchmarkId::new("toucan", &workload.name), |b| {
            b.iter_batched(
                || workload.source.clone(),
                |source| {
                    toucan_parser::driver::parse_preprocessed(
                        black_box(&toucan_config),
                        black_box(source),
                    )
                    .unwrap()
                },
                BatchSize::PerIteration,
            );
        });
        group.bench_function(BenchmarkId::new("lang-c-0.15.1", &workload.name), |b| {
            b.iter_batched(
                || workload.source.clone(),
                |source| {
                    lang_c_reference::driver::parse_preprocessed(
                        black_box(&reference_config),
                        black_box(source),
                    )
                    .unwrap()
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

mod harness {
    use toucan_benchmark::criterion::{criterion_group, criterion_main};

    criterion_group!(benches, super::parser);
    criterion_main!(benches);

    pub(super) fn run() {
        main();
    }
}

fn main() {
    // Both engines use the same worker. Creating and joining it is outside all
    // measurements; Toucan's nested public API calls reuse it with default limits.
    toucan_parser::driver::with_parser_stack(harness::run)
        .expect("cannot create parser benchmark stack");
}
