#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::{self, CpuFeature};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::long_inputs::{
    LongInput, X86Backend, accumulate_x86, initialize_secret_with_capabilities,
    select_x86_accumulation_kernel, select_x86_backend,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::long_inputs::{accumulate_long_input_scalar, initialize_secret_scalar};
use super::*;
use core::ffi::c_void;

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

#[test]
fn empty_vectors() {
    assert_eq!(xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
    assert_eq!(
        xxh3_128(b"", 0),
        [0x6001_c324_468d_497f, 0x99aa_06d3_0147_98d8]
    );
}

#[test]
fn batches_match_one_shot() {
    let values: [&[u8]; 3] = [b"", b"hello", b"xxhash"];
    assert_eq!(xxh3_64_batch(&values, 42), values.map(|v| xxh3_64(v, 42)));
    assert_eq!(xxh3_128_batch(&values, 42), values.map(|v| xxh3_128(v, 42)));

    let mut hashes_64 = [0; 3];
    let mut hashes_128 = [[0; 2]; 3];
    let mut index = 0;
    xxh3_64_batch_for_each(&values, 42, |hash| {
        hashes_64[index] = hash;
        index += 1;
    });
    assert_eq!(index, values.len());
    let mut index = 0;
    xxh3_128_batch_for_each(&values, 42, |hash| {
        hashes_128[index] = hash;
        index += 1;
    });
    assert_eq!(index, values.len());
    assert_eq!(hashes_64, values.map(|value| xxh3_64(value, 42)));
    assert_eq!(hashes_128, values.map(|value| xxh3_128(value, 42)));

    let mixed_owned = [17, 129, 241, 300].map(|length| {
        (0..length)
            .map(|index| (index as u8).wrapping_mul(19).wrapping_add(7))
            .collect::<Vec<_>>()
    });
    let mixed = mixed_owned.each_ref().map(Vec::as_slice);
    assert_eq!(xxh3_64_batch(&mixed, 42), mixed.map(|v| xxh3_64(v, 42)));
    assert_eq!(xxh3_128_batch(&mixed, 42), mixed.map(|v| xxh3_128(v, 42)));

    for item_count in 2..=8 {
        let owned = (0..item_count)
            .map(|item| {
                (0..4161)
                    .map(|index| (index as u8).wrapping_mul(31).wrapping_add(item))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert_eq!(
            xxh3_64_batch(&inputs, 0x1234_5678),
            inputs
                .iter()
                .map(|input| xxhash_rust::xxh3::xxh3_64_with_seed(input, 0x1234_5678))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            xxh3_128_batch(&inputs, 0x1234_5678),
            inputs
                .iter()
                .map(|input| {
                    let hash = xxhash_rust::xxh3::xxh3_128_with_seed(input, 0x1234_5678);
                    [hash as u64, (hash >> 64) as u64]
                })
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn batches_consume_contiguous_equal_length_runs() {
    let owned = [300, 300, 17, 512, 512, 241, 241, 241].map(|length| {
        (0..length)
            .map(|index| (index as u8).wrapping_mul(43).wrapping_add(length as u8))
            .collect::<Vec<_>>()
    });
    let inputs = owned.each_ref().map(Vec::as_slice);
    for seed in [0, 0x0123_4567_89ab_cdef] {
        assert_eq!(
            xxh3_64_batch(&inputs, seed),
            inputs.map(|input| xxh3_64(input, seed))
        );
        assert_eq!(
            xxh3_128_batch(&inputs, seed),
            inputs.map(|input| xxh3_128(input, seed))
        );
    }
}

#[test]
fn matches_reference_at_every_length_through_two_blocks() {
    let input = (0..=2048)
        .map(|index| (index as u8).wrapping_mul(73).wrapping_add(29))
        .collect::<Vec<_>>();
    for length in 0..=2048 {
        let input = &input[..length];
        for &seed in &[0, 0xd6e8_feb8_6659_fd93] {
            assert_eq!(
                xxh3_64(input, seed),
                c_xxh3_64(input, seed),
                "XXH3-64 mismatch for length {length}, seed {seed:#x}",
            );
            let actual = xxh3_128(input, seed);
            assert_eq!(
                actual,
                c_xxh3_128(input, seed),
                "XXH3-128 mismatch for length {length}, seed {seed:#x}",
            );
        }
    }
}

#[test]
fn matches_xxhash_reference_at_boundaries_and_large_lengths() {
    const LENGTHS: &[usize] = &[
        0, 1, 2, 3, 4, 8, 9, 16, 17, 31, 32, 33, 63, 64, 65, 96, 97, 127, 128, 129, 159, 160, 191,
        192, 239, 240, 241, 255, 256, 511, 512, 1023, 1024, 1025, 4161,
    ];
    const SEEDS: &[u64] = &[0, 1, 0x0123_4567_89ab_cdef, u64::MAX];

    for &length in LENGTHS {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(131).wrapping_add(17))
            .collect();
        for &seed in SEEDS {
            assert_eq!(
                xxh3_64(&input, seed),
                xxhash_rust::xxh3::xxh3_64_with_seed(&input, seed),
                "XXH3-64 mismatch for length {length}, seed {seed:#x}",
            );
            let reference = xxhash_rust::xxh3::xxh3_128_with_seed(&input, seed);
            let actual = xxh3_128(&input, seed);
            assert_eq!(
                (u128::from(actual[1]) << 64) | u128::from(actual[0]),
                reference,
                "XXH3-128 mismatch for length {length}, seed {seed:#x}",
            );
        }
    }
}

#[test]
fn randomized_inputs_match_the_official_c_implementation() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for case in 0..128 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let length = (state as usize) % (128 * 1024 + 1);
        let mut input = vec![0_u8; length];
        for byte in &mut input {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        state = state.rotate_left(29).wrapping_add(case);
        let seed = state;
        assert_eq!(xxh3_64(&input, seed), c_xxh3_64(&input, seed));
        assert_eq!(xxh3_128(&input, seed), c_xxh3_128(&input, seed));
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn every_supported_x86_backend_matches_scalar() {
    use crate::backend::Capabilities;

    let input: Vec<u8> = (0..4161)
        .map(|index| (index as u8).wrapping_mul(47).wrapping_add(91))
        .collect();
    let chain_input: Vec<u8> = (0..4096)
        .map(|index| (index as u8).wrapping_mul(53).wrapping_add(17))
        .collect();
    let capabilities = backend::capabilities();
    let scalar = Capabilities::from_features(&[]);
    assert_eq!(select_x86_backend(scalar), X86Backend::Scalar);
    assert_eq!(
        select_x86_backend(Capabilities::from_features(&[CpuFeature::Ssse3])),
        X86Backend::Ssse3
    );
    assert_eq!(
        select_x86_backend(Capabilities::from_features(&[CpuFeature::Sse41])),
        X86Backend::Scalar
    );
    assert_eq!(
        select_x86_backend(Capabilities::from_features(&[
            CpuFeature::Sse41,
            CpuFeature::Ssse3,
        ])),
        X86Backend::Ssse3
    );
    assert_eq!(
        select_x86_backend(Capabilities::from_features(&[CpuFeature::Avx2])),
        X86Backend::Avx2
    );
    assert_eq!(
        select_x86_backend(Capabilities::from_features(&[CpuFeature::Avx512F])),
        X86Backend::Avx512
    );
    assert!(select_x86_accumulation_kernel(X86Backend::Scalar).is_none());
    for backend in [X86Backend::Ssse3, X86Backend::Avx2, X86Backend::Avx512] {
        assert!(select_x86_accumulation_kernel(backend).is_some());
    }
    for &seed in &[0, 1, 0xfeed_beef_cafe_babe] {
        let secret = initialize_secret_scalar(seed);
        let long_input = LongInput::new(&input).unwrap();
        let expected = accumulate_long_input_scalar(long_input, &secret);
        assert_eq!(
            initialize_secret_with_capabilities(seed, capabilities),
            initialize_secret_scalar(seed)
        );
        assert_eq!(
            initialize_secret_with_capabilities(seed, scalar),
            initialize_secret_scalar(seed)
        );
        assert_eq!(
            unsafe { accumulate_x86(long_input, &secret, X86Backend::Scalar) },
            expected
        );

        let supported = [
            (X86Backend::Scalar, &[][..]),
            (X86Backend::Ssse3, &[CpuFeature::Ssse3][..]),
            (X86Backend::Avx2, &[CpuFeature::Avx2][..]),
            (X86Backend::Avx512, &[CpuFeature::Avx512F][..]),
        ];
        for (selected, required) in supported
            .into_iter()
            .filter(|(_, required)| capabilities.supports_all(required))
        {
            let forced = Capabilities::from_features(required);
            assert_eq!(select_x86_backend(forced), selected);
            let actual = unsafe { accumulate_x86(long_input, &secret, selected) };
            assert_eq!(actual, expected, "{selected:?} mismatch for seed {seed:#x}");
            if selected == X86Backend::Avx2 {
                for length in [241, 512, 768, 1024, 1536, 2048, 4096] {
                    let chain_input = &chain_input[..length];
                    let long_chain = LongInput::new(chain_input).unwrap();
                    assert_eq!(
                        unsafe { accumulate_x86(long_chain, &secret, selected) },
                        accumulate_long_input_scalar(long_chain, &secret),
                        "AVX2 four-chain mismatch at {length} bytes for seed {seed:#x}",
                    );
                }
            }
        }
    }
}
