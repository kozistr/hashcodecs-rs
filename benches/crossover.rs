use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

mod support;

const BASE64_ENCODE_SIZES: [usize; 10] = [15, 16, 31, 32, 47, 48, 51, 52, 103, 104];
const BASE64_DECODE_SIZES: [usize; 8] = [12, 16, 28, 32, 60, 64, 124, 128];
const MURMUR_SIZES: [usize; 6] = [15, 16, 31, 32, 255, 256];

fn data(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(17))
        .collect()
}

fn base64_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("base64_encode_crossover");
    for size in BASE64_ENCODE_SIZES {
        let input = data(size);
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |bench, input| {
            bench.iter(|| hashcodecs::base64::b64encode(black_box(input)));
        });
    }
    group.finish();
}

fn base64_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("base64_decode_crossover");
    for size in BASE64_DECODE_SIZES {
        let input = vec![b'A'; size];
        assert!(hashcodecs::base64::b64decode(&input).is_ok());
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |bench, input| {
            bench.iter(|| hashcodecs::base64::b64decode(black_box(input)).unwrap());
        });
    }
    group.finish();
}

macro_rules! murmur_group {
    ($criterion:expr, $name:literal, $function:path) => {{
        let mut group = $criterion.benchmark_group($name);
        for size in MURMUR_SIZES {
            let input = data(size);
            group.throughput(Throughput::Bytes(size as u64));
            group.bench_with_input(BenchmarkId::from_parameter(size), &input, |bench, input| {
                bench.iter(|| $function(black_box(input), 42));
            });
        }
        group.finish();
    }};
}

fn murmur3(c: &mut Criterion) {
    murmur_group!(
        c,
        "murmur_x86_32_crossover",
        hashcodecs::murmur3::murmur3_x86_32
    );
    murmur_group!(
        c,
        "murmur_x86_128_crossover",
        hashcodecs::murmur3::murmur3_x86_128
    );
    murmur_group!(
        c,
        "murmur_x64_128_crossover",
        hashcodecs::murmur3::murmur3_x64_128
    );
}

fn crossover(c: &mut Criterion) {
    support::pin_to_one_cpu();
    base64_encode(c);
    base64_decode(c);
    murmur3(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(1))
        .sample_size(support::SAMPLE_SIZE)
        .warm_up_time(Duration::from_millis(300));
    targets = crossover
}
criterion_main!(benches);
