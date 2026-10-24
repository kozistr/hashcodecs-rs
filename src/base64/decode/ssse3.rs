//! SSSE3 decoding kernel.

#[cfg(target_arch = "x86")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::sse_kernels::decode_kernels;
use super::tables::{
    MIXED_LOW_CLASSES, PACK_SHUFFLE, STANDARD_HIGH_CLASSES, STANDARD_LOW_CLASSES, STANDARD_OFFSETS,
    URLSAFE_HIGH_CLASSES, URLSAFE_LOW_CLASSES, URLSAFE_OFFSETS,
};
use super::x86_contracts::{Decoder, Store};

decode_kernels!(
    "ssse3",
    errors => has_errors(errors)
);

/// # Safety
/// The caller must enable the kernel CPU features and provide 16 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_indices_16_standard(input: *const u8) -> (__m128i, __m128i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let high_classes = unsafe { _mm_loadu_si128(STANDARD_HIGH_CLASSES.as_ptr().cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let low_classes = unsafe { _mm_loadu_si128(STANDARD_LOW_CLASSES.as_ptr().cast()) };
    let (high_nibbles, errors) = classify_ascii(value, high_classes, low_classes);

    (translate_standard(value, high_nibbles), errors)
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 16 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "ssse3")]
#[inline]
#[cfg(any(feature = "python", test))]
pub(super) unsafe fn decode_indices_16_standard_validated(input: *const u8) -> (__m128i, __m128i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_nibbles = high_nibbles(value);

    (translate_standard(value, high_nibbles), _mm_setzero_si128())
}

#[target_feature(enable = "ssse3")]
fn translate_standard(value: __m128i, high_nibbles: __m128i) -> __m128i {
    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let offsets = unsafe { _mm_loadu_si128(STANDARD_OFFSETS.as_ptr().cast()) };

    _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices))
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 16 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_indices_16_urlsafe(input: *const u8) -> (__m128i, __m128i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let high_classes = unsafe { _mm_loadu_si128(URLSAFE_HIGH_CLASSES.as_ptr().cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let low_classes = unsafe { _mm_loadu_si128(URLSAFE_LOW_CLASSES.as_ptr().cast()) };
    let (high_nibbles, errors) = classify_ascii(value, high_classes, low_classes);
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let offsets = unsafe { _mm_loadu_si128(URLSAFE_OFFSETS.as_ptr().cast()) };
    let indices = _mm_add_epi8(value, _mm_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let correction = _mm_and_si128(underscore, _mm_set1_epi8(33));

    (_mm_add_epi8(indices, correction), errors)
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 16 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_indices_16_mixed(input: *const u8) -> (__m128i, __m128i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let high_classes = unsafe { _mm_loadu_si128(URLSAFE_HIGH_CLASSES.as_ptr().cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let low_classes = unsafe { _mm_loadu_si128(MIXED_LOW_CLASSES.as_ptr().cast()) };
    let (high_nibbles, errors) = classify_ascii(value, high_classes, low_classes);
    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let offsets = unsafe { _mm_loadu_si128(STANDARD_OFFSETS.as_ptr().cast()) };
    let indices = _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices));
    let dash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let corrections = _mm_or_si128(
        _mm_and_si128(dash, _mm_set1_epi8(-2)),
        _mm_and_si128(underscore, _mm_set1_epi8(33)),
    );

    (_mm_add_epi8(indices, corrections), errors)
}

#[target_feature(enable = "ssse3")]
pub(super) fn classify_ascii(
    value: __m128i,
    high_classes: __m128i,
    low_classes: __m128i,
) -> (__m128i, __m128i) {
    // Invalid high/low nibble pairs share a class bit. Valid pairs produce zero.
    let high_nibbles = high_nibbles(value);
    let low_nibbles = _mm_and_si128(value, _mm_set1_epi8(0x0f));
    let high_matches = _mm_shuffle_epi8(high_classes, high_nibbles);
    let low_matches = _mm_shuffle_epi8(low_classes, low_nibbles);

    (high_nibbles, _mm_and_si128(high_matches, low_matches))
}

#[target_feature(enable = "ssse3")]
fn high_nibbles(value: __m128i) -> __m128i {
    _mm_and_si128(_mm_srli_epi16(value, 4), _mm_set1_epi8(0x0f))
}

#[target_feature(enable = "ssse3")]
pub(super) fn has_errors(errors: __m128i) -> bool {
    _mm_movemask_epi8(_mm_cmpeq_epi8(errors, _mm_setzero_si128())) != 0xffff
}

#[target_feature(enable = "ssse3")]
pub(super) fn pack_16_indices(indices: __m128i) -> __m128i {
    let merged = _mm_maddubs_epi16(indices, _mm_set1_epi32(0x0140_0140));
    let packed = _mm_madd_epi16(merged, _mm_set1_epi32(0x0001_1000));
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let shuffle = unsafe { _mm_loadu_si128(PACK_SHUFFLE.as_ptr().cast()) };

    _mm_shuffle_epi8(packed, shuffle)
}

/// # Safety
/// The kernel CPU features must be available and `output` must cover 12 writable bytes.
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn store_12_exact(output: *mut u8, value: __m128i) {
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm_storel_epi64(output.cast(), value) };
    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(value, 8));
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { output.add(8).cast::<i32>().write_unaligned(remaining) };
}

/// # Safety
/// The kernel CPU features must be available and `output` must cover 16 writable bytes.
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn store_12_padded(output: *mut u8, value: __m128i) {
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm_storeu_si128(output.cast(), value) };
}
