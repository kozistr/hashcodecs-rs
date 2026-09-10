//! AArch64 NEON encoding kernel.

use std::arch::aarch64::*;

use super::super::{STANDARD_ALPHABET, URLSAFE_ALPHABET};
#[cfg(feature = "python")]
use super::WrappedOutput;

#[target_feature(enable = "neon")]
pub(crate) unsafe fn encode<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    let alphabet = const {
        if URLSAFE {
            URLSAFE_ALPHABET
        } else {
            STANDARD_ALPHABET
        }
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
    unsafe { vst4_u8(output, encode_24_value(input, table)) };
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn encode_24_value(input: *const u8, table: uint8x16x4_t) -> uint8x8x4_t {
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

    uint8x8x4_t(
        vqtbl4_u8(table, first),
        vqtbl4_u8(table, second),
        vqtbl4_u8(table, third),
        vqtbl4_u8(table, fourth),
    )
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn encode_48(input: *const u8, output: *mut u8, table: uint8x16x4_t) {
    unsafe { vst4q_u8(output, encode_48_value(input, table)) };
}

#[target_feature(enable = "neon")]
#[inline]
unsafe fn encode_48_value(input: *const u8, table: uint8x16x4_t) -> uint8x16x4_t {
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

    uint8x16x4_t(
        vqtbl4q_u8(table, first),
        vqtbl4q_u8(table, second),
        vqtbl4q_u8(table, third),
        vqtbl4q_u8(table, fourth),
    )
}

#[cfg(feature = "python")]
#[target_feature(enable = "neon")]
pub(crate) unsafe fn encode_wrapped<const URLSAFE: bool>(
    input: &[u8],
    output: &mut WrappedOutput,
) -> usize {
    let alphabet = const {
        if URLSAFE {
            URLSAFE_ALPHABET
        } else {
            STANDARD_ALPHABET
        }
    };
    let table = uint8x16x4_t(
        unsafe { vld1q_u8(alphabet.as_ptr()) },
        unsafe { vld1q_u8(alphabet.as_ptr().add(16)) },
        unsafe { vld1q_u8(alphabet.as_ptr().add(32)) },
        unsafe { vld1q_u8(alphabet.as_ptr().add(48)) },
    );
    let mut source = 0;

    while source + 192 <= input.len() {
        for offset in [0, 48, 96, 144] {
            let encoded = unsafe { encode_48_value(input.as_ptr().add(source + offset), table) };
            unsafe { write_wrapped_64(output, encoded) };
        }
        source += 192;
    }

    while source + 48 <= input.len() {
        let encoded = unsafe { encode_48_value(input.as_ptr().add(source), table) };
        unsafe { write_wrapped_64(output, encoded) };
        source += 48;
    }

    if source + 24 <= input.len() {
        let encoded = unsafe { encode_24_value(input.as_ptr().add(source), table) };
        unsafe { write_wrapped_32(output, encoded) };
        source += 24;
    }

    source
}

#[cfg(feature = "python")]
#[target_feature(enable = "neon")]
#[inline]
unsafe fn write_wrapped_32(output: &mut WrappedOutput, value: uint8x8x4_t) {
    let first_second = vzip_u8(value.0, value.1);
    let third_fourth = vzip_u8(value.2, value.3);
    let low = vzip_u16(
        vreinterpret_u16_u8(first_second.0),
        vreinterpret_u16_u8(third_fourth.0),
    );
    let high = vzip_u16(
        vreinterpret_u16_u8(first_second.1),
        vreinterpret_u16_u8(third_fourth.1),
    );

    unsafe {
        output.write_16(core::mem::transmute::<uint8x16_t, [u8; 16]>(vcombine_u8(
            vreinterpret_u8_u16(low.0),
            vreinterpret_u8_u16(low.1),
        )));
        output.write_16(core::mem::transmute::<uint8x16_t, [u8; 16]>(vcombine_u8(
            vreinterpret_u8_u16(high.0),
            vreinterpret_u8_u16(high.1),
        )));
    }
}

#[cfg(feature = "python")]
#[target_feature(enable = "neon")]
#[inline]
unsafe fn write_wrapped_64(output: &mut WrappedOutput, value: uint8x16x4_t) {
    let first_second_low = vzip1q_u8(value.0, value.1);
    let first_second_high = vzip2q_u8(value.0, value.1);
    let third_fourth_low = vzip1q_u8(value.2, value.3);
    let third_fourth_high = vzip2q_u8(value.2, value.3);

    unsafe {
        output.write_16(core::mem::transmute::<uint16x8_t, [u8; 16]>(vzip1q_u16(
            vreinterpretq_u16_u8(first_second_low),
            vreinterpretq_u16_u8(third_fourth_low),
        )));
        output.write_16(core::mem::transmute::<uint16x8_t, [u8; 16]>(vzip2q_u16(
            vreinterpretq_u16_u8(first_second_low),
            vreinterpretq_u16_u8(third_fourth_low),
        )));
        output.write_16(core::mem::transmute::<uint16x8_t, [u8; 16]>(vzip1q_u16(
            vreinterpretq_u16_u8(first_second_high),
            vreinterpretq_u16_u8(third_fourth_high),
        )));
        output.write_16(core::mem::transmute::<uint16x8_t, [u8; 16]>(vzip2q_u16(
            vreinterpretq_u16_u8(first_second_high),
            vreinterpretq_u16_u8(third_fourth_high),
        )));
    }
}
