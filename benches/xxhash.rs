use std::ffi::c_void;
use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

mod support;

const SIZES: [usize; 4] = [64, 4 * 1024, 1024 * 1024, 32 * 1024 * 1024];

fn data(size: usize, salt: u8) -> Vec<u8> {
    (0..size)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(salt))
        .collect()
}

fn c_xxh3_64(input: &[u8], seed: u64) -> u64 {
    unsafe {
        xxhash_c_sys::XXH3_64bits_withSeed(input.as_ptr().cast::<c_void>(), input.len(), seed)
    }
}

fn c_xxh3_128(input: &[u8], seed: u64) -> [u64; 2] {
    let hash = unsafe {
        xxhash_c_sys::XXH3_128bits_withSeed(input.as_ptr().cast::<c_void>(), input.len(), seed)
    };
    [hash.low64, hash.high64]
}

fn one_shot(c: &mut Criterion) {
    for size in SIZES {
        let input = data(size, 17);
        let mut group = c.benchmark_group(format!("xxh3_64/{size}"));
        group.throughput(Throughput::Bytes(size as u64));
        assert_eq!(hashcodecs::xxh3_64(&input, 42), c_xxh3_64(&input, 42));
        group.bench_function("hashcodecs", |bench| {
            bench.iter(|| hashcodecs::xxh3_64(black_box(&input), 42))
        });
        group.bench_function("upstream_c", |bench| {
            bench.iter(|| c_xxh3_64(black_box(&input), 42))
        });
        group.finish();

        let mut group = c.benchmark_group(format!("xxh3_128/{size}"));
        group.throughput(Throughput::Bytes(size as u64));
        assert_eq!(hashcodecs::xxh3_128(&input, 42), c_xxh3_128(&input, 42));
        group.bench_function("hashcodecs", |bench| {
            bench.iter(|| hashcodecs::xxh3_128(black_box(&input), 42))
        });
        group.bench_function("upstream_c", |bench| {
            bench.iter(|| c_xxh3_128(black_box(&input), 42))
        });
        group.finish();
    }
}

fn batch(c: &mut Criterion) {
    const ITEMS: usize = 32;
    for size in [64, 4 * 1024, 1024 * 1024] {
        let owned = (0..ITEMS)
            .map(|index| data(size, index as u8))
            .collect::<Vec<_>>();
        let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let mut group = c.benchmark_group(format!("xxh3_batch/{size}"));
        group.throughput(Throughput::Bytes((size * ITEMS) as u64));

        group.bench_with_input(
            BenchmarkId::new("hashcodecs_64", ITEMS),
            &inputs,
            |bench, inputs| bench.iter(|| hashcodecs::xxh3_64_batch(black_box(inputs), 42)),
        );
        group.bench_with_input(
            BenchmarkId::new("upstream_c_64", ITEMS),
            &inputs,
            |bench, inputs| {
                bench.iter(|| {
                    inputs
                        .iter()
                        .map(|input| c_xxh3_64(black_box(input), 42))
                        .collect::<Vec<_>>()
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("hashcodecs_128", ITEMS),
            &inputs,
            |bench, inputs| bench.iter(|| hashcodecs::xxh3_128_batch(black_box(inputs), 42)),
        );
        group.bench_with_input(
            BenchmarkId::new("upstream_c_128", ITEMS),
            &inputs,
            |bench, inputs| {
                bench.iter(|| {
                    inputs
                        .iter()
                        .map(|input| c_xxh3_128(black_box(input), 42))
                        .collect::<Vec<_>>()
                })
            },
        );
        group.finish();
    }
}

fn xxhash(c: &mut Criterion) {
    support::pin_to_one_cpu();
    one_shot(c);
    batch(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(1))
        .sample_size(30)
        .warm_up_time(Duration::from_millis(300));
    targets = xxhash
}
criterion_main!(benches);
