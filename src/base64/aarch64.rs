use std::arch::aarch64::*;

use super::{Base64Error, STANDARD_ALPHABET, URLSAFE_ALPHABET};

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
    let mut source = 0;
    let mut destination = 0;
    while source + 256 <= input.len() {
        unsafe {
            decode_64::<URLSAFE, MIXED>(input.as_ptr().add(source), output.add(destination))?
        };
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source + 64),
                output.add(destination + 48),
            )?
        };
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source + 128),
                output.add(destination + 96),
            )?
        };
        unsafe {
            decode_64::<URLSAFE, MIXED>(
                input.as_ptr().add(source + 192),
                output.add(destination + 144),
            )?
        };
        source += 256;
        destination += 192;
    }
    while source + 64 <= input.len() {
        unsafe {
            decode_64::<URLSAFE, MIXED>(input.as_ptr().add(source), output.add(destination))?
        };
        source += 64;
        destination += 48;
    }
    while source + 16 <= input.len() {
        unsafe {
            decode_16::<URLSAFE, MIXED>(input.as_ptr().add(source), output.add(destination))?
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
) -> Result<(), Base64Error> {
    let input = unsafe { vld1q_u8(input) };
    let (indices, valid) = decode_indices::<URLSAFE, MIXED>(input);
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
) -> Result<(), Base64Error> {
    let input = unsafe { vld4q_u8(input) };
    let (first, first_valid) = decode_indices::<URLSAFE, MIXED>(input.0);
    let (second, second_valid) = decode_indices::<URLSAFE, MIXED>(input.1);
    let (third, third_valid) = decode_indices::<URLSAFE, MIXED>(input.2);
    let (fourth, fourth_valid) = decode_indices::<URLSAFE, MIXED>(input.3);
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
) -> (uint8x16_t, uint8x16_t) {
    let uppercase = between(value, b'A', b'Z');
    let lowercase = between(value, b'a', b'z');
    let digit = between(value, b'0', b'9');
    let standard_62 = vceqq_u8(value, vdupq_n_u8(b'+'));
    let standard_63 = vceqq_u8(value, vdupq_n_u8(b'/'));
    let urlsafe_62 = vceqq_u8(value, vdupq_n_u8(b'-'));
    let urlsafe_63 = vceqq_u8(value, vdupq_n_u8(b'_'));
    let special_62 = if MIXED {
        vorrq_u8(standard_62, urlsafe_62)
    } else if URLSAFE {
        urlsafe_62
    } else {
        standard_62
    };
    let special_63 = if MIXED {
        vorrq_u8(standard_63, urlsafe_63)
    } else if URLSAFE {
        urlsafe_63
    } else {
        standard_63
    };

    let mut indices = vsubq_u8(value, vdupq_n_u8(b'A'));
    indices = vbslq_u8(lowercase, vsubq_u8(value, vdupq_n_u8(b'a' - 26)), indices);
    indices = vbslq_u8(digit, vaddq_u8(value, vdupq_n_u8(4)), indices);
    indices = vbslq_u8(special_62, vdupq_n_u8(62), indices);
    indices = vbslq_u8(special_63, vdupq_n_u8(63), indices);
    let valid = vorrq_u8(
        vorrq_u8(uppercase, lowercase),
        vorrq_u8(digit, vorrq_u8(special_62, special_63)),
    );
    (indices, valid)
}

#[target_feature(enable = "neon")]
#[inline]
fn between(value: uint8x16_t, lower: u8, upper: u8) -> uint8x16_t {
    vandq_u8(
        vcgeq_u8(value, vdupq_n_u8(lower)),
        vcleq_u8(value, vdupq_n_u8(upper)),
    )
}
