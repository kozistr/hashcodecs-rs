//! AArch64 NEON Base64 decoding kernels.

use std::arch::aarch64::*;

use super::super::Base64Error;

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
pub(crate) const DECODE_ERROR_CHECK_INTERVAL: usize = 4 * 1024;

#[derive(Clone, Copy)]
struct DecodeTables {
    high_classes: uint8x16_t,
    low_classes: uint8x16_t,
    offsets: uint8x16_t,
}

#[target_feature(enable = "neon")]
pub(crate) unsafe fn decode_neon<const URLSAFE: bool, const MIXED: bool>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    unsafe { decode_neon_mode::<URLSAFE, MIXED, false>(input, output) }
}

#[target_feature(enable = "neon")]
pub(crate) unsafe fn decode_neon_transactional<const URLSAFE: bool, const MIXED: bool>(
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
