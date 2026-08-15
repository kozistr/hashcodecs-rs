use std::hint::black_box;
use std::time::Duration;

use base64::Engine;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

mod support;

const SIZES: [usize; 4] = [1024, 4 * 1024, 1024 * 1024, 8 * 1024 * 1024];
const SAMPLE_SIZE: usize = 50;

fn data(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(17))
        .collect()
}

macro_rules! benchmark {
    ($group:expr, $size:expr, $input:expr, $name:literal, $function:expr) => {
        $group.bench_with_input(BenchmarkId::new($name, $size), $input, |bench, input| {
            bench.iter(|| black_box(($function)(black_box(input))));
        });
    };
}

fn base64(c: &mut Criterion) {
    support::pin_to_one_cpu();
    standard_encode(c);
    urlsafe_encode(c);
    standard_decode(c);
    urlsafe_decode(c);
}

fn standard_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("standard_encode");
    for size in SIZES {
        let input = data(size);
        let expected = hashcodecs::b64encode(&input);
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(&input),
            expected
        );
        assert_eq!(base64_turbo::STANDARD.encode(&input), expected);

        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(group, size, &input, "hashcodecs", hashcodecs::b64encode);
        benchmark!(group, size, &input, "base64", |input: &[u8]| {
            base64::engine::general_purpose::STANDARD.encode(input)
        });
        benchmark!(group, size, &input, "base64-turbo", |input: &[u8]| {
            base64_turbo::STANDARD.encode(input)
        });
    }
    group.finish();
}

fn urlsafe_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("urlsafe_encode");
    for size in SIZES {
        let input = data(size);
        let expected = hashcodecs::b64encode_urlsafe(&input);
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE.encode(&input),
            expected
        );
        assert_eq!(base64_turbo::URL_SAFE.encode(&input), expected);

        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(
            group,
            size,
            &input,
            "hashcodecs",
            hashcodecs::b64encode_urlsafe
        );
        benchmark!(group, size, &input, "base64", |input: &[u8]| {
            base64::engine::general_purpose::URL_SAFE.encode(input)
        });
        benchmark!(group, size, &input, "base64-turbo", |input: &[u8]| {
            base64_turbo::URL_SAFE.encode(input)
        });
    }
    group.finish();
}

fn standard_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("standard_decode");
    for size in SIZES {
        let expected = data(size);
        let input = hashcodecs::b64encode(&expected);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&input)
                .unwrap(),
            expected
        );
        assert_eq!(base64_turbo::STANDARD.decode(&input).unwrap(), expected);

        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(
            group,
            size,
            input.as_bytes(),
            "hashcodecs",
            |input: &[u8]| { hashcodecs::b64decode(input).unwrap() }
        );
        benchmark!(group, size, input.as_bytes(), "base64", |input: &[u8]| {
            base64::engine::general_purpose::STANDARD
                .decode(input)
                .unwrap()
        });
        benchmark!(
            group,
            size,
            input.as_bytes(),
            "base64-turbo",
            |input: &[u8]| { base64_turbo::STANDARD.decode(input).unwrap() }
        );
    }
    group.finish();
}

fn urlsafe_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("urlsafe_decode");
    for size in SIZES {
        let expected = data(size);
        let input = hashcodecs::b64encode_urlsafe(&expected);
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE
                .decode(&input)
                .unwrap(),
            expected
        );
        assert_eq!(base64_turbo::URL_SAFE.decode(&input).unwrap(), expected);

        group.sample_size(SAMPLE_SIZE);
        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(
            group,
            size,
            input.as_bytes(),
            "hashcodecs",
            |input: &[u8]| { hashcodecs::b64decode_urlsafe(input).unwrap() }
        );
        benchmark!(group, size, input.as_bytes(), "base64", |input: &[u8]| {
            base64::engine::general_purpose::URL_SAFE
                .decode(input)
                .unwrap()
        });
        benchmark!(
            group,
            size,
            input.as_bytes(),
            "base64-turbo",
            |input: &[u8]| { base64_turbo::URL_SAFE.decode(input).unwrap() }
        );
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(1))
        .sample_size(30)
        .warm_up_time(Duration::from_millis(500));
    targets = base64
}
criterion_main!(benches);
