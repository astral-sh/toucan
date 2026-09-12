//! Measure parsing and analysis, with and without retaining checked code.

use std::hint::black_box;

use toucan_benchmark::criterion::{BatchSize, BenchmarkId, Criterion, Throughput};
use toucan_semantic::{AnalysisOptions, analyze_with_options};
use toucan_target::Target;

fn semantic(c: &mut Criterion) {
    let mut group = c.benchmark_group("semantic");
    for workload in toucan_benchmark::parser_workloads() {
        let baseline = analyze_with_options(
            &workload.source,
            Target::X86_64UnknownLinuxGnu,
            &AnalysisOptions::default(),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", workload.name));
        assert!(
            !baseline.unit().declarations.is_empty(),
            "{}: empty analysis",
            workload.name
        );
        let declarations = serde_json::to_vec(baseline.unit()).unwrap();
        drop(baseline);

        group.throughput(Throughput::Bytes(workload.source.len() as u64));
        for retain_code in [false, true] {
            let options = AnalysisOptions {
                retain_code,
                ..Default::default()
            };
            let mode = if retain_code { "retained" } else { "normal" };
            let analysis =
                analyze_with_options(&workload.source, Target::X86_64UnknownLinuxGnu, &options)
                    .unwrap_or_else(|error| panic!("{}: {mode}: {error}", workload.name));
            assert_eq!(analysis.checked().is_some(), retain_code);
            assert_eq!(serde_json::to_vec(analysis.unit()).unwrap(), declarations);
            drop(analysis);

            group.bench_function(BenchmarkId::new(mode, &workload.name), |b| {
                b.iter_batched(
                    || (),
                    |()| {
                        analyze_with_options(
                            black_box(&workload.source),
                            Target::X86_64UnknownLinuxGnu,
                            black_box(&options),
                        )
                        .unwrap()
                    },
                    BatchSize::PerIteration,
                );
            });
        }
    }
    group.finish();
}

mod harness {
    use toucan_benchmark::criterion::{criterion_group, criterion_main};

    criterion_group!(benches, super::semantic);
    criterion_main!(benches);

    pub(super) fn run() {
        main();
    }
}

fn main() {
    toucan_parser::driver::with_parser_stack(harness::run)
        .expect("cannot create semantic benchmark stack");
}
