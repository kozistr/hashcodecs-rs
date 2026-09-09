//! AVX2 decoding kernel.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::ssse3::{errors_are_zero_ssse3, pack_16_indices, store_12_exact};
use super::tables::{
    MIXED_LOW_CLASSES_COMPLEMENT, PACK_SHUFFLE, STANDARD_HIGH_CLASSES,
    STANDARD_LOW_CLASSES_COMPLEMENT, STANDARD_OFFSETS, URLSAFE_HIGH_CLASSES,
    URLSAFE_LOW_CLASSES_COMPLEMENT, URLSAFE_OFFSETS,
};
use super::x86_contracts::{Decoder, Store};

/// TODO
/// So my optimization priority would be:
/// 1. Split bulk and terminal blocks, eliminating the per-iteration padded-store branches.
/// 2. Cache input_ptr / input_len.
/// 3. Inspect assembly for spills caused by the 4× unroll.
/// 4. Benchmark 2× versus 4× unrolling.
/// 5. Leave the combined error reduction alone unless profiling says otherwise.
///    I'd inspect cargo asm/Compiler Explorer output for spills and benchmark 2× vs 4×.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn decode_avx2<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    // Half-cache-line output starts favor one packed 256-bit store, while
    // cache-line-aligned buffers remain faster with the two narrow stores.
    let use_wide_overlapping_stores = output.addr() & 63 == 32;

    while source + 128 <= input.len() {
        let (first, first_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        let (second, second_error) =
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 32)) };
        let (third, third_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 64)) };
        let (fourth, fourth_error) =
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 96)) };

        let errors = _mm256_or_si256(
            _mm256_or_si256(first_error, second_error),
            _mm256_or_si256(third_error, fourth_error),
        );
        if !A::accepts_errors(_mm256_testz_si256(errors, errors) != 0) {
            return Err(Base64Error::InvalidInput);
        }

        if use_wide_overlapping_stores {
            unsafe { store_24_padded_wide(output.add(destination), pack_32(first)) };
            unsafe { store_24_padded_wide(output.add(destination + 24), pack_32(second)) };
            unsafe { store_24_padded_wide(output.add(destination + 48), pack_32(third)) };
        } else {
            unsafe { store_24_padded(output.add(destination), pack_32(first)) };
            unsafe { store_24_padded(output.add(destination + 24), pack_32(second)) };
            unsafe { store_24_padded(output.add(destination + 48), pack_32(third)) };
        }
        unsafe { S::store_24(output.add(destination + 72), pack_32(fourth)) };

        source += 128;
        destination += 96;
    }

    while source + 32 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        if !A::accepts_errors(_mm256_testz_si256(errors, errors) != 0) {
            return Err(Base64Error::InvalidInput);
        }

        unsafe { S::store_24(output.add(destination), pack_32(indices)) };

        source += 32;
        destination += 24;
    }

    // At most one 16-byte block remains after the AVX2 loops. Decode this block here.
    // This keeps the bulk SSSE3 entry point off the AVX2 hot path.
    if source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if !A::accepts_errors(errors_are_zero_ssse3(errors)) {
            return Err(Base64Error::InvalidInput);
        }

        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };

        source += 16;
        destination += 12;
    }

    Ok((source, destination))
}

#[target_feature(enable = "avx2")]
pub(crate) unsafe fn validate<A: Decoder>(input: &[u8]) -> Result<usize, Base64Error> {
    let mut source = 0;

    while source + 128 <= input.len() {
        let (_, first) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        let (_, second) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 32)) };
        let (_, third) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 64)) };
        let (_, fourth) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 96)) };

        let errors = _mm256_or_si256(
            _mm256_or_si256(first, second),
            _mm256_or_si256(third, fourth),
        );
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }

        source += 128;
    }
    while source + 32 <= input.len() {
        let (_, errors) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }

        source += 32;
    }

    if source + 16 <= input.len() {
        let (_, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }

        source += 16;
    }

    Ok(source)
}

#[target_feature(enable = "avx2")]
pub(crate) unsafe fn decode_prefix_avx2<A: Decoder>(
    input: &[u8],
    output: *mut u8,
) -> (usize, usize) {
    let mut source = 0;
    let mut destination = 0;
    // Match `decode_avx2`'s store shape after this group validates.
    let use_wide_overlapping_stores = output.addr() & 63 == 32;

    while source + 128 <= input.len() {
        let (first, first_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        let (second, second_error) =
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 32)) };
        let (third, third_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 64)) };
        let (fourth, fourth_error) =
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 96)) };

        let errors = _mm256_or_si256(
            _mm256_or_si256(first_error, second_error),
            _mm256_or_si256(third_error, fourth_error),
        );
        if _mm256_testz_si256(errors, errors) == 0 {
            break;
        }

        // All four blocks are valid. Each following store replaces the
        // preceding overlap; the final exact store bounds writes to this prefix.
        if use_wide_overlapping_stores {
            unsafe { store_24_padded_wide(output.add(destination), pack_32(first)) };
            unsafe { store_24_padded_wide(output.add(destination + 24), pack_32(second)) };
            unsafe { store_24_padded_wide(output.add(destination + 48), pack_32(third)) };
        } else {
            unsafe { store_24_padded(output.add(destination), pack_32(first)) };
            unsafe { store_24_padded(output.add(destination + 24), pack_32(second)) };
            unsafe { store_24_padded(output.add(destination + 48), pack_32(third)) };
        }
        unsafe { store_24_exact(output.add(destination + 72), pack_32(fourth)) };

        source += 128;
        destination += 96;
    }

    while source + 32 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        if _mm256_testz_si256(errors, errors) == 0 {
            break;
        }

        unsafe { store_24_exact(output.add(destination), pack_32(indices)) };

        source += 32;
        destination += 24;
    }
    if source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if errors_are_zero_ssse3(errors) {
            unsafe { store_12_exact(output.add(destination), pack_16_indices(indices)) };

            source += 16;
            destination += 12;
        }
    }

    (source, destination)
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_indices_32_standard(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = unsafe { _mm_loadu_si128(STANDARD_HIGH_CLASSES.as_ptr().cast()) };
    let low_classes = unsafe { _mm_loadu_si128(STANDARD_LOW_CLASSES_COMPLEMENT.as_ptr().cast()) };

    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);

    (translate_standard(value, high_nibbles), errors)
}

#[target_feature(enable = "avx2")]
#[inline]
#[cfg(any(feature = "python", test))]
pub(super) unsafe fn decode_indices_32_standard_validated(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_nibbles = high_nibbles(value);

    (
        translate_standard(value, high_nibbles),
        _mm256_setzero_si256(),
    )
}

#[target_feature(enable = "avx2")]
fn translate_standard(value: __m256i, high_nibbles: __m256i) -> __m256i {
    let slash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let offset_indices = _mm256_add_epi8(high_nibbles, slash);
    let offsets =
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(STANDARD_OFFSETS.as_ptr().cast()) });

    _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices))
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_indices_32_urlsafe(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = unsafe { _mm_loadu_si128(URLSAFE_HIGH_CLASSES.as_ptr().cast()) };
    let low_classes = unsafe { _mm_loadu_si128(URLSAFE_LOW_CLASSES_COMPLEMENT.as_ptr().cast()) };
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);

    let offsets =
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(URLSAFE_OFFSETS.as_ptr().cast()) });

    let indices = _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let correction = _mm256_and_si256(underscore, _mm256_set1_epi8(33));

    (_mm256_add_epi8(indices, correction), errors)
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_indices_32_mixed(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = unsafe { _mm_loadu_si128(URLSAFE_HIGH_CLASSES.as_ptr().cast()) };
    let low_classes = unsafe { _mm_loadu_si128(MIXED_LOW_CLASSES_COMPLEMENT.as_ptr().cast()) };

    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let slash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let offset_indices = _mm256_add_epi8(high_nibbles, slash);
    let offsets =
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(STANDARD_OFFSETS.as_ptr().cast()) });
    let indices = _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices));
    let dash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'-' as i8));
    let underscore = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let corrections = _mm256_or_si256(
        _mm256_and_si256(dash, _mm256_set1_epi8(-2)),
        _mm256_and_si256(underscore, _mm256_set1_epi8(33)),
    );

    (_mm256_add_epi8(indices, corrections), errors)
}

#[target_feature(enable = "avx2")]
fn classify_ascii_avx2(
    value: __m256i,
    high_classes: __m128i,
    low_classes: __m128i,
) -> (__m256i, __m256i) {
    // Invalid high/low nibble pairs share a class bit. High-bit bytes make the
    // raw low-byte shuffle return zero, leaving the invalid high-class guard.
    let high_nibbles = high_nibbles(value);
    let high_matches = _mm256_shuffle_epi8(_mm256_broadcastsi128_si256(high_classes), high_nibbles);
    let low_mismatches = _mm256_shuffle_epi8(_mm256_broadcastsi128_si256(low_classes), value);

    (
        high_nibbles,
        _mm256_andnot_si256(low_mismatches, high_matches),
    )
}

#[target_feature(enable = "avx2")]
fn high_nibbles(value: __m256i) -> __m256i {
    _mm256_and_si256(_mm256_srli_epi16(value, 4), _mm256_set1_epi8(0x0f))
}

#[target_feature(enable = "avx2")]
fn pack_32(indices: __m256i) -> __m256i {
    let merged = _mm256_maddubs_epi16(indices, _mm256_set1_epi32(0x0140_0140));
    let packed = _mm256_madd_epi16(merged, _mm256_set1_epi32(0x0001_1000));

    let shuffle =
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(PACK_SHUFFLE.as_ptr().cast()) });

    _mm256_shuffle_epi8(packed, shuffle)
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn store_24_exact(output: *mut u8, value: __m256i) {
    let lower = _mm256_castsi256_si128(value);
    let upper = _mm256_extracti128_si256(value, 1);

    // The second store replaces the first store's four lane-padding bytes.
    unsafe { _mm_storeu_si128(output.cast(), lower) };
    unsafe { _mm_storel_epi64(output.add(12).cast(), upper) };

    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(upper, 8));

    unsafe { output.add(20).cast::<i32>().write_unaligned(remaining) };
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn store_24_padded(output: *mut u8, value: __m256i) {
    unsafe { _mm_storeu_si128(output.cast(), _mm256_castsi256_si128(value)) };
    unsafe { _mm_storeu_si128(output.add(12).cast(), _mm256_extracti128_si256(value, 1)) };
}

#[inline(always)]
unsafe fn store_24_padded_wide(output: *mut u8, value: __m256i) {
    // Close the four-byte gap between 128-bit lanes, then let the following
    // store replace the eight padding bytes at the end.
    let packed =
        unsafe { _mm256_permutevar8x32_epi32(value, _mm256_setr_epi32(0, 1, 2, 4, 5, 6, 7, 7)) };
    unsafe { _mm256_storeu_si256(output.cast(), packed) };
}
