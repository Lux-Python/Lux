#![allow(clippy::pedantic, clippy::nursery)]

use std::hint::black_box;
use std::str::FromStr;
use std::time::Duration;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use lux_core::types::Requirement;
use lux_resolver::{MemoryDependencyProvider, PubGrubSolver};

fn build_tiered_graph(tiers: usize, width: usize) -> (MemoryDependencyProvider, Vec<Requirement>) {
    let mut provider = MemoryDependencyProvider::new();

    for t in 0..tiers {
        for w in 0..width {
            let pkg_name = format!("pkg-{t}-{w}");
            provider.add_package(&pkg_name, &["1.0.0", "2.0.0"]);

            if t + 1 < tiers {
                // Diamond convergence to next tier
                let dep1 = format!("pkg-{}-{w} >= 1.0.0", t + 1);
                let dep2 = format!("pkg-{}-{} >= 1.0.0", t + 1, (w + 1) % width);
                provider.add_dependency(&pkg_name, "1.0.0", &dep1);
                provider.add_dependency(&pkg_name, "1.0.0", &dep2);
            }
        }
    }

    let mut root_reqs = Vec::with_capacity(width);
    for w in 0..width {
        let req_str = format!("pkg-0-{w} >= 1.0.0");
        root_reqs.push(Requirement::from_str(&req_str).expect("valid root requirement"));
    }

    (provider, root_reqs)
}

fn bench_resolver(c: &mut Criterion) {
    let mut group = c.benchmark_group("pubgrub_resolver");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    // 500-node tiered graph benchmark (50 tiers x 10 packages)
    {
        let node_count = 500;
        let (provider, root_reqs) = build_tiered_graph(50, 10);
        group.throughput(Throughput::Elements(node_count as u64));
        group.bench_function(BenchmarkId::new("tiered_dag", node_count), |b| {
            b.iter(|| {
                let mut solver = PubGrubSolver::new(black_box(&provider));
                let solution = solver.resolve(black_box(&root_reqs)).expect("resolution should succeed");
                black_box(solution)
            });
        });
    }

    // 5,000-node tiered graph benchmark (500 tiers x 10 packages)
    {
        let node_count = 5000;
        let (provider, root_reqs) = build_tiered_graph(500, 10);
        group.throughput(Throughput::Elements(node_count as u64));
        group.bench_function(BenchmarkId::new("tiered_dag", node_count), |b| {
            b.iter(|| {
                let mut solver = PubGrubSolver::new(black_box(&provider));
                let solution = solver.resolve(black_box(&root_reqs)).expect("resolution should succeed");
                black_box(solution)
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_resolver);
criterion_main!(benches);
