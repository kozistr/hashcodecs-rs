#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::symbols::is_lenient_symbol;

#[target_feature(enable = "avx2")]
pub(in crate::bindings::base64::decode) unsafe fn symbol_count_avx2(
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> usize {
    let [extra0, extra1] = altchars.unwrap_or(*b"AA");
    let extra0 = _mm256_set1_epi8(extra0 as i8);
    let extra1 = _mm256_set1_epi8(extra1 as i8);
    let mut source = 0;
    let mut symbols = 0;
    while source + 32 <= input.len() {
        let bytes = unsafe { _mm256_loadu_si256(input.as_ptr().add(source).cast()) };
        let valid = valid_avx2(bytes, extra0, extra1);
        symbols += _mm256_movemask_epi8(valid).count_ones() as usize;
        source += 32;
    }
    symbols
        + input[source..]
            .iter()
            .filter(|&&byte| is_lenient_symbol(byte, altchars))
            .count()
}

#[target_feature(enable = "avx2")]
pub(in crate::bindings::base64::decode) unsafe fn alphanumeric_prefix_avx2(input: &[u8]) -> usize {
    let mut source = 0;
    while source + 32 <= input.len() {
        let bytes = unsafe { _mm256_loadu_si256(input.as_ptr().add(source).cast()) };
        let valid = _mm256_or_si256(
            _mm256_or_si256(range_avx2(bytes, b'A', b'Z'), range_avx2(bytes, b'a', b'z')),
            range_avx2(bytes, b'0', b'9'),
        );
        let mask = _mm256_movemask_epi8(valid) as u32;
        if mask != u32::MAX {
            return source + (!mask).trailing_zeros() as usize;
        }
        source += 32;
    }
    source
        + input[source..]
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric())
            .unwrap_or(input.len() - source)
}

#[target_feature(enable = "avx2")]
pub(in crate::bindings::base64::decode) unsafe fn translate_avx2(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    let source0_vector = _mm256_set1_epi8(source0 as i8);
    let target0_vector = _mm256_set1_epi8(target0 as i8);
    let source1_vector = _mm256_set1_epi8(source1 as i8);
    let target1_vector = _mm256_set1_epi8(target1 as i8);
    let mut offset = 0;
    while offset + 32 <= input.len() {
        let bytes = unsafe { _mm256_loadu_si256(input.as_ptr().add(offset).cast()) };
        let mask0 = _mm256_cmpeq_epi8(bytes, source0_vector);
        let translated0 = _mm256_or_si256(
            _mm256_and_si256(mask0, target0_vector),
            _mm256_andnot_si256(mask0, bytes),
        );
        let mask1 = _mm256_cmpeq_epi8(bytes, source1_vector);
        let translated1 = _mm256_or_si256(
            _mm256_and_si256(mask1, target1_vector),
            _mm256_andnot_si256(mask1, translated0),
        );
        unsafe { _mm256_storeu_si256(input.as_mut_ptr().add(offset).cast(), translated1) };
        offset += 32;
    }
    for byte in &mut input[offset..] {
        if *byte == source0 {
            *byte = target0;
        } else if *byte == source1 {
            *byte = target1;
        }
    }
}

#[target_feature(enable = "avx2")]
#[inline]
fn valid_avx2(bytes: __m256i, extra0: __m256i, extra1: __m256i) -> __m256i {
    let upper = range_avx2(bytes, b'A', b'Z');
    let lower = range_avx2(bytes, b'a', b'z');
    let digits = range_avx2(bytes, b'0', b'9');
    _mm256_or_si256(
        _mm256_or_si256(upper, lower),
        _mm256_or_si256(
            digits,
            _mm256_or_si256(
                _mm256_or_si256(
                    _mm256_cmpeq_epi8(bytes, _mm256_set1_epi8(b'+' as i8)),
                    _mm256_cmpeq_epi8(bytes, _mm256_set1_epi8(b'/' as i8)),
                ),
                _mm256_or_si256(
                    _mm256_cmpeq_epi8(bytes, extra0),
                    _mm256_cmpeq_epi8(bytes, extra1),
                ),
            ),
        ),
    )
}

#[target_feature(enable = "avx2")]
#[inline]
fn range_avx2(bytes: __m256i, lower: u8, upper: u8) -> __m256i {
    _mm256_and_si256(
        _mm256_cmpeq_epi8(_mm256_max_epu8(bytes, _mm256_set1_epi8(lower as i8)), bytes),
        _mm256_cmpeq_epi8(_mm256_min_epu8(bytes, _mm256_set1_epi8(upper as i8)), bytes),
    )
}

#[target_feature(enable = "sse2")]
pub(in crate::bindings::base64::decode) unsafe fn symbol_count_sse2(
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> usize {
    let [extra0, extra1] = altchars.unwrap_or(*b"AA");
    let extra0 = _mm_set1_epi8(extra0 as i8);
    let extra1 = _mm_set1_epi8(extra1 as i8);
    let mut source = 0;
    let mut symbols = 0;
    while source + 16 <= input.len() {
        let bytes = unsafe { _mm_loadu_si128(input.as_ptr().add(source).cast()) };
        let valid = valid_sse2(bytes, extra0, extra1);
        symbols += _mm_movemask_epi8(valid).count_ones() as usize;
        source += 16;
    }
    symbols
        + input[source..]
            .iter()
            .filter(|&&byte| is_lenient_symbol(byte, altchars))
            .count()
}

#[target_feature(enable = "sse2")]
pub(in crate::bindings::base64::decode) unsafe fn alphanumeric_prefix_sse2(input: &[u8]) -> usize {
    let mut source = 0;
    while source + 16 <= input.len() {
        let bytes = unsafe { _mm_loadu_si128(input.as_ptr().add(source).cast()) };
        let valid = _mm_or_si128(
            _mm_or_si128(range_sse2(bytes, b'A', b'Z'), range_sse2(bytes, b'a', b'z')),
            range_sse2(bytes, b'0', b'9'),
        );
        let mask = _mm_movemask_epi8(valid) as u32;
        if mask != 0xffff {
            return source + ((!mask) & 0xffff).trailing_zeros() as usize;
        }
        source += 16;
    }
    source
        + input[source..]
            .iter()
            .position(|byte| !byte.is_ascii_alphanumeric())
            .unwrap_or(input.len() - source)
}

#[target_feature(enable = "sse2")]
pub(in crate::bindings::base64::decode) unsafe fn translate_sse2(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    let source0_vector = _mm_set1_epi8(source0 as i8);
    let target0_vector = _mm_set1_epi8(target0 as i8);
    let source1_vector = _mm_set1_epi8(source1 as i8);
    let target1_vector = _mm_set1_epi8(target1 as i8);
    let mut offset = 0;
    while offset + 16 <= input.len() {
        let bytes = unsafe { _mm_loadu_si128(input.as_ptr().add(offset).cast()) };
        let mask0 = _mm_cmpeq_epi8(bytes, source0_vector);
        let translated0 = _mm_or_si128(
            _mm_and_si128(mask0, target0_vector),
            _mm_andnot_si128(mask0, bytes),
        );
        let mask1 = _mm_cmpeq_epi8(bytes, source1_vector);
        let translated1 = _mm_or_si128(
            _mm_and_si128(mask1, target1_vector),
            _mm_andnot_si128(mask1, translated0),
        );
        unsafe { _mm_storeu_si128(input.as_mut_ptr().add(offset).cast(), translated1) };
        offset += 16;
    }
    for byte in &mut input[offset..] {
        if *byte == source0 {
            *byte = target0;
        } else if *byte == source1 {
            *byte = target1;
        }
    }
}

#[target_feature(enable = "sse2")]
#[inline]
fn valid_sse2(bytes: __m128i, extra0: __m128i, extra1: __m128i) -> __m128i {
    let upper = range_sse2(bytes, b'A', b'Z');
    let lower = range_sse2(bytes, b'a', b'z');
    let digits = range_sse2(bytes, b'0', b'9');
    _mm_or_si128(
        _mm_or_si128(upper, lower),
        _mm_or_si128(
            digits,
            _mm_or_si128(
                _mm_or_si128(
                    _mm_cmpeq_epi8(bytes, _mm_set1_epi8(b'+' as i8)),
                    _mm_cmpeq_epi8(bytes, _mm_set1_epi8(b'/' as i8)),
                ),
                _mm_or_si128(_mm_cmpeq_epi8(bytes, extra0), _mm_cmpeq_epi8(bytes, extra1)),
            ),
        ),
    )
}

#[target_feature(enable = "sse2")]
#[inline]
fn range_sse2(bytes: __m128i, lower: u8, upper: u8) -> __m128i {
    _mm_and_si128(
        _mm_cmpeq_epi8(_mm_max_epu8(bytes, _mm_set1_epi8(lower as i8)), bytes),
        _mm_cmpeq_epi8(_mm_min_epu8(bytes, _mm_set1_epi8(upper as i8)), bytes),
    )
}
