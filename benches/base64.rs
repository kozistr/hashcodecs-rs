use std::hint::black_box;
use std::time::Duration;

use base64::Engine;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

mod support;

const SIZES: [usize; 4] = [1024, 4 * 1024, 1024 * 1024, 8 * 1024 * 1024];

macro_rules! benchmark {
    ($group:expr, $size:expr, $input:expr, $name:literal, $function:expr) => {
        $group.bench_with_input(BenchmarkId::new($name, $size), $input, |bench, input| {
            bench.iter(|| black_box(($function)(black_box(input))));
        });
    };
}

macro_rules! benchmark_encode {
    ($group:expr, $size:expr, $input:expr, $name:literal, $function:expr, $consume:expr) => {
        $group.bench_with_input(BenchmarkId::new($name, $size), $input, |bench, input| {
            bench.iter(|| {
                let output = black_box(($function)(black_box(input)));
                if $consume {
                    black_box(output.bytes().fold(0_u8, u8::wrapping_add));
                }
            });
        });
    };
}

fn base64(c: &mut Criterion) {
    support::pin_to_one_cpu();
    standard_encode::<false>(c);
    urlsafe_encode::<false>(c);
    standard_decode(c);
    urlsafe_decode(c);
    standard_encode::<true>(c);
    urlsafe_encode::<true>(c);
}

fn standard_encode<const CONSUME: bool>(c: &mut Criterion) {
    let name = if CONSUME {
        "standard_encode_consumed"
    } else {
        "standard_encode"
    };
    let mut group = c.benchmark_group(name);

    for size in SIZES {
        let input = support::data(size, 17);
        let expected = hashcodecs::base64::b64encode(&input);
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(&input),
            expected
        );
        assert_eq!(base64_turbo::STANDARD.encode(&input), expected);

        group.throughput(Throughput::Bytes(size as u64));
        benchmark_encode!(
            group,
            size,
            &input,
            "hashcodecs",
            hashcodecs::base64::b64encode,
            CONSUME
        );
        benchmark_encode!(
            group,
            size,
            &input,
            "base64",
            |input: &[u8]| base64::engine::general_purpose::STANDARD.encode(input),
            CONSUME
        );
        benchmark_encode!(
            group,
            size,
            &input,
            "base64-turbo",
            |input: &[u8]| base64_turbo::STANDARD.encode(input),
            CONSUME
        );
    }

    group.finish();
}

fn urlsafe_encode<const CONSUME: bool>(c: &mut Criterion) {
    let name = if CONSUME {
        "urlsafe_encode_consumed"
    } else {
        "urlsafe_encode"
    };
    let mut group = c.benchmark_group(name);

    for size in SIZES {
        let input = support::data(size, 17);
        let expected = hashcodecs::base64::b64encode_urlsafe(&input);
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE.encode(&input),
            expected
        );
        assert_eq!(base64_turbo::URL_SAFE.encode(&input), expected);

        group.throughput(Throughput::Bytes(size as u64));
        benchmark_encode!(
            group,
            size,
            &input,
            "hashcodecs",
            hashcodecs::base64::b64encode_urlsafe,
            CONSUME
        );
        benchmark_encode!(
            group,
            size,
            &input,
            "base64",
            |input: &[u8]| base64::engine::general_purpose::URL_SAFE.encode(input),
            CONSUME
        );
        benchmark_encode!(
            group,
            size,
            &input,
            "base64-turbo",
            |input: &[u8]| base64_turbo::URL_SAFE.encode(input),
            CONSUME
        );
    }

    group.finish();
}

fn standard_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("standard_decode");

    for size in SIZES {
        let expected = support::data(size, 17);
        let input = hashcodecs::base64::b64encode(&expected);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&input)
                .unwrap(),
            expected
        );
        assert_eq!(base64_turbo::STANDARD.decode(&input).unwrap(), expected);

        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(
            group,
            size,
            input.as_bytes(),
            "hashcodecs",
            |input: &[u8]| { hashcodecs::base64::b64decode(input).unwrap() }
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
        let expected = support::data(size, 17);
        let input = hashcodecs::base64::b64encode_urlsafe(&expected);
        assert_eq!(
            base64::engine::general_purpose::URL_SAFE
                .decode(&input)
                .unwrap(),
            expected
        );
        assert_eq!(base64_turbo::URL_SAFE.decode(&input).unwrap(), expected);

        group.throughput(Throughput::Bytes(size as u64));
        benchmark!(
            group,
            size,
            input.as_bytes(),
            "hashcodecs",
            |input: &[u8]| { hashcodecs::base64::b64decode_urlsafe(input).unwrap() }
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
        .sample_size(support::SAMPLE_SIZE)
        .warm_up_time(Duration::from_millis(500));
    targets = base64
}
criterion_main!(benches);
