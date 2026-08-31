use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::super::{STANDARD_ALPHABET, output_too_small};
use super::super::output::BytesWriter;
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LenientDecodeError {
    InvalidInput,
    OutputTooSmall,
}

pub(super) fn lenient_decode_table(altchars: Option<[u8; 2]>) -> [u8; 256] {
    let mut table = [64_u8; 256];
    for (value, &byte) in STANDARD_ALPHABET.iter().enumerate() {
        table[usize::from(byte)] = value as u8;
    }
    if let Some([plus, slash]) = altchars {
        table[usize::from(plus)] = 62;
        table[usize::from(slash)] = 63;
    }
    table
}

#[inline]
pub(super) fn decoded_symbol_len(symbols: usize) -> usize {
    symbols / 4 * 3
        + match symbols % 4 {
            2 => 1,
            3 => 2,
            _ => 0,
        }
}

#[inline(always)]
pub(super) fn is_lenient_symbol(byte: u8, altchars: Option<[u8; 2]>) -> bool {
    byte.wrapping_sub(b'A') <= b'Z' - b'A'
        || byte.wrapping_sub(b'a') <= b'z' - b'a'
        || byte.wrapping_sub(b'0') <= b'9' - b'0'
        || matches!(byte, b'+' | b'/')
        || altchars.is_some_and(|[plus, slash]| byte == plus || byte == slash)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod lenient_count_x86 {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    pub(in crate::bindings::base64::decode::native) unsafe fn avx2(
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
                .filter(|&&byte| super::is_lenient_symbol(byte, altchars))
                .count()
    }

    #[target_feature(enable = "avx2")]
    pub(in crate::bindings::base64::decode::native) unsafe fn alphanumeric_prefix_avx2(
        input: &[u8],
    ) -> usize {
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
    pub(in crate::bindings::base64::decode::native) unsafe fn translate_avx2(
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
    pub(in crate::bindings::base64::decode::native) unsafe fn sse2(
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
                .filter(|&&byte| super::is_lenient_symbol(byte, altchars))
                .count()
    }

    #[target_feature(enable = "sse2")]
    pub(in crate::bindings::base64::decode::native) unsafe fn alphanumeric_prefix_sse2(
        input: &[u8],
    ) -> usize {
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
    pub(in crate::bindings::base64::decode::native) unsafe fn translate_sse2(
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
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn lenient_symbol_count_neon(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    use std::arch::aarch64::*;

    let [extra0, extra1] = altchars.unwrap_or(*b"AA");
    let mut source = 0;
    let mut symbols = 0;
    while source + 16 <= input.len() {
        let bytes = unsafe { vld1q_u8(input.as_ptr().add(source)) };
        let range = |lower, upper| {
            vandq_u8(
                vcgeq_u8(bytes, vdupq_n_u8(lower)),
                vcleq_u8(bytes, vdupq_n_u8(upper)),
            )
        };
        let valid = vorrq_u8(
            vorrq_u8(range(b'A', b'Z'), range(b'a', b'z')),
            vorrq_u8(
                range(b'0', b'9'),
                vorrq_u8(
                    vorrq_u8(
                        vceqq_u8(bytes, vdupq_n_u8(b'+')),
                        vceqq_u8(bytes, vdupq_n_u8(b'/')),
                    ),
                    vorrq_u8(
                        vceqq_u8(bytes, vdupq_n_u8(extra0)),
                        vceqq_u8(bytes, vdupq_n_u8(extra1)),
                    ),
                ),
            ),
        );
        symbols += vaddvq_u8(vshrq_n_u8::<7>(valid)) as usize;
        source += 16;
    }
    symbols
        + input[source..]
            .iter()
            .filter(|&&byte| is_lenient_symbol(byte, altchars))
            .count()
}

pub(super) fn lenient_symbol_count(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if input.len() >= 32 && std::is_x86_feature_detected!("avx2") {
            return unsafe { lenient_count_x86::avx2(input, altchars) };
        }
        if input.len() >= 16 && std::is_x86_feature_detected!("sse2") {
            return unsafe { lenient_count_x86::sse2(input, altchars) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if input.len() >= 16 {
            return unsafe { lenient_symbol_count_neon(input, altchars) };
        }
    }

    input
        .iter()
        .filter(|&&byte| is_lenient_symbol(byte, altchars))
        .count()
}

pub(super) type AlphanumericPrefix = unsafe fn(&[u8]) -> usize;

pub(super) unsafe fn alphanumeric_prefix_scalar(input: &[u8]) -> usize {
    input
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(input.len())
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) unsafe fn alphanumeric_prefix_avx2(input: &[u8]) -> usize {
    unsafe { lenient_count_x86::alphanumeric_prefix_avx2(input) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) unsafe fn alphanumeric_prefix_sse2(input: &[u8]) -> usize {
    unsafe { lenient_count_x86::alphanumeric_prefix_sse2(input) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_alphanumeric_prefix_for_x86(avx2: bool, sse2: bool) -> AlphanumericPrefix {
    if avx2 {
        return alphanumeric_prefix_avx2;
    }
    if sse2 {
        return alphanumeric_prefix_sse2;
    }
    alphanumeric_prefix_scalar
}

pub(super) fn select_alphanumeric_prefix() -> AlphanumericPrefix {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_alphanumeric_prefix_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    alphanumeric_prefix_scalar
}

pub(super) type TranslateBytes = unsafe fn(&mut [u8], u8, u8, u8, u8);

pub(super) unsafe fn translate_bytes_scalar(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    for byte in input {
        if *byte == source0 {
            *byte = target0;
        } else if *byte == source1 {
            *byte = target1;
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) unsafe fn translate_bytes_avx2(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    unsafe { lenient_count_x86::translate_avx2(input, source0, target0, source1, target1) };
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) unsafe fn translate_bytes_sse2(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    unsafe { lenient_count_x86::translate_sse2(input, source0, target0, source1, target1) };
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub(super) unsafe fn translate_bytes_neon(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    use std::arch::aarch64::*;

    let mut offset = 0;
    while offset + 16 <= input.len() {
        let bytes = unsafe { vld1q_u8(input.as_ptr().add(offset)) };
        let translated0 = vbslq_u8(
            vceqq_u8(bytes, vdupq_n_u8(source0)),
            vdupq_n_u8(target0),
            bytes,
        );
        let translated1 = vbslq_u8(
            vceqq_u8(bytes, vdupq_n_u8(source1)),
            vdupq_n_u8(target1),
            translated0,
        );
        unsafe { vst1q_u8(input.as_mut_ptr().add(offset), translated1) };
        offset += 16;
    }
    unsafe { translate_bytes_scalar(&mut input[offset..], source0, target0, source1, target1) };
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_translate_bytes_for_x86(avx2: bool, sse2: bool) -> TranslateBytes {
    if avx2 {
        return translate_bytes_avx2;
    }
    if sse2 {
        return translate_bytes_sse2;
    }
    translate_bytes_scalar
}

pub(super) fn select_translate_bytes() -> TranslateBytes {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_translate_bytes_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(target_arch = "aarch64")]
    return translate_bytes_neon;

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    translate_bytes_scalar
}

/// Count the output from CPython's non-strict padding and discard rules.
///
/// Current CPython versions continue after a complete padding sequence, so a
/// branchless symbol count plus the trailing padding state determines the
/// result. Older versions use the sequential fallback to honor their early
/// stop at the first complete padding sequence.
pub(super) fn lenient_decoded_len(
    input: &[u8],
    altchars: Option<[u8; 2]>,
    padded: bool,
    continue_after_padding: bool,
) -> Result<usize, LenientDecodeError> {
    if continue_after_padding {
        let symbols = lenient_symbol_count(input, altchars);
        let quad_pos = symbols % 4;
        let pads = if padded && !is_lenient_symbol(b'=', altchars) && quad_pos != 0 {
            input
                .iter()
                .rev()
                .take_while(|&&byte| !is_lenient_symbol(byte, altchars))
                .filter(|&&byte| byte == b'=')
                .count()
        } else {
            0
        };
        return if quad_pos == 1 || (padded && quad_pos != 0 && quad_pos + pads < 4) {
            Err(LenientDecodeError::InvalidInput)
        } else {
            Ok(decoded_symbol_len(symbols))
        };
    }

    let mut source = 0;
    let mut symbols = 0;
    let mut pads = 0;

    while source < input.len() {
        while source + 8 <= input.len() {
            if !input[source..source + 8]
                .iter()
                .all(|&byte| is_lenient_symbol(byte, altchars))
            {
                break;
            }
            symbols += 8;
            pads = 0;
            source += 8;
        }
        if source == input.len() {
            break;
        }

        let byte = input[source];
        source += 1;
        if padded && byte == b'=' && !is_lenient_symbol(b'=', altchars) {
            pads += 1;
            let quad_pos = symbols % 4;
            if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                return Ok(decoded_symbol_len(symbols));
            }
            continue;
        }
        if !is_lenient_symbol(byte, altchars) {
            continue;
        }
        symbols += 1;
        pads = 0;
    }

    let quad_pos = symbols % 4;
    if quad_pos == 1 || (padded && quad_pos != 0 && quad_pos + pads < 4) {
        Err(LenientDecodeError::InvalidInput)
    } else {
        Ok(decoded_symbol_len(symbols))
    }
}

pub(in crate::bindings::base64::decode) fn lenient_continues_after_padding(py: Python<'_>) -> bool {
    let version = py.version_info();
    version_continues_after_padding(version.major, version.minor, version.patch)
}

pub(super) fn version_continues_after_padding(major: u8, minor: u8, patch: u8) -> bool {
    match (major, minor) {
        (3, 13) => patch >= 13,
        (3, 14) => patch >= 4,
        (major, minor) => (major, minor) >= (3, 15),
    }
}

/// Decode with CPython's non-strict padding and invalid-character semantics.
///
/// # Safety
///
/// `output` must be valid for writes of `provided` bytes and must not overlap
/// `input`.
pub(super) unsafe fn decode_lenient_to_ptr<const WRITE: bool>(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    table: &[u8; 256],
    padded: bool,
    continue_after_padding: bool,
) -> Result<usize, LenientDecodeError> {
    let mut source = 0;
    let mut written = 0;
    let mut quad_pos = 0;
    let mut leftchar = 0;
    let mut pads = 0;

    while source < input.len() {
        while quad_pos == 0 && source + 4 <= input.len() {
            let first = table[usize::from(input[source])];
            let second = table[usize::from(input[source + 1])];
            let third = table[usize::from(input[source + 2])];
            let fourth = table[usize::from(input[source + 3])];
            if first | second | third | fourth >= 64 {
                break;
            }
            if provided.saturating_sub(written) < 3 {
                return Err(LenientDecodeError::OutputTooSmall);
            }
            let decoded = [
                (first << 2) | (second >> 4),
                (second << 4) | (third >> 2),
                (third << 6) | fourth,
            ];
            if WRITE {
                unsafe {
                    output
                        .add(written)
                        .copy_from_nonoverlapping(decoded.as_ptr(), 3)
                };
            }
            written += 3;
            source += 4;
        }
        if source == input.len() {
            break;
        }

        let byte = input[source];
        source += 1;
        if padded && byte == b'=' && table[usize::from(b'=')] >= 64 {
            pads += 1;
            if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                return Ok(written);
            }
            continue;
        }

        let value = table[usize::from(byte)];
        if value >= 64 {
            continue;
        }
        pads = 0;
        match quad_pos {
            0 => {
                quad_pos = 1;
                leftchar = value;
            }
            1 => {
                if written == provided {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    unsafe { output.add(written).write((leftchar << 2) | (value >> 4)) };
                }
                written += 1;
                quad_pos = 2;
                leftchar = value & 0x0f;
            }
            2 => {
                if written == provided {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    unsafe { output.add(written).write((leftchar << 4) | (value >> 2)) };
                }
                written += 1;
                quad_pos = 3;
                leftchar = value & 0x03;
            }
            3 => {
                if written == provided {
                    return Err(LenientDecodeError::OutputTooSmall);
                }
                if WRITE {
                    unsafe { output.add(written).write((leftchar << 6) | value) };
                }
                written += 1;
                quad_pos = 0;
                leftchar = 0;
            }
            _ => unreachable!("Base64 quartet position is bounded"),
        }
    }

    if quad_pos == 1 || (padded && quad_pos != 0 && quad_pos + pads < 4) {
        Err(LenientDecodeError::InvalidInput)
    } else {
        Ok(written)
    }
}

pub(in crate::bindings::base64::decode) fn try_decode_lenient<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    padded: bool,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return try_decode_lenient(py, &BytesLike::OwnedVec(input), altchars, padded);
    }
    let writer = BytesWriter::new(py, input.len())?;
    let output_address = unsafe { writer.data() } as usize;
    let table = lenient_decode_table(altchars);
    let continue_after_padding = lenient_continues_after_padding(py);
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let result = unsafe {
        input.with_bytes(|input| {
            let decode = move || {
                decode_lenient_to_ptr::<true>(
                    input,
                    output_address as *mut u8,
                    input.len().div_ceil(4) * 3,
                    &table,
                    padded,
                    continue_after_padding,
                )
            };
            if detach { py.detach(decode) } else { decode() }
        })
    };
    match result {
        Ok(written) => unsafe { writer.finish(py, written).map(Some) },
        Err(LenientDecodeError::InvalidInput | LenientDecodeError::OutputTooSmall) => Ok(None),
    }
}

pub(in crate::bindings::base64::decode) fn try_decode_lenient_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    continue_after_padding: bool,
) -> PyResult<Option<usize>> {
    let table = lenient_decode_table(altchars);
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_lenient_slice_into(
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                &table,
                altchars,
                padded,
                continue_after_padding,
            )
        });
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_lenient_slice_into(
                input,
                output,
                provided,
                &table,
                altchars,
                padded,
                continue_after_padding,
            )
        })
    }
}

/// Decode lenient input without partially writing an undersized destination.
///
/// # Safety
///
/// `output` must be valid for writes of `provided` bytes and must not overlap
/// `input`.
unsafe fn decode_lenient_slice_into(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    table: &[u8; 256],
    altchars: Option<[u8; 2]>,
    padded: bool,
    continue_after_padding: bool,
) -> PyResult<Option<usize>> {
    let maximum = input.len().div_ceil(4) * 3;
    if provided < maximum {
        let required = lenient_decoded_len(input, altchars, padded, continue_after_padding);
        match required {
            Ok(required) if provided < required => {
                return Err(output_too_small(required, provided));
            }
            Ok(_) => {}
            Err(LenientDecodeError::InvalidInput | LenientDecodeError::OutputTooSmall) => {
                return Ok(None);
            }
        }
    }
    Ok(unsafe {
        decode_lenient_to_ptr::<true>(
            input,
            output,
            provided,
            table,
            padded,
            continue_after_padding,
        )
    }
    .ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn x86_backend_selectors_cover_each_dispatch_tier() {
        for (avx2, sse2, expected) in [
            (true, true, alphanumeric_prefix_avx2 as AlphanumericPrefix),
            (false, true, alphanumeric_prefix_sse2 as AlphanumericPrefix),
            (
                false,
                false,
                alphanumeric_prefix_scalar as AlphanumericPrefix,
            ),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_alphanumeric_prefix_for_x86(avx2, sse2),
                expected,
            ));
        }

        for (avx2, sse2, expected) in [
            (true, true, translate_bytes_avx2 as TranslateBytes),
            (false, true, translate_bytes_sse2 as TranslateBytes),
            (false, false, translate_bytes_scalar as TranslateBytes),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_translate_bytes_for_x86(avx2, sse2),
                expected,
            ));
        }
    }

    #[test]
    fn legacy_lenient_sizing_rejects_incomplete_input() {
        assert_eq!(
            lenient_decoded_len(b"A", None, false, false),
            Err(LenientDecodeError::InvalidInput)
        );
        assert_eq!(
            lenient_decoded_len(b"AA", None, true, false),
            Err(LenientDecodeError::InvalidInput)
        );
    }
}
