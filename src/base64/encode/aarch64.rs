//! AArch64 NEON encoding kernel.

use std::arch::aarch64::*;

use super::super::{STANDARD_ALPHABET, URLSAFE_ALPHABET};

#[target_feature(enable = "neon")]
pub(crate) unsafe fn encode<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
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
