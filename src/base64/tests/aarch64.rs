use super::super::Base64Error;
use super::super::decode::aarch64::{
    DECODE_ERROR_CHECK_INTERVAL, decode_neon, decode_neon_transactional,
};
use super::super::encode::aarch64::encode_neon;

const CANARY: u8 = 0xa5;
const GUARD: usize = 32;

fn decoded_len(encoded_len: usize) -> usize {
    encoded_len / 4 * 3
}

fn decode<const URLSAFE: bool, const MIXED: bool>(
    input: &[u8],
    output: &mut [u8],
) -> Result<(usize, usize), Base64Error> {
    unsafe { decode_neon::<URLSAFE, MIXED>(input, output.as_mut_ptr()) }
}

fn error_checks_are_bounded<const URLSAFE: bool, const MIXED: bool>() {
    let total = 3 * DECODE_ERROR_CHECK_INTERVAL;
    for invalid_index in [
        0,
        DECODE_ERROR_CHECK_INTERVAL - 1,
        DECODE_ERROR_CHECK_INTERVAL,
        DECODE_ERROR_CHECK_INTERVAL + DECODE_ERROR_CHECK_INTERVAL / 2,
    ] {
        let mut input = vec![b'A'; total];
        input[invalid_index] = b'!';
        let mut output = vec![CANARY; decoded_len(total)];

        assert_eq!(
            decode::<URLSAFE, MIXED>(&input, &mut output),
            Err(Base64Error::InvalidInput)
        );

        let completed_chunks = invalid_index / DECODE_ERROR_CHECK_INTERVAL + 1;
        let untouched_from = decoded_len(completed_chunks * DECODE_ERROR_CHECK_INTERVAL);
        assert!(output[untouched_from..].iter().all(|&byte| byte == CANARY));
    }

    let final_chunk_len = DECODE_ERROR_CHECK_INTERVAL + 64;
    let mut input = vec![b'A'; final_chunk_len];
    input[final_chunk_len - 1] = b'!';
    let mut output = vec![CANARY; decoded_len(final_chunk_len)];
    assert_eq!(
        decode::<URLSAFE, MIXED>(&input, &mut output),
        Err(Base64Error::InvalidInput)
    );

    for encoded_len in [
        DECODE_ERROR_CHECK_INTERVAL - 16,
        DECODE_ERROR_CHECK_INTERVAL,
        DECODE_ERROR_CHECK_INTERVAL + 16,
        DECODE_ERROR_CHECK_INTERVAL + 64,
    ] {
        let input = vec![b'A'; encoded_len];
        let mut output = vec![CANARY; decoded_len(encoded_len)];
        assert_eq!(
            decode::<URLSAFE, MIXED>(&input, &mut output),
            Ok((encoded_len, output.len()))
        );
        assert!(output.iter().all(|&byte| byte == 0));
    }
}

#[test]
fn decode_error_checks_are_bounded() {
    error_checks_are_bounded::<false, false>();
    error_checks_are_bounded::<true, false>();
    error_checks_are_bounded::<false, true>();
}

fn decode_table<const URLSAFE: bool, const MIXED: bool>() -> &'static [u8; 256] {
    if MIXED {
        &super::super::MIXED_DECODE
    } else if URLSAFE {
        &super::super::URLSAFE_DECODE
    } else {
        &super::super::STANDARD_DECODE
    }
}

fn decode_alphabet<const URLSAFE: bool, const MIXED: bool>() -> super::super::DecodeAlphabet {
    if MIXED {
        super::super::DecodeAlphabet::Mixed
    } else if URLSAFE {
        super::super::DecodeAlphabet::UrlSafe
    } else {
        super::super::DecodeAlphabet::Standard
    }
}

fn encoded_input<const URLSAFE: bool, const MIXED: bool>(length: usize) -> Vec<u8> {
    assert_eq!(length % 4, 0);
    let mut input = Vec::with_capacity(length);
    for group in 0..length / 4 {
        let quartet = if MIXED && group % 2 == 0 {
            b"+/_-"
        } else if MIXED {
            b"-_+/"
        } else if URLSAFE {
            b"Aa-_"
        } else {
            b"Aa+/"
        };
        input.extend_from_slice(quartet);
    }
    input
}

fn scalar_decoded<const URLSAFE: bool, const MIXED: bool>(input: &[u8]) -> Vec<u8> {
    let table = decode_table::<URLSAFE, MIXED>();
    let mut output = Vec::with_capacity(decoded_len(input.len()));
    for quartet in input.chunks_exact(4) {
        let first = table[quartet[0] as usize];
        let second = table[quartet[1] as usize];
        let third = table[quartet[2] as usize];
        let fourth = table[quartet[3] as usize];
        output.extend_from_slice(&[
            (first << 2) | (second >> 4),
            (second << 4) | (third >> 2),
            (third << 6) | fourth,
        ]);
    }
    output
}

fn encode_misaligned_boundaries<const URLSAFE: bool>() {
    for length in 0..=256 {
        for input_offset in 0..16 {
            let input_start = GUARD + input_offset;
            let mut guarded_input = vec![CANARY; input_start + length + GUARD];
            let input = &mut guarded_input[input_start..input_start + length];
            for (index, byte) in input.iter_mut().enumerate() {
                *byte = (index as u8).wrapping_mul(37).wrapping_add(11);
            }

            let expected_len = super::super::encoded_len(length);
            let mut expected = vec![0; expected_len];
            super::super::encode_scalar(input, &mut expected, URLSAFE);

            let output_offset = (input_offset * 7 + length) & 15;
            let output_start = GUARD + output_offset;
            let mut guarded_output = vec![CANARY; output_start + expected_len + GUARD];
            let consumed = unsafe {
                encode_neon::<URLSAFE>(input, guarded_output.as_mut_ptr().add(output_start))
            };
            let written = consumed / 3 * 4;

            assert_eq!(consumed, length / 24 * 24, "length={length}");
            assert_eq!(
                &guarded_output[output_start..output_start + written],
                &expected[..written],
                "length={length} input_offset={input_offset} output_offset={output_offset}"
            );
            assert!(
                guarded_output[..output_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_output[output_start + written..]
                    .iter()
                    .all(|&byte| byte == CANARY),
                "SIMD suffix length={length} input_offset={input_offset} output_offset={output_offset}"
            );
            assert!(
                guarded_input[..input_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[input_start + length..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

#[test]
fn encode_misaligned_boundaries_preserve_guards_and_suffix() {
    encode_misaligned_boundaries::<false>();
    encode_misaligned_boundaries::<true>();
}

fn decode_misaligned_boundaries<const URLSAFE: bool, const MIXED: bool>() {
    let mut lengths: Vec<usize> = (0..=192).step_by(4).collect();
    lengths.extend([
        DECODE_ERROR_CHECK_INTERVAL - 16,
        DECODE_ERROR_CHECK_INTERVAL - 12,
        DECODE_ERROR_CHECK_INTERVAL - 8,
        DECODE_ERROR_CHECK_INTERVAL - 4,
        DECODE_ERROR_CHECK_INTERVAL,
        DECODE_ERROR_CHECK_INTERVAL + 4,
        DECODE_ERROR_CHECK_INTERVAL + 8,
        DECODE_ERROR_CHECK_INTERVAL + 12,
        DECODE_ERROR_CHECK_INTERVAL + 16,
        DECODE_ERROR_CHECK_INTERVAL + 60,
        DECODE_ERROR_CHECK_INTERVAL + 64,
    ]);
    lengths.sort_unstable();
    lengths.dedup();

    for length in lengths {
        let encoded = encoded_input::<URLSAFE, MIXED>(length);
        let expected = scalar_decoded::<URLSAFE, MIXED>(&encoded);
        assert!(!expected.contains(&CANARY));

        for input_offset in 0..16 {
            let input_start = GUARD + input_offset;
            let mut guarded_input = vec![CANARY; input_start + length + GUARD];
            guarded_input[input_start..input_start + length].copy_from_slice(&encoded);
            let input = &guarded_input[input_start..input_start + length];

            let output_offset = (input_offset * 7 + length / 4) & 15;
            let output_start = GUARD + output_offset;
            let mut guarded_output = vec![CANARY; output_start + expected.len() + GUARD];
            let offsets = unsafe {
                decode_neon::<URLSAFE, MIXED>(input, guarded_output.as_mut_ptr().add(output_start))
            }
            .unwrap();
            let consumed = length / 16 * 16;
            let written = decoded_len(consumed);

            assert_eq!(offsets, (consumed, written));
            assert_eq!(
                &guarded_output[output_start..output_start + written],
                &expected[..written],
                "length={length} input_offset={input_offset} output_offset={output_offset}"
            );
            assert!(
                guarded_output[..output_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_output[output_start + written..]
                    .iter()
                    .all(|&byte| byte == CANARY),
                "SIMD suffix length={length} input_offset={input_offset} output_offset={output_offset}"
            );
            assert!(
                guarded_input[..input_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[input_start + length..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

#[test]
fn decode_misaligned_boundaries_preserve_guards_and_suffix() {
    decode_misaligned_boundaries::<false, false>();
    decode_misaligned_boundaries::<true, false>();
    decode_misaligned_boundaries::<false, true>();
}

fn invalid_bytes<const URLSAFE: bool, const MIXED: bool>() -> &'static [u8] {
    if MIXED {
        b"!\xff="
    } else if URLSAFE {
        b"!\xff+/"
    } else {
        b"!\xff-_"
    }
}

fn invalid_lanes<const URLSAFE: bool, const MIXED: bool>() {
    for block_len in [16, 64, 80] {
        let lanes = if block_len == 80 {
            64..80
        } else {
            0..block_len
        };
        for invalid_index in lanes {
            for &invalid_byte in invalid_bytes::<URLSAFE, MIXED>() {
                let input_offset = invalid_index & 15;
                let input_start = GUARD + input_offset;
                let mut guarded_input = vec![CANARY; input_start + block_len + GUARD];
                guarded_input[input_start..input_start + block_len].fill(b'A');
                guarded_input[input_start + invalid_index] = invalid_byte;
                let input = &guarded_input[input_start..input_start + block_len];

                let output_offset = (invalid_index * 7 + invalid_byte as usize) & 15;
                let output_start = GUARD + output_offset;
                let output_len = decoded_len(block_len);
                let mut guarded_output = vec![CANARY; output_start + output_len + GUARD];
                let result = unsafe {
                    decode_neon::<URLSAFE, MIXED>(
                        input,
                        guarded_output.as_mut_ptr().add(output_start),
                    )
                };
                assert_eq!(result, Err(Base64Error::InvalidInput));

                let untouched_from = if block_len == 16 {
                    0
                } else if block_len == 80 {
                    48
                } else {
                    output_len
                };
                assert!(
                    guarded_output[..output_start]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
                assert!(
                    guarded_output[output_start + untouched_from..]
                        .iter()
                        .all(|&byte| byte == CANARY),
                    "block_len={block_len} invalid_index={invalid_index} invalid_byte={invalid_byte:#x}"
                );
                assert!(
                    guarded_input[..input_start]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
                assert!(
                    guarded_input[input_start + block_len..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
            }
        }
    }
}

#[test]
fn decode_rejects_every_invalid_lane_without_tail_writes() {
    invalid_lanes::<false, false>();
    invalid_lanes::<true, false>();
    invalid_lanes::<false, true>();
}

fn guarded_checkpoint_invalids<const URLSAFE: bool, const MIXED: bool>() {
    let total = 3 * DECODE_ERROR_CHECK_INTERVAL;
    for invalid_index in [
        0,
        63,
        DECODE_ERROR_CHECK_INTERVAL - 1,
        DECODE_ERROR_CHECK_INTERVAL,
        DECODE_ERROR_CHECK_INTERVAL + 63,
        2 * DECODE_ERROR_CHECK_INTERVAL - 1,
        2 * DECODE_ERROR_CHECK_INTERVAL,
        total - 1,
    ] {
        for input_offset in [0, 1, 7, 15] {
            let input_start = GUARD + input_offset;
            let mut guarded_input = vec![CANARY; input_start + total + GUARD];
            guarded_input[input_start..input_start + total].fill(b'A');
            guarded_input[input_start + invalid_index] = b'!';
            let input = &guarded_input[input_start..input_start + total];

            let output_offset = (input_offset * 7 + invalid_index) & 15;
            let output_start = GUARD + output_offset;
            let output_len = decoded_len(total);
            let mut guarded_output = vec![CANARY; output_start + output_len + GUARD];
            let result = unsafe {
                decode_neon::<URLSAFE, MIXED>(input, guarded_output.as_mut_ptr().add(output_start))
            };
            assert_eq!(result, Err(Base64Error::InvalidInput));

            let chunk_end = ((invalid_index / DECODE_ERROR_CHECK_INTERVAL + 1)
                * DECODE_ERROR_CHECK_INTERVAL)
                .min(total);
            let untouched_from = decoded_len(chunk_end);
            assert!(
                guarded_output[..output_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_output[output_start + untouched_from..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[..input_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[input_start + total..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

#[test]
fn decode_checkpoint_invalids_are_bounded_and_guarded() {
    guarded_checkpoint_invalids::<false, false>();
    guarded_checkpoint_invalids::<true, false>();
    guarded_checkpoint_invalids::<false, true>();
}

fn full_decode_misaligned_handoffs<const URLSAFE: bool, const MIXED: bool>() {
    for original_len in 3071..=3075 {
        let original: Vec<u8> = (0..original_len)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect();
        let mut encoded = vec![0; super::super::encoded_len(original_len)];
        super::super::encode_scalar(&original, &mut encoded, URLSAFE);
        if MIXED {
            for (index, byte) in encoded.iter_mut().enumerate() {
                if index / 4 % 2 != 0 {
                    *byte = match *byte {
                        b'+' => b'-',
                        b'/' => b'_',
                        byte => byte,
                    };
                }
            }
        }
        let layout = super::super::decode_layout(&encoded).unwrap();

        for input_offset in 0..16 {
            let input_start = GUARD + input_offset;
            let mut guarded_input = vec![CANARY; input_start + encoded.len() + GUARD];
            guarded_input[input_start..input_start + encoded.len()].copy_from_slice(&encoded);
            let input = &guarded_input[input_start..input_start + encoded.len()];

            let output_offset = (input_offset * 7 + original_len) & 15;
            let output_start = GUARD + output_offset;
            let mut guarded_output = vec![CANARY; output_start + original_len + GUARD];
            super::super::decode_to_slice_with_layout_and_alphabet(
                input,
                &mut guarded_output[output_start..output_start + original_len],
                layout,
                decode_alphabet::<URLSAFE, MIXED>(),
            )
            .unwrap();

            assert_eq!(
                &guarded_output[output_start..output_start + original_len],
                original
            );
            assert!(
                guarded_output[..output_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_output[output_start + original_len..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[..input_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[input_start + encoded.len()..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

fn invalid_scalar_checkpoint_tail<const URLSAFE: bool, const MIXED: bool>() {
    let encoded_len = DECODE_ERROR_CHECK_INTERVAL + 4;
    for invalid_index in [DECODE_ERROR_CHECK_INTERVAL, encoded_len - 1] {
        for input_offset in 0..16 {
            let input_start = GUARD + input_offset;
            let mut guarded_input = vec![CANARY; input_start + encoded_len + GUARD];
            guarded_input[input_start..input_start + encoded_len].fill(b'A');
            guarded_input[input_start + invalid_index] = b'!';
            let input = &guarded_input[input_start..input_start + encoded_len];
            let layout = super::super::decode_layout(input).unwrap();

            let output_offset = (input_offset * 7 + invalid_index) & 15;
            let output_start = GUARD + output_offset;
            let mut guarded_output = vec![CANARY; output_start + layout.output_len + GUARD];
            let result = super::super::decode_to_slice_with_layout_and_alphabet(
                input,
                &mut guarded_output[output_start..output_start + layout.output_len],
                layout,
                decode_alphabet::<URLSAFE, MIXED>(),
            );
            assert_eq!(result, Err(Base64Error::InvalidInput));

            let simd_output_len = decoded_len(DECODE_ERROR_CHECK_INTERVAL);
            assert!(
                guarded_output[..output_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_output[output_start + simd_output_len..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[..input_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[input_start + encoded_len..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

#[test]
fn decode_checkpoint_scalar_and_padding_handoffs_are_exact() {
    full_decode_misaligned_handoffs::<false, false>();
    full_decode_misaligned_handoffs::<true, false>();
    full_decode_misaligned_handoffs::<false, true>();
    invalid_scalar_checkpoint_tail::<false, false>();
    invalid_scalar_checkpoint_tail::<true, false>();
    invalid_scalar_checkpoint_tail::<false, true>();
}

fn transactional_invalid_blocks<const URLSAFE: bool, const MIXED: bool>() {
    const ENCODED_LEN: usize = 3 * 64;
    for invalid_index in [0, 63, 64, 127, 128, ENCODED_LEN - 1] {
        for input_offset in [0, 1, 7, 15] {
            let input_start = GUARD + input_offset;
            let mut guarded_input = vec![CANARY; input_start + ENCODED_LEN + GUARD];
            guarded_input[input_start..input_start + ENCODED_LEN].fill(b'A');
            guarded_input[input_start + invalid_index] = b'!';
            let input = &guarded_input[input_start..input_start + ENCODED_LEN];

            let output_offset = (input_offset * 7 + invalid_index) & 15;
            let output_start = GUARD + output_offset;
            let output_len = decoded_len(ENCODED_LEN);
            let mut guarded_output = vec![CANARY; output_start + output_len + GUARD];
            let result = unsafe {
                decode_neon_transactional::<URLSAFE, MIXED>(
                    input,
                    guarded_output.as_mut_ptr().add(output_start),
                )
            };
            assert_eq!(result, Err(Base64Error::InvalidInput));

            let completed_output = invalid_index / 64 * 48;
            assert!(
                guarded_output[..output_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_output[output_start + completed_output..]
                    .iter()
                    .all(|&byte| byte == CANARY),
                "invalid_index={invalid_index} input_offset={input_offset} output_offset={output_offset}"
            );
            assert!(
                guarded_input[..input_start]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
            assert!(
                guarded_input[input_start + ENCODED_LEN..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}

#[test]
fn transactional_decode_never_stores_the_failing_block() {
    transactional_invalid_blocks::<false, false>();
    transactional_invalid_blocks::<true, false>();
    transactional_invalid_blocks::<false, true>();
}
