use super::x64_words_as_u128;
use crate::murmur3::block_buffer::FullBlocks;
use crate::murmur3::primitives::read_partial_u64_le;
use crate::murmur3::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128, x64_128, x86_32, x86_128};
use std::io::Cursor;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::{
    backend::{self as cpu, CpuFeature},
    murmur3::dispatch,
};

fn x86_words_as_u128(words: [u32; 4]) -> u128 {
    let mut bytes = [0; 16];

    for (index, word) in words.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }

    u128::from_le_bytes(bytes)
}

#[test]
fn match_known_digests() {
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
fn match_scalar_x86_128() {
    let data: Vec<u8> = (0..255).map(|value| value as u8).collect();
    let mut scalar = [7; 4];
    x86_128::mix_body(FullBlocks::new(&data[..240]).unwrap(), &mut scalar);
    let scalar_hash = x86_128::finalize(scalar, 240);
    assert_eq!(
        x86_words_as_u128(scalar_hash),
        murmur3::murmur3_x86_128(&mut Cursor::new(&data[..240]), 7).unwrap()
    );
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[test]
fn match_scalar_fallback() {
    let data = (0..256).map(|value| value as u8).collect::<Vec<_>>();

    let blocks32 = FullBlocks::new(&data[..32]).unwrap();
    let mut expected32 = 7;
    x86_32::mix_body_scalar(blocks32, &mut expected32);
    let mut actual32 = 7;
    x86_32::mix_body_with_backend(blocks32, &mut actual32, dispatch::Backend::Scalar);
    assert_eq!(actual32, expected32);

    let blocks128 = FullBlocks::new(&data).unwrap();
    let mut expected_x86 = [11; 4];
    x86_128::mix_body_scalar(blocks128, &mut expected_x86);
    let mut actual_x86 = [11; 4];
    x86_128::mix_body_with_backend(blocks128, &mut actual_x86, dispatch::Backend::Scalar);
    assert_eq!(actual_x86, expected_x86);

    let mut expected_x64 = [13; 2];
    x64_128::mix_body_scalar(blocks128, &mut expected_x64);
    let mut actual_x64 = [13; 2];
    x64_128::mix_body_with_backend(blocks128, &mut actual_x64, dispatch::Backend::Scalar, false);
    assert_eq!(actual_x64, expected_x64);
}

#[test]
fn match_tail_lengths() {
    let seeds = [0, 1, 0xfeed_beef, u32::MAX];

    for length in 0..=543 {
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
                x86_32::hash_scalar(&input, seed),
                expected_x86_32,
                "scalar x86_32 length={length} seed={seed}"
            );
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                let capabilities = cpu::capabilities();
                let block_end = input.len() & !3;
                let supported = [
                    capabilities
                        .supports(CpuFeature::Sse41)
                        .then_some(dispatch::Backend::Sse41),
                    capabilities
                        .supports(CpuFeature::Avx2)
                        .then_some(dispatch::Backend::Avx2),
                ];

                for selected in supported.into_iter().flatten() {
                    let mut hash = seed;
                    unsafe {
                        x86_32::x86::mix_body(
                            FullBlocks::new(&input[..block_end]).unwrap(),
                            &mut hash,
                            selected,
                        )
                    };
                    assert_eq!(
                        x86_32::finish(&input, hash, block_end),
                        expected_x86_32,
                        "{selected:?} x86_32 length={length} seed={seed}"
                    );
                }
            }

            let expected_x86_128 =
                murmur3::murmur3_x86_128(&mut Cursor::new(&input), seed).unwrap();
            let x86 = x86_words_as_u128(murmur3_x86_128(&input, seed));
            assert_eq!(x86, expected_x86_128, "x86_128 length={length} seed={seed}");
            let block_end = input.len() & !15;
            let mut scalar_x86_128 = [seed; 4];
            x86_128::mix_body_scalar(
                FullBlocks::new(&input[..block_end]).unwrap(),
                &mut scalar_x86_128,
            );
            assert_eq!(
                x86_words_as_u128(x86_128::finish(&input, scalar_x86_128, block_end)),
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
                x64_words_as_u128(x64_128::hash_scalar(&input, seed as u64)),
                expected_x64_128,
                "scalar x64_128 length={length} seed={seed}"
            );
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            assert_x64_128_simd_backends(&input, seed, expected_x64_128);
        }
    }
}

#[test]
fn match_x64_unaligned_blocks() {
    let data: Vec<u8> = (0..4160)
        .map(|index| (index as u8).wrapping_mul(137).wrapping_add(251))
        .collect();

    for offset in 0..32 {
        for length in [
            16, 31, 128, 255, 511, 512, 513, 1023, 1024, 1025, 4095, 4096, 4097,
        ] {
            let input = &data[offset..offset + length];

            for seed in [0, 0xfeed_beef, u32::MAX] {
                let expected = murmur3::murmur3_x64_128(&mut Cursor::new(input), seed).unwrap();
                assert_eq!(
                    x64_words_as_u128(murmur3_x64_128(input, seed)),
                    expected,
                    "offset={offset} length={length} seed={seed}"
                );
                assert_eq!(
                    x64_words_as_u128(x64_128::hash_scalar(input, seed as u64)),
                    expected,
                    "scalar offset={offset} length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                assert_x64_128_simd_backends(input, seed, expected);
            }
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn assert_x86_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
    let block_end = input.len() & !15;
    let capabilities = cpu::capabilities();
    let supported = [
        capabilities
            .supports(CpuFeature::Sse41)
            .then_some(dispatch::Backend::Sse41),
        capabilities
            .supports(CpuFeature::Avx2)
            .then_some(dispatch::Backend::Avx2),
    ];

    for selected in supported.into_iter().flatten() {
        let mut hashes = [seed; 4];
        let blocks = FullBlocks::new(&input[..block_end]).unwrap();
        unsafe { x86_128::x86::mix_body(blocks, &mut hashes, selected) };
        assert_eq!(
            x86_words_as_u128(x86_128::finish(input, hashes, block_end)),
            expected
        );
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn assert_x64_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
    let block_end = input.len() & !15;
    let capabilities = cpu::capabilities();
    let supported = [
        capabilities
            .supports(CpuFeature::Sse41)
            .then_some((dispatch::Backend::Sse41, false)),
        capabilities
            .supports(CpuFeature::Avx2)
            .then_some((dispatch::Backend::Avx2, false)),
        (capabilities.supports(CpuFeature::Avx2) && capabilities.supports(CpuFeature::Bmi2))
            .then_some((dispatch::Backend::Avx2, true)),
    ];

    for (selected, bmi2) in supported.into_iter().flatten() {
        let mut hashes = [seed as u64; 2];
        unsafe {
            x64_128::x86::mix_body(
                FullBlocks::new(&input[..block_end]).unwrap(),
                &mut hashes,
                selected,
                bmi2,
            )
        };
        assert_eq!(
            x64_words_as_u128(x64_128::finish(input, hashes, block_end)),
            expected
        );
    }
}
