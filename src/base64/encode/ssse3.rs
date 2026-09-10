//! x86 SSSE3 encoding kernel.

use std::arch::asm;
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(feature = "python")]
use super::WrappedOutput;

#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    unsafe { encode_with_offsets(input, output, standard_offsets::<URLSAFE>()) }
}

#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode_custom(input: &[u8], output: *mut u8, offsets: &[i8; 16]) -> usize {
    let offsets = unsafe { _mm_loadu_si128(offsets.as_ptr().cast()) };
    unsafe { encode_with_offsets(input, output, offsets) }
}

#[target_feature(enable = "ssse3")]
#[inline]
unsafe fn encode_with_offsets(input: &[u8], output: *mut u8, offsets: __m128i) -> usize {
    let mut source = 0;
    let mut destination = 0;

    while source + 52 <= input.len() {
        let first = unsafe { encode_12(input.as_ptr().add(source), offsets) };
        let second = unsafe { encode_12(input.as_ptr().add(source + 12), offsets) };
        let third = unsafe { encode_12(input.as_ptr().add(source + 24), offsets) };
        let fourth = unsafe { encode_12(input.as_ptr().add(source + 36), offsets) };

        unsafe { _mm_storeu_si128(output.add(destination).cast(), first) };
        unsafe { _mm_storeu_si128(output.add(destination + 16).cast(), second) };
        unsafe { _mm_storeu_si128(output.add(destination + 32).cast(), third) };
        unsafe { _mm_storeu_si128(output.add(destination + 48).cast(), fourth) };

        source += 48;
        destination += 64;
    }

    // Loading a vector reads 16 bytes, so leave enough bytes for the load.
    while source + 16 <= input.len() {
        let encoded = unsafe { encode_12(input.as_ptr().add(source), offsets) };
        unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };

        source += 12;
        destination += 16;
    }

    source
}

#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode_wrapped<const URLSAFE: bool>(
    input: &[u8],
    output: &mut WrappedOutput,
) -> usize {
    unsafe { encode_wrapped_with_offsets(input, output, standard_offsets::<URLSAFE>()) }
}

#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode_wrapped_custom(
    input: &[u8],
    output: &mut WrappedOutput,
    offsets: &[i8; 16],
) -> usize {
    let offsets = unsafe { _mm_loadu_si128(offsets.as_ptr().cast()) };
    unsafe { encode_wrapped_with_offsets(input, output, offsets) }
}

#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
#[inline]
unsafe fn encode_wrapped_with_offsets(
    input: &[u8],
    output: &mut WrappedOutput,
    offsets: __m128i,
) -> usize {
    let mut source = 0;

    while source + 52 <= input.len() {
        for offset in [0, 12, 24, 36] {
            let encoded = unsafe { encode_12(input.as_ptr().add(source + offset), offsets) };
            unsafe { output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(encoded)) };
        }
        source += 48;
    }

    while source + 16 <= input.len() {
        let encoded = unsafe { encode_12(input.as_ptr().add(source), offsets) };
        unsafe { output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(encoded)) };
        source += 12;
    }

    source
}

#[target_feature(enable = "ssse3")]
unsafe fn encode_12(input: *const u8, offsets: __m128i) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);

    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = unsafe { mulhi_epu16_exact_ssse3(higher, _mm_set1_epi32(0x0400_0040)) };
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = _mm_mullo_epi16(lower, _mm_set1_epi32(0x0100_0010));

    ascii_from_indices(_mm_or_si128(higher, lower), offsets)
}

// LLVM can expand this constant multiply into a long widen/shift/pack
// sequence. Keep the single SSE2 instruction on the SSSE3 fallback path.
#[inline]
#[target_feature(enable = "ssse3")]
unsafe fn mulhi_epu16_exact_ssse3(mut value: __m128i, multiplier: __m128i) -> __m128i {
    unsafe {
        asm!(
            "pmulhuw {value}, {multiplier}",
            value = inout(xmm_reg) value,
            multiplier = in(xmm_reg) multiplier,
            options(pure, nomem, nostack)
        );
    }

    value
}

#[target_feature(enable = "ssse3")]
fn ascii_from_indices(indices: __m128i, offsets: __m128i) -> __m128i {
    let reduced = _mm_subs_epu8(indices, _mm_set1_epi8(51));
    let lower = _mm_cmpgt_epi8(indices, _mm_set1_epi8(25));
    let reduced = _mm_sub_epi8(reduced, lower);

    _mm_add_epi8(_mm_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "ssse3")]
fn standard_offsets<const URLSAFE: bool>() -> __m128i {
    _mm_setr_epi8(
        b'A' as i8,
        (b'a' - 26) as i8,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        -4,
        if URLSAFE { -17 } else { -19 },
        if URLSAFE { 32 } else { -16 },
        0,
        0,
    )
}
