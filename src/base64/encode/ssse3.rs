//! x86 SSSE3 encoding kernel.

use std::arch::asm;
#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    let mut source = 0;
    let mut destination = 0;

    while source + 52 <= input.len() {
        let first = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source)) };
        let second = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source + 12)) };
        let third = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source + 24)) };
        let fourth = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source + 36)) };

        unsafe { _mm_storeu_si128(output.add(destination).cast(), first) };
        unsafe { _mm_storeu_si128(output.add(destination + 16).cast(), second) };
        unsafe { _mm_storeu_si128(output.add(destination + 32).cast(), third) };
        unsafe { _mm_storeu_si128(output.add(destination + 48).cast(), fourth) };

        source += 48;
        destination += 64;
    }

    // Loading a vector reads 16 bytes, so leave enough bytes for the load.
    while source + 16 <= input.len() {
        let encoded = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source)) };
        unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };

        source += 12;
        destination += 16;
    }

    source
}

#[target_feature(enable = "ssse3")]
unsafe fn encode_12<const URLSAFE: bool>(input: *const u8) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);

    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = unsafe { mulhi_epu16_exact_ssse3(higher, _mm_set1_epi32(0x0400_0040)) };
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = _mm_mullo_epi16(lower, _mm_set1_epi32(0x0100_0010));

    ascii_from_indices::<URLSAFE>(_mm_or_si128(higher, lower))
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
fn ascii_from_indices<const URLSAFE: bool>(indices: __m128i) -> __m128i {
    let reduced = _mm_subs_epu8(indices, _mm_set1_epi8(51));
    let lower = _mm_cmpgt_epi8(indices, _mm_set1_epi8(25));
    let reduced = _mm_sub_epi8(reduced, lower);
    let offsets = _mm_setr_epi8(
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
    );

    _mm_add_epi8(_mm_shuffle_epi8(offsets, reduced), indices)
}
