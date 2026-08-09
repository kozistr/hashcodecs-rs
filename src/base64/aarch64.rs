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
    source
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
    let tables = unsafe { decode_tables::<URLSAFE, MIXED>() };
    let mut source = 0;
    let mut destination = 0;
    while source + 256 <= input.len() {
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source),
                output.add(destination),
                tables,
            )?
        };
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source + 64),
                output.add(destination + 48),
                tables,
            )?
        };
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source + 128),
                output.add(destination + 96),
                tables,
            )?
        };
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source + 192),
                output.add(destination + 144),
                tables,
            )?
        };
        source += 256;
        destination += 192;
    }
    while source + 64 <= input.len() {
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source),
                output.add(destination),
                tables,
            )?
        };
        source += 64;
        destination += 48;
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
    let (indices, valid) = decode_indices::<URLSAFE, MIXED>(input, tables);
    if vminvq_u8(valid) != u8::MAX {
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
    output: *mut u8,
    tables: DecodeTables,
) -> Result<(), Base64Error> {
    let input = unsafe { vld4q_u8(input) };
    let (first, first_valid) = decode_indices::<URLSAFE, MIXED>(input.0, tables);
    let (second, second_valid) = decode_indices::<URLSAFE, MIXED>(input.1, tables);
    let (third, third_valid) = decode_indices::<URLSAFE, MIXED>(input.2, tables);
    let (fourth, fourth_valid) = decode_indices::<URLSAFE, MIXED>(input.3, tables);
    let valid = vandq_u8(
        vandq_u8(first_valid, second_valid),
        vandq_u8(third_valid, fourth_valid),
    );
    if vminvq_u8(valid) != u8::MAX {
        return Err(Base64Error::InvalidInput);
    }

    let decoded = uint8x16x3_t(
        vorrq_u8(vshlq_n_u8::<2>(first), vshrq_n_u8::<4>(second)),
        vorrq_u8(vshlq_n_u8::<4>(second), vshrq_n_u8::<2>(third)),
        vorrq_u8(vshlq_n_u8::<6>(third), fourth),
    );
    unsafe { vst3q_u8(output, decoded) };
    Ok(())
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
    let slash = vceqq_u8(value, vdupq_n_u8(b'/'));
    let offset_indices = if MIXED || !URLSAFE {
        vaddq_u8(high_nibbles, slash)
    } else {
        high_nibbles
    };
    let mut indices = vaddq_u8(value, vqtbl1q_u8(tables.offsets, offset_indices));
    if MIXED {
        let dash = vceqq_u8(value, vdupq_n_u8(b'-'));
        let underscore = vceqq_u8(value, vdupq_n_u8(b'_'));
        let corrections = vorrq_u8(
            vandq_u8(dash, vdupq_n_u8(254)),
            vandq_u8(underscore, vdupq_n_u8(33)),
        );
        indices = vaddq_u8(indices, corrections);
    } else if URLSAFE {
        let underscore = vceqq_u8(value, vdupq_n_u8(b'_'));
        indices = vaddq_u8(indices, vandq_u8(underscore, vdupq_n_u8(33)));
    }
    (indices, vceqq_u8(errors, vdupq_n_u8(0)))
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn decode_tables<const URLSAFE: bool, const MIXED: bool>() -> DecodeTables {
    let (high_classes, low_classes, offsets) = if MIXED {
        (&URLSAFE_HIGH_CLASSES, &MIXED_LOW_CLASSES, &STANDARD_OFFSETS)
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
