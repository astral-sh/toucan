use std::hint::black_box;

use toucan_benchmark::criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn bindings(c: &mut Criterion) {
    let mut group = c.benchmark_group("bindings");
    for workload in toucan_benchmark::workloads() {
        // Fail before timing if selection accidentally drops the public API.
        let bindings = workload.builder.clone().generate().unwrap();
        assert!(
            bindings.report().skipped_declarations.is_empty(),
            "{} skipped declarations: {:?}",
            workload.name,
            bindings.report().skipped_declarations
        );
        assert!(
            bindings.to_string().contains(workload.entry_point),
            "{} is missing {}",
            workload.name,
            workload.entry_point
        );
        drop(bindings);

        group.bench_function(BenchmarkId::from_parameter(&workload.name), |b| {
            b.iter(|| {
                black_box(workload.builder.clone())
                    .generate()
                    .unwrap()
                    .to_string()
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bindings);
criterion_main!(benches);
