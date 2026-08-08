use std::hint::black_box;
use std::io::Cursor;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

mod support;

const SIZES: [usize; 3] = [4 * 1024, 1024 * 1024, 32 * 1024 * 1024];
const SAMPLE_SIZE: usize = 50;

fn data(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(17))
        .collect()
}

fn x86_128_as_u128(words: [u32; 4]) -> u128 {
    (words[0] as u128)
        | ((words[1] as u128) << 32)
        | ((words[2] as u128) << 64)
        | ((words[3] as u128) << 96)
}

fn x64_128_as_u128(words: [u64; 2]) -> u128 {
    (words[0] as u128) | ((words[1] as u128) << 64)
}

macro_rules! benchmark {
    ($group:expr, $size:expr, $input:expr, $name:literal, $function:expr) => {
        $group.bench_with_input(BenchmarkId::new($name, $size), $input, |bench, input| {
            bench.iter(|| black_box(($function)(black_box(input))));
        });
    };
}

fn murmur3(c: &mut Criterion) {
    support::pin_to_one_cpu();
    x86_32(c);
    x86_128(c);
    x64_128(c);
}

fn x86_32(c: &mut Criterion) {
    let mut group = c.benchmark_group("x86_32");
    for size in SIZES {
        let input = data(size);
        let expected = hashcodecs::murmur3_x86_32(&input, 42);
        assert_eq!(
            murmur3::murmur3_32(&mut Cursor::new(&input), 42).unwrap(),
            expected
        );
        assert_eq!(murmurs::murmur3_x86_32(&input, 42), expected);
        assert_eq!(mm3h::murmurhash3_32_with_seed(&input, 42), expected);
        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(group, size, &input, "hashcodecs", |input: &[u8]| {
            hashcodecs::murmur3_x86_32(input, 42)
        });
        benchmark!(group, size, &input, "murmur3", |input: &[u8]| {
            murmur3::murmur3_32(&mut Cursor::new(input), 42).unwrap()
        });
        benchmark!(group, size, &input, "murmurs", |input: &[u8]| {
            murmurs::murmur3_x86_32(input, 42)
        });
        benchmark!(group, size, &input, "mm3h", |input: &[u8]| {
            mm3h::murmurhash3_32_with_seed(input, 42)
        });
    }
    group.finish();
}

fn x86_128(c: &mut Criterion) {
    let mut group = c.benchmark_group("x86_128");
    for size in SIZES {
        let input = data(size);
        let expected = hashcodecs::murmur3_x86_128(&input, 42);
        assert_eq!(
            murmur3::murmur3_x86_128(&mut Cursor::new(&input), 42).unwrap(),
            x86_128_as_u128(expected)
        );
        assert_eq!(murmurs::murmur3_x86_128(&input, 42), expected);
        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(group, size, &input, "hashcodecs", |input: &[u8]| {
            hashcodecs::murmur3_x86_128(input, 42)
        });
        benchmark!(group, size, &input, "murmur3", |input: &[u8]| {
            murmur3::murmur3_x86_128(&mut Cursor::new(input), 42).unwrap()
        });
        benchmark!(group, size, &input, "murmurs", |input: &[u8]| {
            murmurs::murmur3_x86_128(input, 42)
        });
    }
    group.finish();
}

fn x64_128(c: &mut Criterion) {
    let mut group = c.benchmark_group("x64_128");
    for size in SIZES {
        let input = data(size);
        let expected = hashcodecs::murmur3_x64_128(&input, 42);
        let expected_u128 = x64_128_as_u128(expected);
        assert_eq!(
            murmur3::murmur3_x64_128(&mut Cursor::new(&input), 42).unwrap(),
            expected_u128
        );
        assert_eq!(murmurs::murmur3_x64_128(&input, 42), expected);
        assert_eq!(fastmurmur3::murmur3_x64_128(&input, 42), expected_u128);
        assert_eq!(mm3h::murmurhash3_128_with_seed(&input, 42), expected_u128);
        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(group, size, &input, "hashcodecs", |input: &[u8]| {
            hashcodecs::murmur3_x64_128(input, 42)
        });
        benchmark!(group, size, &input, "murmur3", |input: &[u8]| {
            murmur3::murmur3_x64_128(&mut Cursor::new(input), 42).unwrap()
        });
        benchmark!(group, size, &input, "murmurs", |input: &[u8]| {
            murmurs::murmur3_x64_128(input, 42)
        });
        benchmark!(group, size, &input, "fastmurmur3", |input: &[u8]| {
            fastmurmur3::murmur3_x64_128(input, 42)
        });
        benchmark!(group, size, &input, "mm3h", |input: &[u8]| {
            mm3h::murmurhash3_128_with_seed(input, 42)
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(1))
        .sample_size(30)
        .warm_up_time(Duration::from_millis(500));
    targets = murmur3
}
criterion_main!(benches);
