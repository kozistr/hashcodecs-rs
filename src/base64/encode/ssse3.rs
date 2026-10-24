//! x86 SSSE3 encoding kernel.

use std::arch::asm;
#[cfg(target_arch = "x86")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86_64::*;

#[cfg(feature = "python")]
use super::WrappedOutput;

/// # Safety
/// The caller must reserve the complete padded encoded length in writable, non-overlapping output
/// storage. Architecture kernels additionally require their declared CPU features and any fallback
/// features.
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input loads,
    // and dispatch has enabled the required CPU features.
    unsafe { encode_with_offsets(input, output, standard_offsets::<URLSAFE>()) }
}

/// # Safety
/// The caller must reserve the complete padded encoded length in writable, non-overlapping output
/// storage. Architecture kernels additionally require their declared CPU features and any fallback
/// features.
#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode_custom(input: &[u8], output: *mut u8, offsets: &[i8; 16]) -> usize {
    // SAFETY: The fixed-size alphabet, offset, or shuffle table contains every byte of this vector
    // load; the kernel features are enabled.
    let offsets = unsafe { _mm_loadu_si128(offsets.as_ptr().cast()) };
    // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input loads,
    // and dispatch has enabled the required CPU features.
    unsafe { encode_with_offsets(input, output, offsets) }
}

/// # Safety
/// The caller must reserve the complete padded encoded length in writable, non-overlapping output
/// storage. Architecture kernels additionally require their declared CPU features and any fallback
/// features.
#[target_feature(enable = "ssse3")]
#[inline]
unsafe fn encode_with_offsets(input: &[u8], output: *mut u8, offsets: __m128i) -> usize {
    let mut source = 0;
    let mut destination = 0;

    while source + 52 <= input.len() {
        // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
        // loads, and dispatch has enabled the required CPU features.
        let first = unsafe { encode_12(input.as_ptr().add(source), offsets) };
        // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
        // loads, and dispatch has enabled the required CPU features.
        let second = unsafe { encode_12(input.as_ptr().add(source + 12), offsets) };
        // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
        // loads, and dispatch has enabled the required CPU features.
        let third = unsafe { encode_12(input.as_ptr().add(source + 24), offsets) };
        // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
        // loads, and dispatch has enabled the required CPU features.
        let fourth = unsafe { encode_12(input.as_ptr().add(source + 36), offsets) };

        // SAFETY: The encoded destination range fits the caller-provided capacity, and unaligned
        // stores impose no additional alignment requirement.
        unsafe { _mm_storeu_si128(output.add(destination).cast(), first) };
        // SAFETY: The encoded destination range fits the caller-provided capacity, and unaligned
        // stores impose no additional alignment requirement.
        unsafe { _mm_storeu_si128(output.add(destination + 16).cast(), second) };
        // SAFETY: The encoded destination range fits the caller-provided capacity, and unaligned
        // stores impose no additional alignment requirement.
        unsafe { _mm_storeu_si128(output.add(destination + 32).cast(), third) };
        // SAFETY: The encoded destination range fits the caller-provided capacity, and unaligned
        // stores impose no additional alignment requirement.
        unsafe { _mm_storeu_si128(output.add(destination + 48).cast(), fourth) };

        source += 48;
        destination += 64;
    }

    // Loading a vector reads 16 bytes, so leave enough bytes for the load.
    while source + 16 <= input.len() {
        // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
        // loads, and dispatch has enabled the required CPU features.
        let encoded = unsafe { encode_12(input.as_ptr().add(source), offsets) };
        // SAFETY: The encoded destination range fits the caller-provided capacity, and unaligned
        // stores impose no additional alignment requirement.
        unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };

        source += 12;
        destination += 16;
    }

    source
}

/// # Safety
/// The kernel CPU features and any fallback features must be available. WrappedOutput must retain
/// capacity for the encoded input and inserted separators without overlapping the input.
#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode_wrapped<const URLSAFE: bool>(
    input: &[u8],
    output: &mut WrappedOutput,
) -> usize {
    // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input loads,
    // and dispatch has enabled the required CPU features.
    unsafe { encode_wrapped_with_offsets(input, output, standard_offsets::<URLSAFE>()) }
}

/// # Safety
/// The kernel CPU features and any fallback features must be available. WrappedOutput must retain
/// capacity for the encoded input and inserted separators without overlapping the input.
#[cfg(feature = "python")]
#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn encode_wrapped_custom(
    input: &[u8],
    output: &mut WrappedOutput,
    offsets: &[i8; 16],
) -> usize {
    // SAFETY: The fixed-size alphabet, offset, or shuffle table contains every byte of this vector
    // load; the kernel features are enabled.
    let offsets = unsafe { _mm_loadu_si128(offsets.as_ptr().cast()) };
    // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input loads,
    // and dispatch has enabled the required CPU features.
    unsafe { encode_wrapped_with_offsets(input, output, offsets) }
}

/// # Safety
/// The kernel CPU features and any fallback features must be available. WrappedOutput must retain
/// capacity for the encoded input and inserted separators without overlapping the input.
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
            // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
            // loads, and dispatch has enabled the required CPU features.
            let encoded = unsafe { encode_12(input.as_ptr().add(source + offset), offsets) };
            // SAFETY: The vector and byte array have identical sizes and all bits are initialized;
            // the wrapped writer owns enough output capacity.
            unsafe { output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(encoded)) };
        }

        source += 48;
    }

    while source + 16 <= input.len() {
        // SAFETY: The caller reserves the encoded output capacity, loop bounds cover full input
        // loads, and dispatch has enabled the required CPU features.
        let encoded = unsafe { encode_12(input.as_ptr().add(source), offsets) };
        // SAFETY: The vector and byte array have identical sizes and all bits are initialized; the
        // wrapped writer owns enough output capacity.
        unsafe { output.write_16(core::mem::transmute::<__m128i, [u8; 16]>(encoded)) };
        source += 12;
    }

    source
}

/// # Safety
/// The kernel CPU features must be available and `input` must cover 16 readable bytes, including
/// the four bytes beyond the consumed group.
#[target_feature(enable = "ssse3")]
unsafe fn encode_12(input: *const u8, offsets: __m128i) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);

    // SAFETY: The caller supplies the full load width, or the tail mask selects only readable input
    // bytes; the kernel features are enabled.
    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = mulhi_epu16_exact(higher, _mm_set1_epi32(0x0400_0040));
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = _mm_mullo_epi16(lower, _mm_set1_epi32(0x0100_0010));

    ascii_from_indices(_mm_or_si128(higher, lower), offsets)
}

// LLVM can expand this constant multiply into a long widen/shift/pack
// sequence. Keep the single SSE2 instruction on the SSSE3 fallback path.
#[inline]
#[target_feature(enable = "ssse3")]
fn mulhi_epu16_exact(mut value: __m128i, multiplier: __m128i) -> __m128i {
    // SAFETY: The enabled instruction set supports this register-only operation; the assembly
    // touches no memory or stack.
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
