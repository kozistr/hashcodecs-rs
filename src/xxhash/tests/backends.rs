use super::{c_xxh3_64, c_xxh3_128};
use crate::xxhash::long_inputs::{
    LongEngine, LongInput, accumulate_long_input_scalar, finalize_long_64, finalize_long_128,
    initialize_secret_scalar,
};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::{
    backend::{self, Capabilities, CpuFeature},
    xxhash::long_inputs::{
        X86Backend, accumulate_x86, initialize_secret_with_capabilities,
        select_x86_accumulation_kernel, select_x86_backend,
    },
};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn match_avx2_tails() {
    if !backend::capabilities().supports(CpuFeature::Avx2) {
        return;
    }

    let input = (0..1024)
        .map(|index| (index as u8).wrapping_mul(73).wrapping_add(29))
        .collect::<Vec<_>>();

    for length in 241..=1024 {
        let input = &input[..length];
        let long_input = LongInput::new(input).unwrap();

        for seed in [0, 0xd6e8_feb8_6659_fd93] {
            let secret = initialize_secret_scalar(seed);
            let acc = unsafe { accumulate_x86(long_input, &secret, X86Backend::Avx2) };
            assert_eq!(
                finalize_long_64(length, &secret, acc),
                c_xxh3_64(input, seed),
                "AVX2 XXH3-64 mismatch for length {length}, seed {seed:#x}",
            );

            assert_eq!(
                finalize_long_128(length, &secret, acc),
                c_xxh3_128(input, seed),
                "AVX2 XXH3-128 mismatch for length {length}, seed {seed:#x}",
            );
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn match_medium_hash_128() {
    let capabilities = backend::capabilities();

    for required in [
        &[][..],
        &[CpuFeature::Ssse3][..],
        &[CpuFeature::Avx2][..],
        &[CpuFeature::Avx2, CpuFeature::Avx512F][..],
    ] {
        if !capabilities.supports_all(required) {
            continue;
        }

        let engine = LongEngine::new_with_capabilities(Capabilities::from_features(required));

        for seed in [0, 42, u64::MAX] {
            let secret = initialize_secret_scalar(seed);

            for length in 241..=1025 {
                let offset = length % 32;
                let data = (0..length + offset)
                    .map(|index| (index as u8).wrapping_mul(73).wrapping_add(29))
                    .collect::<Vec<_>>();
                let input = &data[offset..];
                let long_input = LongInput::new(input).unwrap();
                let expected = c_xxh3_128(input, seed);
                assert_eq!(
                    engine.hash_128(long_input, &secret),
                    expected,
                    "length {length}, offset {offset}, seed {seed:#x}, features {required:?}",
                );
                assert_eq!(
                    engine.hash_128_seeded(long_input, seed),
                    expected,
                    "seeded length {length}, offset {offset}, seed {seed:#x}, features {required:?}",
                );
            }
        }
    }
}

#[test]
fn match_scalar_accumulation() {
    let input: Vec<u8> = (0..4161)
        .map(|index| (index as u8).wrapping_mul(47).wrapping_add(91))
        .collect();
    let native = LongEngine::new();

    for length in [241, 256, 511, 512, 1023, 1024, 1025, 2048, 4161] {
        let input = &input[..length];
        let long_input = LongInput::new(input).unwrap();

        for seed in [0, 1, 0xfeed_beef_cafe_babe] {
            let secret = initialize_secret_scalar(seed);
            let acc = accumulate_long_input_scalar(long_input, &secret);
            assert_eq!(
                finalize_long_64(length, &secret, acc),
                c_xxh3_64(input, seed)
            );
            assert_eq!(
                finalize_long_128(length, &secret, acc),
                c_xxh3_128(input, seed)
            );
            assert_eq!(native.accumulate(long_input, &secret), acc);
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn match_x86_backends() {
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
