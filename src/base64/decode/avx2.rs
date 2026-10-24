//! AVX2 decoding kernel.

#[cfg(target_arch = "x86")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
#[allow(clippy::wildcard_imports, reason = "architecture intrinsic vocabulary")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::ssse3::{self, pack_16_indices, store_12_exact};
use super::tables::{
    MIXED_HASH_OFFSETS, MIXED_LOW_CLASSES_COMPLEMENT, MIXED_SHIFTS, PACK_SHUFFLE,
    STANDARD_HIGH_CLASSES, STANDARD_LOW_CLASSES_COMPLEMENT, STANDARD_OFFSETS, URLSAFE_HIGH_CLASSES,
    URLSAFE_LOW_CLASSES_COMPLEMENT,
};
use super::x86_contracts::{Decoder, Store};

/// # Safety
/// The checked layout must describe this input, and output must cover its decoded length without
/// aliasing input. SIMD entry points additionally require their CPU features and the full exact or
/// padded store width. Validated paths require a checked alphabet.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn decode<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    // Half-cache-line output starts favor one packed 256-bit store, while
    // cache-line-aligned buffers remain faster with the two narrow stores.
    let use_wide_overlapping_stores = output.addr() & 63 == 32;

    while source + 128 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (first, first_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        let (second, second_error) =
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 32)) };
        let errors = _mm256_or_si256(first_error, second_error);
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (third, third_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 64)) };
        let errors = _mm256_or_si256(errors, third_error);
        let (fourth, fourth_error) =
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 96)) };
        let errors = _mm256_or_si256(errors, fourth_error);

        if A::rejects_input(_mm256_testz_si256(errors, errors) == 0) {
            return Err(Base64Error::InvalidInput);
        }

        if use_wide_overlapping_stores {
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded_wide(output.add(destination), pack_32(first)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded_wide(output.add(destination + 24), pack_32(second)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded_wide(output.add(destination + 48), pack_32(third)) };
        } else {
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded(output.add(destination), pack_32(first)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded(output.add(destination + 24), pack_32(second)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded(output.add(destination + 48), pack_32(third)) };
        }

        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        unsafe { S::store_24(output.add(destination + 72), pack_32(fourth)) };

        source += 128;
        destination += 96;
    }

    while source + 32 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (indices, errors) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };

        if A::rejects_input(_mm256_testz_si256(errors, errors) == 0) {
            return Err(Base64Error::InvalidInput);
        }

        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        unsafe { S::store_24(output.add(destination), pack_32(indices)) };

        source += 32;
        destination += 24;
    }

    // At most one 16-byte block remains after the AVX2 loops. Decode this block here.
    // This keeps the bulk SSSE3 entry point off the AVX2 hot path.
    if source + 16 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };

        if A::rejects_input(ssse3::has_errors(errors)) {
            return Err(Base64Error::InvalidInput);
        }

        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };

        source += 16;
        destination += 12;
    }

    Ok((source, destination))
}

#[target_feature(enable = "avx2")]
pub(crate) fn validate<A: Decoder>(input: &[u8]) -> Result<usize, Base64Error> {
    let mut source = 0;

    while source + 128 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (_, first) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (_, second) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 32)) };
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (_, third) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 64)) };
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
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
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (_, errors) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };

        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }

        source += 32;
    }

    if source + 16 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (_, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };

        if ssse3::has_errors(errors) {
            return Err(Base64Error::InvalidInput);
        }

        source += 16;
    }

    Ok(source)
}

/// # Safety
/// The checked layout must describe this input, and output must cover its decoded length without
/// aliasing input. SIMD entry points additionally require their CPU features and the full exact or
/// padded store width. Validated paths require a checked alphabet.
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn decode_prefix<A: Decoder>(input: &[u8], output: *mut u8) -> (usize, usize) {
    let mut source = 0;
    let mut destination = 0;
    // Match `decode`'s store shape after this group validates.
    let use_wide_overlapping_stores = output.addr() & 63 == 32;

    while source + 128 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (first, first_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };
        let (second, second_error) =
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 32)) };
        let errors = _mm256_or_si256(first_error, second_error);
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (third, third_error) = unsafe { A::decode_indices_32(input.as_ptr().add(source + 64)) };
        let errors = _mm256_or_si256(errors, third_error);
        let (fourth, fourth_error) =
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { A::decode_indices_32(input.as_ptr().add(source + 96)) };
        let errors = _mm256_or_si256(errors, fourth_error);

        if _mm256_testz_si256(errors, errors) == 0 {
            break;
        }

        // All four blocks are valid. Each following store replaces the
        // preceding overlap; the final exact store bounds writes to this prefix.
        if use_wide_overlapping_stores {
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded_wide(output.add(destination), pack_32(first)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded_wide(output.add(destination + 24), pack_32(second)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded_wide(output.add(destination + 48), pack_32(third)) };
        } else {
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded(output.add(destination), pack_32(first)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded(output.add(destination + 24), pack_32(second)) };
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_24_padded(output.add(destination + 48), pack_32(third)) };
        }

        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        unsafe { store_24_exact(output.add(destination + 72), pack_32(fourth)) };

        source += 128;
        destination += 96;
    }

    while source + 32 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (indices, errors) = unsafe { A::decode_indices_32(input.as_ptr().add(source)) };

        if _mm256_testz_si256(errors, errors) == 0 {
            break;
        }

        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        unsafe { store_24_exact(output.add(destination), pack_32(indices)) };

        source += 32;
        destination += 24;
    }

    if source + 16 <= input.len() {
        // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
        // decoded capacity required by the store policy and enables the kernel features.
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };

        if !ssse3::has_errors(errors) {
            // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves the
            // decoded capacity required by the store policy and enables the kernel features.
            unsafe { store_12_exact(output.add(destination), pack_16_indices(indices)) };

            source += 16;
            destination += 12;
        }
    }

    (source, destination)
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 32 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_indices_32_standard(input: *const u8) -> (__m256i, __m256i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let high_classes = unsafe { _mm_loadu_si128(STANDARD_HIGH_CLASSES.as_ptr().cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let low_classes = unsafe { _mm_loadu_si128(STANDARD_LOW_CLASSES_COMPLEMENT.as_ptr().cast()) };

    let (high_nibbles, errors) = classify_ascii(value, high_classes, low_classes);

    (translate_standard(value, high_nibbles), errors)
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 32 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "avx2")]
#[inline]
#[cfg(any(feature = "python", test))]
pub(super) unsafe fn decode_indices_32_standard_validated(input: *const u8) -> (__m256i, __m256i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
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
        // SAFETY: The fixed-size classification or shuffle table contains the full vector being
        // loaded; the kernel features are enabled.
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(STANDARD_OFFSETS.as_ptr().cast()) });

    _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices))
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 32 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_indices_32_urlsafe(input: *const u8) -> (__m256i, __m256i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let high_classes = unsafe { _mm_loadu_si128(URLSAFE_HIGH_CLASSES.as_ptr().cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let low_classes = unsafe { _mm_loadu_si128(URLSAFE_LOW_CLASSES_COMPLEMENT.as_ptr().cast()) };
    let (high_nibbles, errors) = classify_ascii(value, high_classes, low_classes);

    (translate_mixed(value, high_nibbles), errors)
}

/// # Safety
/// The caller must enable the kernel CPU features and provide 32 readable input bytes. Validated
/// variants require previously checked alphabet symbols.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_indices_32_mixed(input: *const u8) -> (__m256i, __m256i) {
    // SAFETY: The caller supplies the full readable vector width, or a mask bounds the tail; the
    // kernel's target features are enabled.
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let high_classes = unsafe { _mm_loadu_si128(URLSAFE_HIGH_CLASSES.as_ptr().cast()) };
    // SAFETY: The fixed-size classification or shuffle table contains the full vector being loaded;
    // the kernel features are enabled.
    let low_classes = unsafe { _mm_loadu_si128(MIXED_LOW_CLASSES_COMPLEMENT.as_ptr().cast()) };

    let (high_nibbles, errors) = classify_ascii(value, high_classes, low_classes);
    (translate_mixed(value, high_nibbles), errors)
}

#[target_feature(enable = "avx2")]
fn translate_mixed(value: __m256i, high_nibbles: __m256i) -> __m256i {
    let shifts =
        // SAFETY: The fixed-size classification or shuffle table contains the full vector being
        // loaded; the kernel features are enabled.
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(MIXED_SHIFTS.as_ptr().cast()) });
    let offsets =
        // SAFETY: The fixed-size classification or shuffle table contains the full vector being
        // loaded; the kernel features are enabled.
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(MIXED_HASH_OFFSETS.as_ptr().cast()) });
    let offset_indices = _mm256_add_epi8(high_nibbles, _mm256_shuffle_epi8(shifts, value));
    _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices))
}

#[target_feature(enable = "avx2")]
fn classify_ascii(
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
        // SAFETY: The fixed-size classification or shuffle table contains the full vector being
        // loaded; the kernel features are enabled.
        _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(PACK_SHUFFLE.as_ptr().cast()) });

    _mm256_shuffle_epi8(packed, shuffle)
}

/// # Safety
/// The kernel CPU features must be available and `output` must cover 24 writable bytes.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn store_24_exact(output: *mut u8, value: __m256i) {
    let lower = _mm256_castsi256_si128(value);
    let upper = _mm256_extracti128_si256(value, 1);

    // The second store replaces the first store's four lane-padding bytes.
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm_storeu_si128(output.cast(), lower) };
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm_storel_epi64(output.add(12).cast(), upper) };

    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(upper, 8));

    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { output.add(20).cast::<i32>().write_unaligned(remaining) };
}

/// # Safety
/// The kernel CPU features must be available and `output` must cover 28 writable bytes.
#[target_feature(enable = "avx2")]
pub(super) unsafe fn store_24_padded(output: *mut u8, value: __m256i) {
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm_storeu_si128(output.cast(), _mm256_castsi256_si128(value)) };
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm_storeu_si128(output.add(12).cast(), _mm256_extracti128_si256(value, 1)) };
}

/// # Safety
/// The kernel CPU features must be available and `output` must cover 32 writable bytes.
#[inline(always)]
unsafe fn store_24_padded_wide(output: *mut u8, value: __m256i) {
    // Duplicate payload in the padding to avoid a zero-fill blend after the
    // lane permutation. The following store replaces all eight padding bytes.
    let packed =
        // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
        // kernel supports these stores.
        unsafe { _mm256_permutevar8x32_epi32(value, _mm256_setr_epi32(0, 1, 2, 4, 5, 6, 0, 1)) };
    // SAFETY: The caller reserves the helper's exact or padded destination width; the enabled
    // kernel supports these stores.
    unsafe { _mm256_storeu_si256(output.cast(), packed) };
}
