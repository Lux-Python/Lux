#![allow(clippy::pedantic, clippy::nursery)]

use std::hint::black_box;
use std::time::Duration;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tempfile::TempDir;

use lux_cache::archive::wheel::extract_wheel;
use lux_cache::cas::{CasStore, EnvironmentLinker};

fn create_mock_wheel(num_files: usize) -> Vec<u8> {
    let mut archive = Vec::new();
    let mut cd_entries = Vec::new();

    for i in 0..num_files {
        let name = format!("bench_pkg/submod_{i}.py");
        let content = format!("# Module {i}\ndef calculate_{i}(x: int) -> int:\n    return x * {i}\n").into_bytes();
        let compressed = miniz_oxide::deflate::compress_to_vec(&content, 6);
        let method = 8u16;
        let local_offset = archive.len() as u32;

        archive.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        archive.extend_from_slice(&20u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&method.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u32.to_le_bytes());
        archive.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(content.len() as u32).to_le_bytes());
        archive.extend_from_slice(&(name.len() as u16).to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(name.as_bytes());
        archive.extend_from_slice(&compressed);

        cd_entries.push((name, local_offset, method, compressed.len() as u32, content.len() as u32));
    }

    let cd_start = archive.len() as u32;

    for (name, local_offset, method, comp_size, uncomp_size) in &cd_entries {
        archive.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        archive.extend_from_slice(&20u16.to_le_bytes());
        archive.extend_from_slice(&20u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&method.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u32.to_le_bytes());
        archive.extend_from_slice(&comp_size.to_le_bytes());
        archive.extend_from_slice(&uncomp_size.to_le_bytes());
        archive.extend_from_slice(&(name.len() as u16).to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u16.to_le_bytes());
        archive.extend_from_slice(&0u32.to_le_bytes());
        archive.extend_from_slice(&local_offset.to_le_bytes());
        archive.extend_from_slice(name.as_bytes());
    }

    let cd_end = archive.len() as u32;
    let cd_size = cd_end - cd_start;

    archive.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    archive.extend_from_slice(&0u16.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());
    archive.extend_from_slice(&(num_files as u16).to_le_bytes());
    archive.extend_from_slice(&(num_files as u16).to_le_bytes());
    archive.extend_from_slice(&cd_size.to_le_bytes());
    archive.extend_from_slice(&cd_start.to_le_bytes());
    archive.extend_from_slice(&0u16.to_le_bytes());

    archive
}

fn bench_cas_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("cas_linking_and_storage");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    // 1. Benchmark CAS Linking Throughput
    {
        let num_files = 50;
        let wheel_bytes = create_mock_wheel(num_files);
        let temp_cas = TempDir::new().expect("cas dir");
        let store = CasStore::open(Some(temp_cas.path().to_path_buf())).expect("store");
        let (_, extracted_path) = store.store_extracted_wheel(&wheel_bytes).expect("extracted");

        group.throughput(Throughput::Elements(num_files as u64));
        group.bench_function(BenchmarkId::new("link_tree", num_files), |b| {
            let mut counter = 0usize;
            b.iter(|| {
                counter += 1;
                let target_dir = temp_cas.path().join(format!("env_bench_{counter}"));
                let report = EnvironmentLinker::link_tree(black_box(&extracted_path), black_box(&target_dir))
                    .expect("linking succeeds");
                black_box(report)
            });
        });
    }

    // 2. Benchmark Wheel Unpacking & Extraction
    {
        let num_files = 50;
        let wheel_bytes = create_mock_wheel(num_files);
        let temp_cas = TempDir::new().expect("cas dir");
        let uncompressed_bytes: usize = (0..num_files)
            .map(|i| format!("# Module {i}\ndef calculate_{i}(x: int) -> int:\n    return x * {i}\n").len())
            .sum();

        group.throughput(Throughput::Bytes(uncompressed_bytes as u64));
        group.bench_function(BenchmarkId::new("extract_wheel", num_files), |b| {
            let mut counter = 0usize;
            b.iter(|| {
                counter += 1;
                let dest_dir = temp_cas.path().join(format!("unpack_{counter}"));
                let res = extract_wheel(black_box(&wheel_bytes), black_box(&dest_dir))
                    .expect("extraction succeeds");
                black_box(res)
            });
        });
    }

    // 3. Benchmark Blob Storage & SHA-256
    {
        let blob_size = 64 * 1024; // 64 KB
        let sample_blob = vec![0x3cu8; blob_size];
        let temp_cas = TempDir::new().expect("cas dir");
        let store = CasStore::open(Some(temp_cas.path().to_path_buf())).expect("store");

        group.throughput(Throughput::Bytes(blob_size as u64));
        group.bench_function(BenchmarkId::new("store_blob_64kb", blob_size), |b| {
            b.iter(|| {
                let res = store.store_blob(black_box(&sample_blob)).expect("blob store");
                black_box(res)
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_cas_operations);
criterion_main!(benches);