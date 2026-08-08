#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::Base64Error;

pub(super) struct StandardDecoder;
pub(super) struct UrlSafeDecoder;
pub(super) struct MixedDecoder;
pub(super) struct ExactStore;
pub(super) struct PaddedStore;

pub(super) trait Decoder {
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i);
    unsafe fn decode_16(input: *const u8) -> Option<__m128i>;
    unsafe fn decode_indices_16_sse41(input: *const u8) -> (__m128i, __m128i);
}

pub(super) trait Store {
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
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_standard(input) }
    }

    #[inline(always)]
    unsafe fn decode_16(input: *const u8) -> Option<__m128i> {
        unsafe { decode_16_standard(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16_sse41(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_sse41_standard(input) }
    }
}

impl Decoder for UrlSafeDecoder {
    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_urlsafe(input) }
    }

    #[inline(always)]
    unsafe fn decode_16(input: *const u8) -> Option<__m128i> {
        unsafe { decode_16_urlsafe(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16_sse41(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_sse41_urlsafe(input) }
    }
}

impl Decoder for MixedDecoder {
    #[inline(always)]
    unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
        unsafe { decode_indices_32_mixed(input) }
    }

    #[inline(always)]
    unsafe fn decode_16(input: *const u8) -> Option<__m128i> {
        unsafe { decode_16_mixed(input) }
    }

    #[inline(always)]
    unsafe fn decode_indices_16_sse41(input: *const u8) -> (__m128i, __m128i) {
        unsafe { decode_indices_16_sse41_mixed(input) }
    }
}

#[target_feature(enable = "ssse3")]
pub(super) unsafe fn encode_ssse3<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
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

#[target_feature(enable = "ssse3,sse4.1")]
pub(super) unsafe fn encode_sse41<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    unsafe { encode_ssse3::<URLSAFE>(input, output) }
}

#[target_feature(enable = "ssse3,sse4.1,sse4.2")]
pub(super) unsafe fn encode_sse42<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    unsafe { encode_ssse3::<URLSAFE>(input, output) }
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn encode_avx2<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
    let mut source = 0;
    let mut destination = 0;
    while source + 104 <= input.len() {
        let first = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source)) };
        let second = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source + 24)) };
        let third = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source + 48)) };
        let fourth = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source + 72)) };
        unsafe { _mm256_storeu_si256(output.add(destination).cast(), first) };
        unsafe { _mm256_storeu_si256(output.add(destination + 32).cast(), second) };
        unsafe { _mm256_storeu_si256(output.add(destination + 64).cast(), third) };
        unsafe { _mm256_storeu_si256(output.add(destination + 96).cast(), fourth) };
        source += 96;
        destination += 128;
    }
    while source + 32 <= input.len() {
        let encoded = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source)) };
        unsafe { _mm256_storeu_si256(output.add(destination).cast(), encoded) };
        source += 24;
        destination += 32;
    }
    // The remainder still benefits from the SSSE3 kernel.
    source + unsafe { encode_ssse3::<URLSAFE>(&input[source..], output.add(destination)) }
}

#[target_feature(enable = "avx2")]
pub(super) unsafe fn decode_avx2<A: Decoder, S: Store>(
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
        unsafe { S::store_24(output.add(destination), pack_32(first)) };
        unsafe { S::store_24(output.add(destination + 24), pack_32(second)) };
        unsafe { S::store_24(output.add(destination + 48), pack_32(third)) };
        unsafe { S::store_24(output.add(destination + 72), pack_32(fourth)) };
        source += 128;
        destination += 96;
    }
    while source + 32 <= input.len() {
        let (indices, errors) = unsafe { A::decode_32(input.as_ptr().add(source)) };
        if _mm256_testz_si256(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_24(output.add(destination), pack_32(indices)) };
        source += 32;
        destination += 24;
    }
    let (tail_source, tail_destination) =
        unsafe { decode_ssse3::<A, S>(&input[source..], output.add(destination)) }?;
    Ok((source + tail_source, destination + tail_destination))
}

#[target_feature(enable = "ssse3")]
pub(super) unsafe fn decode_ssse3<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 16 <= input.len() {
        let decoded =
            unsafe { A::decode_16(input.as_ptr().add(source)) }.ok_or(Base64Error::InvalidInput)?;
        unsafe { S::store_12(output.add(destination), decoded) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3,sse4.1")]
pub(super) unsafe fn decode_sse41<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    let mut source = 0;
    let mut destination = 0;
    while source + 64 <= input.len() {
        let (first, first_errors) =
            unsafe { A::decode_indices_16_sse41(input.as_ptr().add(source)) };
        let (second, second_errors) =
            unsafe { A::decode_indices_16_sse41(input.as_ptr().add(source + 16)) };
        let (third, third_errors) =
            unsafe { A::decode_indices_16_sse41(input.as_ptr().add(source + 32)) };
        let (fourth, fourth_errors) =
            unsafe { A::decode_indices_16_sse41(input.as_ptr().add(source + 48)) };
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
        let (indices, errors) = unsafe { A::decode_indices_16_sse41(input.as_ptr().add(source)) };
        if _mm_testz_si128(errors, errors) == 0 {
            return Err(Base64Error::InvalidInput);
        }
        unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
        source += 16;
        destination += 12;
    }
    Ok((source, destination))
}

#[target_feature(enable = "ssse3,sse4.1,sse4.2")]
pub(super) unsafe fn decode_sse42<A: Decoder, S: Store>(
    input: &[u8],
    output: *mut u8,
) -> Result<(usize, usize), Base64Error> {
    unsafe { decode_sse41::<A, S>(input, output) }
}

#[target_feature(enable = "ssse3")]
unsafe fn encode_12<const URLSAFE: bool>(input: *const u8) -> __m128i {
    let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);
    let mut value = unsafe { _mm_loadu_si128(input.cast()) };
    value = _mm_shuffle_epi8(value, shuffle);

    let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
    let higher = _mm_mulhi_epu16(higher, _mm_set1_epi32(0x0400_0040));
    let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
    let lower = _mm_mullo_epi16(lower, _mm_set1_epi32(0x0100_0010));
    ascii_from_indices::<URLSAFE>(_mm_or_si128(higher, lower))
}

#[target_feature(enable = "avx2")]
unsafe fn encode_24<const URLSAFE: bool>(input: *const u8) -> __m256i {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let value = _mm256_permutevar8x32_epi32(value, _mm256_setr_epi32(0, 1, 2, 3, 3, 4, 5, 6));
    let shuffle = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10,
    ));
    let value = _mm256_shuffle_epi8(value, shuffle);

    let higher = _mm256_and_si256(value, _mm256_set1_epi32(0x0fc0_fc00));
    let higher = _mm256_mulhi_epu16(higher, _mm256_set1_epi32(0x0400_0040));
    let lower = _mm256_and_si256(value, _mm256_set1_epi32(0x003f_03f0));
    let lower = _mm256_mullo_epi16(lower, _mm256_set1_epi32(0x0100_0010));
    ascii_from_indices_avx2::<URLSAFE>(_mm256_or_si256(higher, lower))
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_16_standard(input: *const u8) -> Option<__m128i> {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let (indices, valid) = ascii_to_indices_standard(value);
    decode_16_indices(indices, valid)
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_16_urlsafe(input: *const u8) -> Option<__m128i> {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let (indices, valid) = ascii_to_indices_urlsafe(value);
    decode_16_indices(indices, valid)
}

#[target_feature(enable = "ssse3")]
unsafe fn decode_16_mixed(input: *const u8) -> Option<__m128i> {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let (indices, valid) = ascii_to_indices_mixed(value);
    decode_16_indices(indices, valid)
}

#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn decode_indices_16_sse41_standard(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    ascii_to_indices_sse41(value)
}

#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn decode_indices_16_sse41_urlsafe(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    let normalized = normalize_urlsafe_sse41(value);
    let (indices, mut errors) = ascii_to_indices_sse41(normalized);
    let standard_symbols = _mm_or_si128(
        _mm_cmpeq_epi8(value, _mm_set1_epi8(b'+' as i8)),
        _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8)),
    );
    errors = _mm_or_si128(errors, standard_symbols);
    (indices, errors)
}

#[target_feature(enable = "ssse3,sse4.1")]
unsafe fn decode_indices_16_sse41_mixed(input: *const u8) -> (__m128i, __m128i) {
    let value = unsafe { _mm_loadu_si128(input.cast()) };
    ascii_to_indices_sse41(normalize_urlsafe_sse41(value))
}

#[target_feature(enable = "ssse3,sse4.1")]
fn normalize_urlsafe_sse41(value: __m128i) -> __m128i {
    let dash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8));
    let underscore = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8));
    let value = _mm_blendv_epi8(value, _mm_set1_epi8(b'+' as i8), dash);
    _mm_blendv_epi8(value, _mm_set1_epi8(b'/' as i8), underscore)
}

#[target_feature(enable = "ssse3")]
fn ascii_to_indices_sse41(value: __m128i) -> (__m128i, __m128i) {
    let mask = _mm_set1_epi8(0x2f);
    let high_nibbles = _mm_and_si128(_mm_srli_epi32(value, 4), mask);
    let low_nibbles = _mm_and_si128(value, mask);
    let high_classes = _mm_shuffle_epi8(
        _mm_setr_epi8(
            0x10, 0x10, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10,
            0x10, 0x10,
        ),
        high_nibbles,
    );
    let low_classes = _mm_shuffle_epi8(
        _mm_setr_epi8(
            0x15, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x13, 0x1a, 0x1b, 0x1b,
            0x1b, 0x1a,
        ),
        low_nibbles,
    );
    let errors = _mm_and_si128(high_classes, low_classes);

    let slash = _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8));
    let offset_indices = _mm_add_epi8(high_nibbles, slash);
    let offsets = _mm_setr_epi8(0, 16, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0);
    (
        _mm_add_epi8(value, _mm_shuffle_epi8(offsets, offset_indices)),
        errors,
    )
}

#[target_feature(enable = "ssse3")]
fn decode_16_indices(indices: __m128i, valid: __m128i) -> Option<__m128i> {
    if _mm_movemask_epi8(valid) != 0xffff {
        return None;
    }

    Some(pack_16_indices(indices))
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
    let (value, mut indices) = unsafe { decode_indices_32_base(input) };
    let special_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'+' as i8));
    let special_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    indices = _mm256_add_epi8(indices, _mm256_and_si256(special_63, _mm256_set1_epi8(-3)));
    decode_indices_32_finish(value, indices, special_62, special_63)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_urlsafe(input: *const u8) -> (__m256i, __m256i) {
    let (value, mut indices) = unsafe { decode_indices_32_base(input) };
    let special_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'-' as i8));
    let special_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let corrections = _mm256_or_si256(
        _mm256_and_si256(special_62, _mm256_set1_epi8(-2)),
        _mm256_and_si256(special_63, _mm256_set1_epi8(33)),
    );
    indices = _mm256_add_epi8(indices, corrections);
    decode_indices_32_finish(value, indices, special_62, special_63)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_mixed(input: *const u8) -> (__m256i, __m256i) {
    let (value, mut indices) = unsafe { decode_indices_32_base(input) };
    let standard_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'+' as i8));
    let standard_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
    let urlsafe_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'-' as i8));
    let urlsafe_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
    let special_62 = _mm256_or_si256(standard_62, urlsafe_62);
    let special_63 = _mm256_or_si256(standard_63, urlsafe_63);
    let corrections = _mm256_or_si256(
        _mm256_and_si256(urlsafe_62, _mm256_set1_epi8(-2)),
        _mm256_or_si256(
            _mm256_and_si256(standard_63, _mm256_set1_epi8(-3)),
            _mm256_and_si256(urlsafe_63, _mm256_set1_epi8(33)),
        ),
    );
    indices = _mm256_add_epi8(indices, corrections);
    decode_indices_32_finish(value, indices, special_62, special_63)
}

#[target_feature(enable = "avx2")]
unsafe fn decode_indices_32_base(input: *const u8) -> (__m256i, __m256i) {
    let value = unsafe { _mm256_loadu_si256(input.cast()) };
    let high_nibbles = _mm256_and_si256(_mm256_srli_epi16(value, 4), _mm256_set1_epi8(0x0f));
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        0, 0, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
    ));
    (
        value,
        _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, high_nibbles)),
    )
}

#[target_feature(enable = "avx2")]
fn decode_indices_32_finish(
    value: __m256i,
    indices: __m256i,
    special_62: __m256i,
    special_63: __m256i,
) -> (__m256i, __m256i) {
    let digits = range_errors_avx2(value, b'0', 9);
    let uppercase = range_errors_avx2(value, b'A', 25);
    let lowercase = range_errors_avx2(value, b'a', 25);
    let range_errors = _mm256_min_epu8(digits, _mm256_min_epu8(uppercase, lowercase));
    let symbols = _mm256_or_si256(special_62, special_63);
    (indices, _mm256_andnot_si256(symbols, range_errors))
}

#[target_feature(enable = "avx2")]
fn range_errors_avx2(value: __m256i, start: u8, length: i8) -> __m256i {
    _mm256_subs_epu8(
        _mm256_sub_epi8(value, _mm256_set1_epi8(start as i8)),
        _mm256_set1_epi8(length),
    )
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
fn ascii_from_indices<const URLSAFE: bool>(indices: __m128i) -> __m128i {
    let reduced = _mm_subs_epu8(indices, _mm_set1_epi8(51));
    let upper = _mm_cmpgt_epi8(_mm_set1_epi8(26), indices);
    let reduced = _mm_or_si128(reduced, _mm_and_si128(upper, _mm_set1_epi8(13)));
    let offsets = _mm_setr_epi8(
        b'G' as i8,
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
        b'A' as i8,
        0,
        0,
    );
    _mm_add_epi8(_mm_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "avx2")]
fn ascii_from_indices_avx2<const URLSAFE: bool>(indices: __m256i) -> __m256i {
    let reduced = _mm256_subs_epu8(indices, _mm256_set1_epi8(51));
    let less = _mm256_cmpgt_epi8(_mm256_set1_epi8(26), indices);
    let reduced = _mm256_or_si256(reduced, _mm256_and_si256(less, _mm256_set1_epi8(13)));
    let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
        b'G' as i8,
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
        b'A' as i8,
        0,
        0,
    ));
    _mm256_add_epi8(_mm256_shuffle_epi8(offsets, reduced), indices)
}

#[target_feature(enable = "ssse3")]
fn ascii_to_indices_standard(value: __m128i) -> (__m128i, __m128i) {
    ascii_to_indices_with_symbols(
        value,
        _mm_cmpeq_epi8(value, _mm_set1_epi8(b'+' as i8)),
        _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8)),
    )
}

#[target_feature(enable = "ssse3")]
fn ascii_to_indices_urlsafe(value: __m128i) -> (__m128i, __m128i) {
    ascii_to_indices_with_symbols(
        value,
        _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8)),
        _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8)),
    )
}

#[target_feature(enable = "ssse3")]
fn ascii_to_indices_mixed(value: __m128i) -> (__m128i, __m128i) {
    ascii_to_indices_with_symbols(
        value,
        _mm_or_si128(
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'+' as i8)),
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8)),
        ),
        _mm_or_si128(
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8)),
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8)),
        ),
    )
}

#[target_feature(enable = "ssse3")]
fn ascii_to_indices_with_symbols(
    value: __m128i,
    special_62: __m128i,
    special_63: __m128i,
) -> (__m128i, __m128i) {
    let upper = between(value, b'A', b'Z');
    let lower = between(value, b'a', b'z');
    let digit = between(value, b'0', b'9');

    let mut indices = _mm_sub_epi8(value, _mm_set1_epi8(b'A' as i8));
    indices = select(
        lower,
        _mm_sub_epi8(value, _mm_set1_epi8((b'a' - 26) as i8)),
        indices,
    );
    indices = select(digit, _mm_add_epi8(value, _mm_set1_epi8(4)), indices);
    indices = select(special_62, _mm_set1_epi8(62), indices);
    indices = select(special_63, _mm_set1_epi8(63), indices);
    (
        indices,
        _mm_or_si128(
            _mm_or_si128(upper, lower),
            _mm_or_si128(digit, _mm_or_si128(special_62, special_63)),
        ),
    )
}

#[target_feature(enable = "ssse3")]
fn between(value: __m128i, lower: u8, upper: u8) -> __m128i {
    let above_lower = _mm_cmpgt_epi8(value, _mm_set1_epi8((lower - 1) as i8));
    let below_upper = _mm_cmpgt_epi8(_mm_set1_epi8((upper + 1) as i8), value);
    _mm_and_si128(above_lower, below_upper)
}

#[target_feature(enable = "ssse3")]
fn select(mask: __m128i, yes: __m128i, no: __m128i) -> __m128i {
    _mm_or_si128(_mm_and_si128(mask, yes), _mm_andnot_si128(mask, no))
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
