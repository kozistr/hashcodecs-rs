//! x86 Base64 decoding kernels and store policies.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::{Base64Error, MIXED_DECODE, STANDARD_DECODE, URLSAFE_DECODE};

mod avx512;

#[cfg(test)]
pub(in crate::base64) use avx512::DECODE_SHUFFLE as AVX512_DECODE_SHUFFLE;
pub(in crate::base64) use avx512::decode as decode_avx512;

pub(crate) struct StandardDecoder;
pub(crate) struct UrlSafeDecoder;
pub(crate) struct MixedDecoder;
pub(crate) struct ExactStore;
pub(crate) struct PaddedStore;

pub(crate) trait Decoder {
    fn decode_table() -> &'static [u8; 256];
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i);
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i);
}

pub(crate) trait Store {
    unsafe fn store_12(output: *mut u8, value: __m128i);
    unsafe fn store_24(output: *mut u8, value: __m256i);
}

impl Store for ExactStore {
    #[inline(always)]
    unsafe fn store_12(output: *mut u8, value: __m128i) {
        unsafe { store_12_exact(output, value) };
    }

    #[inline(always)]
    unsafe fn store_24(output: *mut u8, value: __m256i) {
        unsafe { store_24_exact(output, value) };
    }
}

impl Store for PaddedStore {
    #[inline(always)]
    unsafe fn store_12(output: *mut u8, value: __m128i) {
        unsafe { store_12_padded(output, value) };
    }

    #[inline(always)]
    unsafe fn store_24(output: *mut u8, value: __m256i) {
        unsafe { store_24_padded(output, value) };
    }
}

impl Decoder for StandardDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &STANDARD_DECODE
    }

    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_standard(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_standard(input) }
    }
}

impl Decoder for UrlSafeDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &URLSAFE_DECODE
    }

    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_urlsafe(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_urlsafe(input) }
    }
}

impl Decoder for MixedDecoder {
    #[inline(always)]
    fn decode_table() -> &'static [u8; 256] {
        &MIXED_DECODE
    }

    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_mixed(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_mixed(input) }
    }
}
#[target_feature(enable = "avx2")]
pub(crate) unsafe fn decode_avx2<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 128 <= input.len() {
        let (first, first_error) = unsafe { A::decode_32(input.as_ptr().add(source)) };
        let (second, second_error) = unsafe { A::decode_32(input.as_ptr().add(source + 32)) };
        let (third, third_error) = unsafe { A::decode_32(input.as_ptr().add(source + 64)) };
        let (fourth, fourth_error) = unsafe { A::decode_32(input.as_ptr().add(source + 96)) };
        let errors = _mm256_or_si256(
            _mm256_or_si256(first_error, second_error),
            _mm256_or_si256(third_error, fourth_error),
        );
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        // The padded stores write four bytes into the following block's output,
        // where the next store replaces them. This is safe for exact-output
        // callers because each of these blocks has a complete successor.
        unsafe { store_24_padded(output.add(destination), pack_32(first)) };
        unsafe { store_24_padded(output.add(destination + 24), pack_32(second)) };
        unsafe { store_24_padded(output.add(destination + 48), pack_32(third)) };
        if source + 160 <= input.len() {
            unsafe { store_24_padded(output.add(destination + 72), pack_32(fourth)) };
        } else {
            unsafe { S::store_24(output.add(destination + 72), pack_32(fourth)) };
        }
        source += 128;
        destination += 96;
    }
    while source + 32 <= input.len() {
        let (indices, errors) = unsafe { A::decode_32(input.as_ptr().add(source)) };
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        if source + 64 <= input.len() {
            // A complete following block provides enough in-bounds output for
            // the four-byte overlap, which that block replaces.
            unsafe { store_24_padded(output.add(destination), pack_32(indices)) };
        } else {
            unsafe { S::store_24(output.add(destination), pack_32(indices)) };
        }
        source += 32;
        destination += 24;
    }
    // At most one 16-byte block remains after the AVX2 loops. Decode it
    // directly so the bulk SSSE3 entry point does not sit on the AVX2 hot path.
    if source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3")]
pub(crate) unsafe fn decode_ssse3<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 64 <= input.len() {
        let (first, first_errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        let (second, second_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
        let (third, third_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
        let (fourth, fourth_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
        let errors = _mm_or_si128(
            _mm_or_si128(first_errors, second_errors),
            _mm_or_si128(third_errors, fourth_errors),
        );
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(first)) };
        unsafe { S::store_12(output.add(destination + 12), pack_16_indices(second)) };
        unsafe { S::store_12(output.add(destination + 24), pack_16_indices(third)) };
        unsafe { S::store_12(output.add(destination + 36), pack_16_indices(fourth)) };
        source += 64;
        destination += 48;
    }
    while source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if !errors_are_zero_ssse3(errors) {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3,sse4.1")]
pub(crate) unsafe fn decode_sse41<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 64 <= input.len() {
        let (first, first_errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        let (second, second_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
        let (third, third_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
        let (fourth, fourth_errors) =
            unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
        let errors = _mm_or_si128(
            _mm_or_si128(first_errors, second_errors),
            _mm_or_si128(third_errors, fourth_errors),
        );
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(first)) };
        unsafe { S::store_12(output.add(destination + 12), pack_16_indices(second)) };
        unsafe { S::store_12(output.add(destination + 24), pack_16_indices(third)) };
        unsafe { S::store_12(output.add(destination + 36), pack_16_indices(fourth)) };
        source += 64;
        destination += 48;
    }
    while source + 16 <= input.len() {
        let (indices, errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}
#[target_feature(enable = "ssse3")]
unsafe fn decode_indices_16_standard(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
        0x10,
    );
    let low_classes = _mm_setr_epi8(
        0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, 0x1a, 0x1b, 0x1b, 0x1b,
        0x1a,
    );
    let (high_nibbles, errors) = classify_ascii_ssse3(value, high_classes, low_classes);
    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    let offsets = _mm_setr_epi8(0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
    (
        _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices)),
        errors,
    )
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_indices_16_urlsafe(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3b, 0x3b, 0x3a, 0x3b,
        0x33,
    );
    let (high_nibbles, errors) = classify_ascii_ssse3(value, high_classes, low_classes);
    let offsets = _mm_setr_epi8(0, 0, 17, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
    let indices = _mm_add_epi8(value, _mm_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let correction = _mm_and_si128(underscore, _mm_set1_epi8(33));
    (_mm_add_epi8(indices, correction), errors)
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_indices_16_mixed(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3a, 0x3b, 0x3a, 0x3b,
        0x32,
    );
    let (high_nibbles, errors) = classify_ascii_ssse3(value, high_classes, low_classes);
    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    let offsets = _mm_setr_epi8(0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
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
fn classify_ascii_ssse3(
    value: __m128i,
    high_classes: __m128i,
    low_classes: __m128i,
) -> (__m128i, __m128i) {
    // Invalid high/low nibble pairs share a class bit; valid pairs produce zero.
    let mask = _mm_set1_epi8(0x0f);
    let high_nibbles = _mm_and_si128(_mm_srli_epi16(value, 4), mask);
    let low_nibbles = _mm_and_si128(value, mask);
    let high_matches = _mm_shuffle_epi8(high_classes, high_nibbles);
    let low_matches = _mm_shuffle_epi8(low_classes, low_nibbles);
    (high_nibbles, _mm_and_si128(high_matches, low_matches))
}

#[target_feature(enable = "ssse3")]
fn errors_are_zero_ssse3(errors: __m128i) -> bool {
    _mm_movemask_epi8(_mm_cmpeq_epi8(errors, _mm_setzero_si128())) == 0xffff
}

#[target_feature(enable = "ssse3")]
fn pack_16_indices(indices: __m128i) -> __m128i {
    let merged = _mm_maddubs_epi16(indices, _mm_set1_epi32(0x0140_0140));
    let packed = _mm_madd_epi16(merged, _mm_set1_epi32(0x0001_1000));
    let shuffle = _mm_setr_epi8(2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1);
    _mm_shuffle_epi8(packed, shuffle)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_standard(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x2a, 0x2b, 0x2b, 0x2b,
        0x2a,
    );
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let slash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let offset_indices = _mm256_add_epi8(high_nibbles, slash);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
    (
        _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, offset_indices)),
        errors,
    )
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_urlsafe(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3b, 0x3b, 0x3a, 0x3b,
        0x33,
    );
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 0, 17, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
    let indices = _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let correction = _mm256_and_si256(underscore, _mm256_set1_epi8(33));
    (_mm256_add_epi8(indices, correction), errors)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_mixed(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_classes = _mm_setr_epi8(
        0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
        0x20,
    );
    let low_classes = _mm_setr_epi8(
        0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3a, 0x3b, 0x3a, 0x3b,
        0x32,
    );
    let (high_nibbles, errors) = classify_ascii_avx2(value, high_classes, low_classes);
    let slash = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let offset_indices = _mm256_add_epi8(high_nibbles, slash);
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
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
    // Invalid high/low nibble pairs share a class bit; valid pairs produce zero.
    let mask = _mm256_set1_epi8(0x0f);
    let high_nibbles = _mm256_and_si256(_mm256_srli_epi16(value, 4), mask);
    let low_nibbles = _mm256_and_si256(value, mask);
    let high_matches = _mm256_shuffle_epi8(_mm256_broadcastsi128_si256(high_classes), high_nibbles);
    let low_matches = _mm256_shuffle_epi8(_mm256_broadcastsi128_si256(low_classes), low_nibbles);
    (high_nibbles, _mm256_and_si256(high_matches, low_matches))
}

#[target_feature(enable = "avx2")]
fn pack_32(indices: __m256i) -> __m256i {
    let merged = _mm256_maddubs_epi16(indices, _mm256_set1_epi32(0x0140_0140));
    let packed = _mm256_madd_epi16(merged, _mm256_set1_epi32(0x0001_1000));
    let shuffle = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1,
    ));
    _mm256_shuffle_epi8(packed, shuffle)
}

#[target_feature(enable = "ssse3")]
unsafe fn store_12_exact(output: *mut u8, value: __m128i) {
    unsafe { _mm_storel_epi64(output.cast(), value) };
    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(value, 8));
    unsafe { output.add(8).cast::<i32>().write_unaligned(remaining) };
}

#[target_feature(enable = "avx2")]
unsafe fn store_24_exact(output: *mut u8, value: __m256i) {
    let lower = _mm256_castsi256_si128(value);
    let upper = _mm256_extracti128_si256(value, 1);
    // The first store's four lane-padding bytes are replaced by the second.
    unsafe { _mm_storeu_si128(output.cast(), lower) };
    unsafe { _mm_storel_epi64(output.add(12).cast(), upper) };
    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(upper, 8));
    unsafe { output.add(20).cast::<i32>().write_unaligned(remaining) };
}

#[target_feature(enable = "ssse3")]
unsafe fn store_12_padded(output: *mut u8, value: __m128i) {
    unsafe { _mm_storeu_si128(output.cast(), value) };
}

#[target_feature(enable = "avx2")]
unsafe fn store_24_padded(output: *mut u8, value: __m256i) {
    unsafe { _mm_storeu_si128(output.cast(), _mm256_castsi256_si128(value)) };
    unsafe { _mm_storeu_si128(output.add(12).cast(), _mm256_extracti128_si256(value, 1)) };
}
