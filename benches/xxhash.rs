use std::ffi::c_void;
use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

mod support;

const SIZES: [usize; 15] = [
    16,
    17,
    64,
    128,
    129,
    240,
    241,
    512,
    768,
    1024,
    1536,
    2048,
    4 * 1024,
    1024 * 1024,
    8 * 1024 * 1024,
];
const MIXED_BATCHES: [(&str, [usize; 4]); 3] = [
    ("two_long_runs", [1024, 1024, 4 * 1024, 4 * 1024]),
    ("short_then_long_boundary", [240, 240, 241, 241]),
    ("long_then_short_boundary", [241, 241, 240, 240]),
];

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
        assert_eq!(
            hashcodecs::xxhash::xxh3_64(&input, 42),
            c_xxh3_64(&input, 42)
        );
        group.bench_function("hashcodecs", |bench| {
            bench.iter(|| hashcodecs::xxhash::xxh3_64(black_box(&input), 42))
        });
        group.bench_function("upstream_c", |bench| {
            bench.iter(|| c_xxh3_64(black_box(&input), 42))
        });
        group.finish();

        let mut group = c.benchmark_group(format!("xxh3_128/{size}"));
        group.throughput(Throughput::Bytes(size as u64));
        assert_eq!(
            hashcodecs::xxhash::xxh3_128(&input, 42),
            c_xxh3_128(&input, 42)
        );
        group.bench_function("hashcodecs", |bench| {
            bench.iter(|| hashcodecs::xxhash::xxh3_128(black_box(&input), 42))
        });
        group.bench_function("upstream_c", |bench| {
            bench.iter(|| c_xxh3_128(black_box(&input), 42))
        });
        group.finish();
    }
}

fn benchmark_batch(c: &mut Criterion, group_name: &str, owned: &[Vec<u8>]) {
    let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let expected_64 = inputs
        .iter()
        .map(|input| c_xxh3_64(input, 42))
        .collect::<Vec<_>>();
    let expected_128 = inputs
        .iter()
        .map(|input| c_xxh3_128(input, 42))
        .collect::<Vec<_>>();
    assert_eq!(hashcodecs::xxhash::xxh3_64_batch(&inputs, 42), expected_64);
    assert_eq!(
        hashcodecs::xxhash::xxh3_128_batch(&inputs, 42),
        expected_128
    );

    let items = inputs.len();
    let total_size = inputs.iter().map(|input| input.len()).sum::<usize>();
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(total_size as u64));
    group.bench_with_input(
        BenchmarkId::new("hashcodecs_64", items),
        &inputs,
        |bench, inputs| bench.iter(|| hashcodecs::xxhash::xxh3_64_batch(black_box(inputs), 42)),
    );
    group.bench_with_input(
        BenchmarkId::new("upstream_c_64", items),
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
        BenchmarkId::new("hashcodecs_128", items),
        &inputs,
        |bench, inputs| bench.iter(|| hashcodecs::xxhash::xxh3_128_batch(black_box(inputs), 42)),
    );
    group.bench_with_input(
        BenchmarkId::new("upstream_c_128", items),
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

fn batch(c: &mut Criterion) {
    for items in [2, 3, 32] {
        for size in [64, 1024, 4 * 1024, 1024 * 1024] {
            let owned = (0..items)
                .map(|index| data(size, index as u8))
                .collect::<Vec<_>>();
            benchmark_batch(c, &format!("xxh3_batch/{items}_items/{size}"), &owned);
        }
    }

    for (name, sizes) in MIXED_BATCHES {
        let owned = sizes
            .into_iter()
            .enumerate()
            .map(|(index, size)| data(size, index as u8))
            .collect::<Vec<_>>();
        benchmark_batch(c, &format!("xxh3_batch/mixed/{name}"), &owned);
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
        .sample_size(support::SAMPLE_SIZE)
        .warm_up_time(Duration::from_millis(300));
    targets = xxhash
}
criterion_main!(benches);
