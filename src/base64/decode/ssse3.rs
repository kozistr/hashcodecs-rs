//! SSSE3 decoding kernel.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::Base64Error;
use super::tables::{PACK_SHUFFLE, STANDARD_OFFSETS, URLSAFE_OFFSETS};
use super::x86_contracts::{Decoder, Store};

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
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_indices_16_standard(input: *const u8) -> (__m128i, __m128i) {
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
    let offsets = unsafe { _mm_loadu_si128(STANDARD_OFFSETS.as_ptr().cast()) };
    (
        _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices)),
        errors,
    )
}

#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_indices_16_urlsafe(input: *const u8) -> (__m128i, __m128i) {
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
    let offsets = unsafe { _mm_loadu_si128(URLSAFE_OFFSETS.as_ptr().cast()) };
    let indices = _mm_add_epi8(value, _mm_shuffle_epi8(offsets, high_nibbles));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let correction = _mm_and_si128(underscore, _mm_set1_epi8(33));
    (_mm_add_epi8(indices, correction), errors)
}

#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_indices_16_mixed(input: *const u8) -> (__m128i, __m128i) {
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
pub(super) fn classify_ascii_ssse3(
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
pub(super) fn errors_are_zero_ssse3(errors: __m128i) -> bool {
    _mm_movemask_epi8(_mm_cmpeq_epi8(errors, _mm_setzero_si128())) == 0xffff
}

#[target_feature(enable = "ssse3")]
pub(super) fn pack_16_indices(indices: __m128i) -> __m128i {
    let merged = _mm_maddubs_epi16(indices, _mm_set1_epi32(0x0140_0140));
    let packed = _mm_madd_epi16(merged, _mm_set1_epi32(0x0001_1000));
    let shuffle = unsafe { _mm_loadu_si128(PACK_SHUFFLE.as_ptr().cast()) };
    _mm_shuffle_epi8(packed, shuffle)
}
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn store_12_exact(output: *mut u8, value: __m128i) {
    unsafe { _mm_storel_epi64(output.cast(), value) };
    let remaining = _mm_cvtsi128_si32(_mm_srli_si128(value, 8));
    unsafe { output.add(8).cast::<i32>().write_unaligned(remaining) };
}
#[target_feature(enable = "ssse3")]
pub(super) unsafe fn store_12_padded(output: *mut u8, value: __m128i) {
    unsafe { _mm_storeu_si128(output.cast(), value) };
}
