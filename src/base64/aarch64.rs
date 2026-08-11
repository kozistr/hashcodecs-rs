use std::arch::aarch64::*;

use super::{Base64Error, STANDARD_ALPHABET, URLSAFE_ALPHABET};

const STANDARD_HIGH_CLASSES: [u8; 16] = [
    0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
];
const STANDARD_LOW_CLASSES: [u8; 16] = [
    0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x2a, 0x2b, 0x2b, 0x2b, 0x2a,
];
const URLSAFE_HIGH_CLASSES: [u8; 16] = [
    0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
];
const URLSAFE_LOW_CLASSES: [u8; 16] = [
    0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3b, 0x3b, 0x3a, 0x3b, 0x33,
];
const MIXED_LOW_CLASSES: [u8; 16] = [
    0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3a, 0x3b, 0x3a, 0x3b, 0x32,
];
const STANDARD_OFFSETS: [u8; 16] = [0, 16, 19, 4, 191, 191, 185, 185, 0, 0, 0, 0, 0, 0, 0, 0];
const URLSAFE_OFFSETS: [u8; 16] = [0, 0, 17, 4, 191, 191, 185, 185, 0, 0, 0, 0, 0, 0, 0, 0];
const MIXED_OFFSETS: [u8; 16] = [0, 16, 19, 4, 191, 191, 185, 185, 17, 224, 0, 0, 0, 0, 0, 0];

// Amortize the horizontal reduction without scanning an unbounded amount of
// input after the first invalid byte. This is measured in encoded input bytes.
const DECODE_ERROR_CHECK_INTERVAL: usize = 4 * 1024;
const _: () = assert!(
    DECODE_ERROR_CHECK_INTERVAL >= 64 && DECODE_ERROR_CHECK_INTERVAL.is_multiple_of(64),
    "the NEON decode error-check interval must contain complete 64-byte blocks"
);

#[derive(Clone, Copy)]
struct DecodeTables {
    high_classes: uint8x16_t,
    low_classes: uint8x16_t,
    offsets: uint8x16_t,
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn encode_neon<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    let alphabet = if URLSAFE {
        URLSAFE_ALPHABET
    } else {
        STANDARD_ALPHABET
    };
    let table = uint8x16x4_t(
        unsafe { vld1q_u8(alphabet.as_ptr()) },
        unsafe { vld1q_u8(alphabet.as_ptr().add(16)) },
        unsafe { vld1q_u8(alphabet.as_ptr().add(32)) },
        unsafe { vld1q_u8(alphabet.as_ptr().add(48)) },
    );

    let mut source = 0;
    let mut destination = 0;
    while source + 192 <= input.len() {
        unsafe { encode_48(input.as_ptr().add(source), output.add(destination), table) };
        unsafe {
            encode_48(
                input.as_ptr().add(source + 48),
                output.add(destination + 64),
                table,
            )
        };
        unsafe {
            encode_48(
                input.as_ptr().add(source + 96),
                output.add(destination + 128),
                table,
            )
        };
        unsafe {
            encode_48(
                input.as_ptr().add(source + 144),
                output.add(destination + 192),
                table,
            )
        };
        source += 192;
        destination += 256;
    }
    while source + 48 <= input.len() {
        unsafe { encode_48(input.as_ptr().add(source), output.add(destination), table) };
        source += 48;
        destination += 64;
    }
    if source + 24 <= input.len() {
        unsafe { encode_24(input.as_ptr().add(source), output.add(destination), table) };
        source += 24;
    }
    source
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn encode_24(input: *const u8, output: *mut u8, table: uint8x16x4_t) {
    let input = unsafe { vld3_u8(input) };
    let first = vshr_n_u8::<2>(input.0);
    let second = vand_u8(
        vsli_n_u8::<4>(vshr_n_u8::<4>(input.1), input.0),
        vdup_n_u8(0x3f),
    );
    let third = vand_u8(
        vsli_n_u8::<2>(vshr_n_u8::<6>(input.2), input.1),
        vdup_n_u8(0x3f),
    );
    let fourth = vand_u8(input.2, vdup_n_u8(0x3f));
    unsafe {
        vst4_u8(
            output,
            uint8x8x4_t(
                vqtbl4_u8(table, first),
                vqtbl4_u8(table, second),
                vqtbl4_u8(table, third),
                vqtbl4_u8(table, fourth),
            ),
        )
    };
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn encode_48(input: *const u8, output: *mut u8, table: uint8x16x4_t) {
    let input = unsafe { vld3q_u8(input) };
    let first = vshrq_n_u8::<2>(input.0);
    let second = vandq_u8(
        vsliq_n_u8::<4>(vshrq_n_u8::<4>(input.1), input.0),
        vdupq_n_u8(0x3f),
    );
    let third = vandq_u8(
        vsliq_n_u8::<2>(vshrq_n_u8::<6>(input.2), input.1),
        vdupq_n_u8(0x3f),
    );
    let fourth = vandq_u8(input.2, vdupq_n_u8(0x3f));
    unsafe {
        vst4q_u8(
            output,
            uint8x16x4_t(
                vqtbl4q_u8(table, first),
                vqtbl4q_u8(table, second),
                vqtbl4q_u8(table, third),
                vqtbl4q_u8(table, fourth),
            ),
        )
    };
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn decode_neon<const URLSAFE: bool, const MIXED: bool>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    unsafe { decode_neon_mode::<URLSAFE, MIXED, false>(input, output) }
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn decode_neon_transactional<const URLSAFE: bool, const MIXED: bool>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    unsafe { decode_neon_mode::<URLSAFE, MIXED, true>(input, output) }
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn decode_neon_mode<
    const URLSAFE: bool,
    const MIXED: bool,
    const TRANSACTIONAL_ERRORS: bool,
>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let tables = unsafe { decode_tables::<URLSAFE, MIXED>() };
    let mut source = 0;
    let mut destination = 0;
    if TRANSACTIONAL_ERRORS {
        while source + 64 <= input.len() {
            let (decoded, errors) =
                unsafe { decode_64::<URLSAFE, MIXED>(input.as_ptr().add(source), tables) };
            if vmaxvq_u8(errors) != 0 {
                return Err(Base64Error::InvalidInput);
            }
            unsafe { store_decoded_64(output.add(destination), decoded) };
            source += 64;
            destination += 48;
        }
    }
    while source + 64 <= input.len() {
        let bulk_remaining = (input.len() - source) & !63;
        let chunk_end = source + bulk_remaining.min(DECODE_ERROR_CHECK_INTERVAL);
        let mut errors = vdupq_n_u8(0);
        while source < chunk_end {
            let (decoded, block_errors) =
                unsafe { decode_64::<URLSAFE, MIXED>(input.as_ptr().add(source), tables) };
            errors = vorrq_u8(errors, block_errors);
            unsafe { store_decoded_64(output.add(destination), decoded) };
            source += 64;
            destination += 48;
        }
        if vmaxvq_u8(errors) != 0 {
            return Err(Base64Error::InvalidInput);
        }
    }
    while source + 16 <= input.len() {
        unsafe {
            decode_16::<URLSAFE, MIXED>(
                input.as_ptr().add(source),
                output.add(destination),
                tables,
            )?
        };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn decode_16<const URLSAFE: bool, const MIXED: bool>(
    input: *const u8,
    output: *mut u8,
    tables: DecodeTables,
) -> Result<(), Base64Error> {
    let input = unsafe { vld1q_u8(input) };
    let (indices, errors) = decode_indices::<URLSAFE, MIXED>(input, tables);
    if vmaxvq_u8(errors) != 0 {
        return Err(Base64Error::InvalidInput);
    }

    let mut indices_array = [0_u8; 16];
    unsafe { vst1q_u8(indices_array.as_mut_ptr(), indices) };
    for group in 0..4 {
        let source = group * 4;
        let destination = group * 3;
        let first = indices_array[source];
        let second = indices_array[source + 1];
        let third = indices_array[source + 2];
        let fourth = indices_array[source + 3];
        unsafe {
            output.add(destination).write((first << 2) | (second >> 4));
            output
                .add(destination + 1)
                .write((second << 4) | (third >> 2));
            output.add(destination + 2).write((third << 6) | fourth);
        }
    }
    Ok(())
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn decode_64<const URLSAFE: bool, const MIXED: bool>(
    input: *const u8,
    tables: DecodeTables,
) -> (uint8x16x3_t, uint8x16_t) {
    let input = unsafe { vld4q_u8(input) };
    let (first, first_errors) = decode_indices::<URLSAFE, MIXED>(input.0, tables);
    let (second, second_errors) = decode_indices::<URLSAFE, MIXED>(input.1, tables);
    let (third, third_errors) = decode_indices::<URLSAFE, MIXED>(input.2, tables);
    let (fourth, fourth_errors) = decode_indices::<URLSAFE, MIXED>(input.3, tables);
    let errors = vorrq_u8(
        vorrq_u8(first_errors, second_errors),
        vorrq_u8(third_errors, fourth_errors),
    );

    let decoded = uint8x16x3_t(
        vorrq_u8(vshlq_n_u8::<2>(first), vshrq_n_u8::<4>(second)),
        vorrq_u8(vshlq_n_u8::<4>(second), vshrq_n_u8::<2>(third)),
        vorrq_u8(vshlq_n_u8::<6>(third), fourth),
    );
    (decoded, errors)
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn store_decoded_64(output: *mut u8, decoded: uint8x16x3_t) {
    unsafe { vst3q_u8(output, decoded) };
}

#[target_feature(enable = "neon")]
#[inline]
fn decode_indices<const URLSAFE: bool, const MIXED: bool>(
    value: uint8x16_t,
    tables: DecodeTables,
) -> (uint8x16_t, uint8x16_t) {
    let high_nibbles = vshrq_n_u8::<4>(value);
    let low_nibbles = vandq_u8(value, vdupq_n_u8(0x0f));
    let errors = vandq_u8(
        vqtbl1q_u8(tables.high_classes, high_nibbles),
        vqtbl1q_u8(tables.low_classes, low_nibbles),
    );
    let offset_indices = if MIXED {
        let slash = vceqq_u8(value, vdupq_n_u8(b'/'));
        let dash = vceqq_u8(value, vdupq_n_u8(b'-'));
        let underscore = vceqq_u8(value, vdupq_n_u8(b'_'));
        let offset_indices = vaddq_u8(high_nibbles, slash);
        let offset_indices = vbslq_u8(dash, vdupq_n_u8(8), offset_indices);
        vbslq_u8(underscore, vdupq_n_u8(9), offset_indices)
    } else if !URLSAFE {
        let slash = vceqq_u8(value, vdupq_n_u8(b'/'));
        vaddq_u8(high_nibbles, slash)
    } else {
        high_nibbles
    };
    let mut indices = vaddq_u8(value, vqtbl1q_u8(tables.offsets, offset_indices));
    if URLSAFE && !MIXED {
        let underscore = vceqq_u8(value, vdupq_n_u8(b'_'));
        indices = vaddq_u8(indices, vandq_u8(underscore, vdupq_n_u8(33)));
    }
    (indices, errors)
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn decode_tables<const URLSAFE: bool, const MIXED: bool>() -> DecodeTables {
    let (high_classes, low_classes, offsets) = if MIXED {
        (&URLSAFE_HIGH_CLASSES, &MIXED_LOW_CLASSES, &MIXED_OFFSETS)
    } else if URLSAFE {
        (
            &URLSAFE_HIGH_CLASSES,
            &URLSAFE_LOW_CLASSES,
            &URLSAFE_OFFSETS,
        )
    } else {
        (
            &STANDARD_HIGH_CLASSES,
            &STANDARD_LOW_CLASSES,
            &STANDARD_OFFSETS,
        )
    };
    unsafe {
        DecodeTables {
            high_classes: vld1q_u8(high_classes.as_ptr()),
            low_classes: vld1q_u8(low_classes.as_ptr()),
            offsets: vld1q_u8(offsets.as_ptr()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    decode_neon::<URLSAFE, MIXED>(
                        input,
                        guarded_output.as_mut_ptr().add(output_start),
                    )
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
                    decode_neon::<URLSAFE, MIXED>(
                        input,
                        guarded_output.as_mut_ptr().add(output_start),
                    )
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
}
