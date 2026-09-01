use super::block_buffer::FullBlocks;
use super::primitives::read_partial_u64_le;
use super::x64_128::murmur3_x64_128_scalar_inner;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x64_128::{finish_x64_128, mix_x64_128_body_scalar, mix_x64_128_body_with_backend};
use super::x86_32::murmur3_x86_32_scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x86_32::{finish_x86_32, mix_x86_32_body_scalar, mix_x86_32_body_with_backend};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x86_128::mix_x86_128_body_with_backend;
use super::x86_128::{finalize_x86_128, finish_x86_128, mix_x86_128_body, mix_x86_128_body_scalar};
use super::*;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::{x64_128::x86 as x64_x86, x86_32::x86 as x86_32_x86, x86_128::x86 as x86_128_x86};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::{self as cpu, SimdBackend};
use std::io::Cursor;

fn x86_words_as_u128(words: [u32; 4]) -> u128 {
    let mut bytes = [0; 16];
    for (index, word) in words.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    u128::from_le_bytes(bytes)
}

fn x64_words_as_u128(words: [u64; 2]) -> u128 {
    (words[0] as u128) | ((words[1] as u128) << 64)
}

#[test]
fn dispatch_thresholds_are_explicit_and_feature_gated() {
    use crate::backend::{Capabilities, SimdBackend as Simd};
    use dispatch::Backend::{Avx2, Scalar, Sse41};
    let caps = |backend| Capabilities::from_supported_backends(&[backend]);

    assert_eq!(
        dispatch::select_x86_32_backend(15, caps(Simd::Avx2)),
        Scalar
    );
    assert_eq!(
        dispatch::select_x86_32_backend(31, caps(Simd::Avx2)),
        Scalar
    );
    assert_eq!(
        dispatch::select_x86_32_backend(16, caps(Simd::Sse41)),
        Sse41
    );
    assert_eq!(dispatch::select_x86_32_backend(32, caps(Simd::Avx2)), Avx2);
    assert_eq!(
        dispatch::select_x86_32_backend(usize::MAX, caps(Simd::Scalar)),
        Scalar
    );

    assert_eq!(
        dispatch::select_x86_128_backend(255, caps(Simd::Avx2)),
        Scalar
    );
    assert_eq!(
        dispatch::select_x86_128_backend(256, caps(Simd::Avx2)),
        Avx2
    );
    assert_eq!(
        dispatch::select_x86_128_backend(16 * 1024 * 1024 - 1, caps(Simd::Sse41)),
        Scalar
    );
    assert_eq!(
        dispatch::select_x86_128_backend(16 * 1024 * 1024, caps(Simd::Sse41)),
        Sse41
    );
    assert_eq!(
        dispatch::select_x86_128_backend(usize::MAX, caps(Simd::Scalar)),
        Scalar
    );

    assert_eq!(
        dispatch::select_x64_128_backend(15, caps(Simd::Avx2)),
        Scalar
    );
    assert_eq!(
        dispatch::select_x64_128_backend(16, caps(Simd::Sse41)),
        Sse41
    );
    assert_eq!(dispatch::select_x64_128_backend(32, caps(Simd::Avx2)), Avx2);
    assert_eq!(
        dispatch::select_x64_128_backend(8 * 1024 * 1024, caps(Simd::Sse41)),
        Sse41
    );
    assert_eq!(
        dispatch::select_x64_128_backend(8 * 1024 * 1024 + 1, caps(Simd::Sse41)),
        Scalar
    );
}

#[test]
fn incremental_hashers_match_one_shot_for_all_tail_lengths() {
    let seeds = [0, 1, u32::MAX];
    let chunk_sizes = [1, 2, 3, 4, 7, 16, 31, 64];
    for length in 0..=128 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(29).wrapping_add(7))
            .collect();
        for seed in seeds {
            for chunk_size in chunk_sizes {
                let mut x86_32 = Murmur3X86Hasher32::new(seed);
                let mut x86_128 = Murmur3X86Hasher128::new(seed);
                let mut x64_128 = Murmur3X64Hasher128::new(seed);
                for chunk in input.chunks(chunk_size) {
                    x86_32.update(chunk);
                    x86_128.update(chunk);
                    x64_128.update(chunk);
                }
                assert_eq!(x86_32.digest(), murmur3_x86_32(&input, seed));
                assert_eq!(x86_128.digest(), murmur3_x86_128(&input, seed));
                assert_eq!(x64_128.digest(), murmur3_x64_128(&input, seed));
            }
        }
    }

    let mut original = Murmur3X64Hasher128::default();
    original.update(b"prefix");
    let snapshot = original.clone();
    original.update(b"-suffix");
    assert_eq!(snapshot.digest(), murmur3_x64_128(b"prefix", 0));
    assert_eq!(original.digest(), murmur3_x64_128(b"prefix-suffix", 0));
    assert_eq!(
        Murmur3X86Hasher32::default().digest(),
        murmur3_x86_32(b"", 0)
    );
    assert_eq!(
        Murmur3X86Hasher128::default().digest(),
        murmur3_x86_128(b"", 0)
    );
}

#[test]
fn incremental_hasher_debug_is_redacted() {
    let secret = b"secret message bytes";

    let mut x86_32 = Murmur3X86Hasher32::new(7);
    let mut x86_128 = Murmur3X86Hasher128::new(7);
    let mut x64_128 = Murmur3X64Hasher128::new(7);
    x86_32.update(secret);
    x86_128.update(secret);
    x64_128.update(secret);

    assert_eq!(
        format!("{x86_32:?}"),
        "Murmur3Hasher { algorithm: \"x86_32\", total_length: 20, buffered_length: 0 }"
    );
    assert_eq!(
        format!("{x86_128:?}"),
        "Murmur3Hasher { algorithm: \"x86_128\", total_length: 20, buffered_length: 4 }"
    );
    assert_eq!(
        format!("{x64_128:?}"),
        "Murmur3Hasher { algorithm: \"x64_128\", total_length: 20, buffered_length: 4 }"
    );
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn assert_x86_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
    let block_end = input.len() & !15;
    let capabilities = cpu::capabilities();
    let supported = [
        capabilities
            .supports(SimdBackend::Sse41)
            .then_some(dispatch::Backend::Sse41),
        capabilities
            .supports(SimdBackend::Avx2)
            .then_some(dispatch::Backend::Avx2),
    ];
    for selected in supported.into_iter().flatten() {
        let mut hashes = [seed; 4];
        let blocks = FullBlocks::new(&input[..block_end]).unwrap();
        assert!(unsafe { x86_128_x86::try_mix_x86_128_body(blocks, &mut hashes, selected) });
        assert_eq!(
            x86_words_as_u128(finish_x86_128(input, hashes, block_end)),
            expected
        );
    }

    let mut unchanged = [seed; 4];
    assert!(!unsafe {
        x86_128_x86::try_mix_x86_128_body(
            FullBlocks::new(&input[..block_end]).unwrap(),
            &mut unchanged,
            dispatch::Backend::Scalar,
        )
    });
    assert_eq!(unchanged, [seed; 4]);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn assert_x64_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
    let block_end = input.len() & !15;
    let capabilities = cpu::capabilities();
    let supported = [
        capabilities
            .supports(SimdBackend::Sse41)
            .then_some((dispatch::Backend::Sse41, false)),
        capabilities
            .supports(SimdBackend::Avx2)
            .then_some((dispatch::Backend::Avx2, false)),
        (capabilities.supports(SimdBackend::Avx2) && capabilities.has_bmi2())
            .then_some((dispatch::Backend::Avx2, true)),
    ];
    for (selected, bmi2) in supported.into_iter().flatten() {
        let mut hashes = [seed as u64; 2];
        assert!(unsafe {
            x64_x86::try_mix_x64_128_body(
                FullBlocks::new(&input[..block_end]).unwrap(),
                &mut hashes,
                selected,
                bmi2,
            )
        });
        assert_eq!(
            x64_words_as_u128(finish_x64_128(input, hashes, block_end)),
            expected
        );
    }

    let mut unchanged = [seed as u64; 2];
    assert!(!unsafe {
        x64_x86::try_mix_x64_128_body(
            FullBlocks::new(&input[..block_end]).unwrap(),
            &mut unchanged,
            dispatch::Backend::Scalar,
            false,
        )
    });
    assert_eq!(unchanged, [seed as u64; 2]);
}

#[test]
fn known_answer_vectors() {
    assert_eq!(read_partial_u64_le(&[]), 0);
    assert_eq!(murmur3_x86_32(b"hello", 0), 0x248b_fa47);
    assert_eq!(murmur3_x86_32(&[1, 2, 3], 0), 2_161_234_436);
    assert_eq!(
        murmur3_x86_128(&[1, 2, 3], 0),
        [4_127_286_497, 3_037_938_227, 3_037_938_227, 3_037_938_227]
    );
    assert_eq!(
        murmur3_x64_128(&[1, 2, 3], 0),
        [1_901_714_139_111_438_249, 5_282_241_052_699_499_109]
    );
}

#[test]
fn scalar_x86_128_body_matches_the_reference() {
    let data: Vec<u8> = (0..255).map(|value| value as u8).collect();
    let mut scalar = [7; 4];
    mix_x86_128_body(FullBlocks::new(&data[..240]).unwrap(), &mut scalar);
    let scalar_hash = finalize_x86_128(scalar, 240);
    assert_eq!(
        x86_words_as_u128(scalar_hash),
        murmur3::murmur3_x86_128(&mut Cursor::new(&data[..240]), 7).unwrap()
    );
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn scalar_dispatch_fallbacks_process_full_blocks() {
    let data = (0..256).map(|value| value as u8).collect::<Vec<_>>();

    let blocks32 = FullBlocks::new(&data[..32]).unwrap();
    let mut expected32 = 7;
    mix_x86_32_body_scalar(blocks32, &mut expected32);
    let mut actual32 = 7;
    mix_x86_32_body_with_backend(blocks32, &mut actual32, dispatch::Backend::Scalar);
    assert_eq!(actual32, expected32);

    let blocks128 = FullBlocks::new(&data).unwrap();
    let mut expected_x86 = [11; 4];
    mix_x86_128_body_scalar(blocks128, &mut expected_x86);
    let mut actual_x86 = [11; 4];
    mix_x86_128_body_with_backend(blocks128, &mut actual_x86, dispatch::Backend::Scalar);
    assert_eq!(actual_x86, expected_x86);

    let mut expected_x64 = [13; 2];
    mix_x64_128_body_scalar(blocks128, &mut expected_x64);
    let mut actual_x64 = [13; 2];
    mix_x64_128_body_with_backend(blocks128, &mut actual_x64, dispatch::Backend::Scalar, false);
    assert_eq!(actual_x64, expected_x64);
}

#[test]
fn matches_the_reference_implementation_for_every_tail_length() {
    let seeds = [0, 1, 0xfeed_beef, u32::MAX];
    for length in 0..=512 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(73).wrapping_add(19))
            .collect();
        for seed in seeds {
            let expected_x86_32 = murmur3::murmur3_32(&mut Cursor::new(&input), seed).unwrap();
            assert_eq!(
                murmur3_x86_32(&input, seed),
                expected_x86_32,
                "x86_32 length={length} seed={seed}"
            );
            assert_eq!(
                murmur3_x86_32_scalar(&input, seed),
                expected_x86_32,
                "scalar x86_32 length={length} seed={seed}"
            );
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                let capabilities = cpu::capabilities();
                let block_end = input.len() & !3;
                let supported = [
                    capabilities
                        .supports(SimdBackend::Sse41)
                        .then_some(dispatch::Backend::Sse41),
                    capabilities
                        .supports(SimdBackend::Avx2)
                        .then_some(dispatch::Backend::Avx2),
                ];
                for selected in supported.into_iter().flatten() {
                    let mut hash = seed;
                    assert!(unsafe {
                        x86_32_x86::try_mix_x86_32_body(
                            FullBlocks::new(&input[..block_end]).unwrap(),
                            &mut hash,
                            selected,
                        )
                    });
                    assert_eq!(
                        finish_x86_32(&input, hash, block_end),
                        expected_x86_32,
                        "{selected:?} x86_32 length={length} seed={seed}"
                    );
                }

                let mut unchanged = seed;
                assert!(!unsafe {
                    x86_32_x86::try_mix_x86_32_body(
                        FullBlocks::new(&input[..block_end]).unwrap(),
                        &mut unchanged,
                        dispatch::Backend::Scalar,
                    )
                });
                assert_eq!(unchanged, seed);
            }

            let expected_x86_128 =
                murmur3::murmur3_x86_128(&mut Cursor::new(&input), seed).unwrap();
            let x86 = x86_words_as_u128(murmur3_x86_128(&input, seed));
            assert_eq!(x86, expected_x86_128, "x86_128 length={length} seed={seed}");
            let block_end = input.len() & !15;
            let mut scalar_x86_128 = [seed; 4];
            mix_x86_128_body_scalar(
                FullBlocks::new(&input[..block_end]).unwrap(),
                &mut scalar_x86_128,
            );
            assert_eq!(
                x86_words_as_u128(finish_x86_128(&input, scalar_x86_128, block_end)),
                expected_x86_128,
                "scalar x86_128 length={length} seed={seed}"
            );
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            assert_x86_128_simd_backends(&input, seed, expected_x86_128);

            let x64 = x64_words_as_u128(murmur3_x64_128(&input, seed));
            let expected_x64_128 =
                murmur3::murmur3_x64_128(&mut Cursor::new(&input), seed).unwrap();
            assert_eq!(x64, expected_x64_128, "x64_128 length={length} seed={seed}");
            assert_eq!(
                x64_words_as_u128(murmur3_x64_128_scalar_inner(&input, seed as u64)),
                expected_x64_128,
                "scalar x64_128 length={length} seed={seed}"
            );
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            assert_x64_128_simd_backends(&input, seed, expected_x64_128);
        }
    }
}
