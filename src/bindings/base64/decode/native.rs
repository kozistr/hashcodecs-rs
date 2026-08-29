use core::slice;

use pyo3::exceptions::PyMemoryError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::{STANDARD_ALPHABET, output_too_small, pybytes_with_len};
use super::fallback::decoding_error;
use super::output::BytesWriter;
use super::plan::DecodeOptions;
use crate::base64::{
    Base64Error, DecodeAlphabet, DecodeLayout, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
};
use crate::bindings::buffer::{BytesLike, contiguous_bytes_like, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LenientDecodeError {
    InvalidInput,
    OutputTooSmall,
}

fn lenient_decode_table(altchars: Option<[u8; 2]>) -> [u8; 256] {
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
fn decoded_symbol_len(symbols: usize) -> usize {
    symbols / 4 * 3
        + match symbols % 4 {
            2 => 1,
            3 => 2,
            _ => 0,
        }
}

#[inline(always)]
fn is_lenient_symbol(byte: u8, altchars: Option<[u8; 2]>) -> bool {
    byte.wrapping_sub(b'A') <= b'Z' - b'A'
        || byte.wrapping_sub(b'a') <= b'z' - b'a'
        || byte.wrapping_sub(b'0') <= b'9' - b'0'
        || matches!(byte, b'+' | b'/')
        || altchars.is_some_and(|[plus, slash]| byte == plus || byte == slash)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod lenient_count_x86 {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn avx2(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
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
    pub(super) unsafe fn alphanumeric_prefix_avx2(input: &[u8]) -> usize {
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
    pub(super) unsafe fn translate_avx2(
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
    pub(super) unsafe fn sse2(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
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
    pub(super) unsafe fn alphanumeric_prefix_sse2(input: &[u8]) -> usize {
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
    pub(super) unsafe fn translate_sse2(
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

fn lenient_symbol_count(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
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

type AlphanumericPrefix = unsafe fn(&[u8]) -> usize;

unsafe fn alphanumeric_prefix_scalar(input: &[u8]) -> usize {
    input
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(input.len())
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn alphanumeric_prefix_avx2(input: &[u8]) -> usize {
    unsafe { lenient_count_x86::alphanumeric_prefix_avx2(input) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn alphanumeric_prefix_sse2(input: &[u8]) -> usize {
    unsafe { lenient_count_x86::alphanumeric_prefix_sse2(input) }
}

fn select_alphanumeric_prefix() -> AlphanumericPrefix {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("avx2") {
            return alphanumeric_prefix_avx2;
        }
        if std::is_x86_feature_detected!("sse2") {
            return alphanumeric_prefix_sse2;
        }
    }
    alphanumeric_prefix_scalar
}

type TranslateBytes = unsafe fn(&mut [u8], u8, u8, u8, u8);

unsafe fn translate_bytes_scalar(
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
unsafe fn translate_bytes_avx2(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    unsafe { lenient_count_x86::translate_avx2(input, source0, target0, source1, target1) };
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn translate_bytes_sse2(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    unsafe { lenient_count_x86::translate_sse2(input, source0, target0, source1, target1) };
}

fn select_translate_bytes() -> TranslateBytes {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("avx2") {
            return translate_bytes_avx2;
        }
        if std::is_x86_feature_detected!("sse2") {
            return translate_bytes_sse2;
        }
    }
    translate_bytes_scalar
}

/// Count the output from CPython's non-strict padding and discard rules.
///
/// Current CPython versions continue after a complete padding sequence, so a
/// branchless symbol count plus the trailing padding state determines the
/// result. Older versions use the sequential fallback to honor their early
/// stop at the first complete padding sequence.
fn lenient_decoded_len(
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

pub(super) fn lenient_continues_after_padding(py: Python<'_>) -> bool {
    let version = py.version_info();
    match (version.major, version.minor) {
        (3, 13) => version.patch >= 13,
        (3, 14) => version.patch >= 4,
        (major, minor) => (major, minor) >= (3, 15),
    }
}

/// Decode with CPython's non-strict padding and invalid-character semantics.
///
/// # Safety
///
/// `output` must be valid for writes of `provided` bytes and must not overlap
/// `input`.
unsafe fn decode_lenient_to_ptr<const WRITE: bool>(
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

pub(super) fn try_decode_lenient<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    padded: bool,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return try_decode_lenient(py, &BytesLike::Owned(input), altchars, padded);
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

pub(super) fn try_decode_lenient_into(
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

const ADVANCED_STAGING_CAPACITY: usize = 4096;

#[derive(Clone, Copy)]
struct Translation {
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
    translate: TranslateBytes,
}

impl Translation {
    fn new(table: &[u8; 256]) -> Option<Self> {
        let mut sources = [0_u8; 2];
        let mut targets = [0_u8; 2];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            if value < 64 && STANDARD_ALPHABET[usize::from(value)] != byte {
                assert!(count < 2, "a Base64 alphabet translates at most two bytes");
                sources[count] = byte;
                targets[count] = STANDARD_ALPHABET[usize::from(value)];
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        if count == 1 {
            sources[1] = sources[0];
            targets[1] = targets[0];
        }
        Some(Self {
            source0: sources[0],
            target0: targets[0],
            source1: sources[1],
            target1: targets[1],
            translate: select_translate_bytes(),
        })
    }

    unsafe fn apply(self, input: &mut [u8]) {
        unsafe {
            (self.translate)(
                input,
                self.source0,
                self.target0,
                self.source1,
                self.target1,
            )
        };
    }
}

#[derive(Clone, Copy)]
enum StrictSpecials {
    None,
    One(u8),
    Two(u8, u8),
    Three(u8, u8, u8),
    Many,
}

impl StrictSpecials {
    fn new(table: &[u8; 256], ignored: &[bool; 256], padded: bool) -> Self {
        let equals_is_padding = padded && table[usize::from(b'=')] >= 64;
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            let discarded =
                value >= 64 && ignored[usize::from(byte)] && !(equals_is_padding && byte == b'=');
            if discarded {
                if count == bytes.len() {
                    return Self::Many;
                }
                bytes[count] = byte;
                count += 1;
            }
        }
        match (count, bytes) {
            (0, _) => Self::None,
            (1, [first, ..]) => Self::One(first),
            (2, [first, second, _]) => Self::Two(first, second),
            (3, [first, second, third]) => Self::Three(first, second, third),
            _ => unreachable!("strict special-byte count is bounded"),
        }
    }

    fn find(self, input: &[u8]) -> Option<usize> {
        match self {
            Self::None => None,
            Self::One(first) => memchr::memchr(first, input),
            Self::Two(first, second) => memchr::memchr2(first, second, input),
            Self::Three(first, second, third) => memchr::memchr3(first, second, third, input),
            Self::Many => unreachable!("many special bytes use the generic decoder"),
        }
    }

    fn forbidden(table: &[u8; 256], ignored: &[bool; 256]) -> Self {
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for (value, &byte) in STANDARD_ALPHABET.iter().enumerate() {
            if table[usize::from(byte)] >= 64 && !ignored[usize::from(byte)] {
                if count == bytes.len() {
                    return Self::Many;
                }
                bytes[count] = byte;
                count += 1;
            } else if table[usize::from(byte)] < 64 {
                debug_assert!(
                    table[usize::from(byte)] == value as u8
                        || STANDARD_ALPHABET[usize::from(table[usize::from(byte)])] != byte
                );
            }
        }
        match (count, bytes) {
            (0, _) => Self::None,
            (1, [first, ..]) => Self::One(first),
            (2, [first, second, _]) => Self::Two(first, second),
            (3, [first, second, third]) => Self::Three(first, second, third),
            _ => unreachable!("strict forbidden-byte count is bounded"),
        }
    }
}

struct AdvancedDecoder {
    table: [u8; 256],
    ignored: [bool; 256],
    strict_mode: bool,
    padded: bool,
    canonical: bool,
    alphanumeric_prefix: AlphanumericPrefix,
    strict_specials: StrictSpecials,
    strict_forbidden: StrictSpecials,
    translation: Option<Translation>,
}

impl AdvancedDecoder {
    fn new(py: Python<'_>, options: DecodeOptions<'_, '_>) -> PyResult<Self> {
        let DecodeOptions {
            altchars,
            padded,
            ignorechars,
            canonical,
            ..
        } = options;
        let mut ignored = [false; 256];
        if let Some(ignorechars) = ignorechars {
            let ignorechars = contiguous_bytes_like(py, ignorechars, "ignorechars")?;
            unsafe {
                ignorechars.with_bytes(|bytes| {
                    for &byte in bytes {
                        ignored[usize::from(byte)] = true;
                    }
                })
            };
        }

        let mut table = [64; 256];
        for (value, &byte) in STANDARD_ALPHABET[..62].iter().enumerate() {
            table[usize::from(byte)] = value as u8;
        }
        let custom_alphabet = altchars.is_some() && ignorechars.is_some();
        if !custom_alphabet {
            table[usize::from(b'+')] = 62;
            table[usize::from(b'/')] = 63;
        }
        if let Some([plus, slash]) = altchars {
            if !custom_alphabet || plus != b'=' {
                table[usize::from(plus)] = 62;
            }
            if !custom_alphabet || slash != b'=' {
                table[usize::from(slash)] = 63;
            }
        }

        let strict_specials = StrictSpecials::new(&table, &ignored, padded);
        let strict_forbidden = StrictSpecials::forbidden(&table, &ignored);
        let translation = Translation::new(&table);
        Ok(Self {
            table,
            ignored,
            strict_mode: options.strict_mode(),
            padded,
            canonical,
            alphanumeric_prefix: select_alphanumeric_prefix(),
            strict_specials,
            strict_forbidden,
            translation,
        })
    }

    fn preserves_alphanumeric(&self) -> bool {
        STANDARD_ALPHABET[..62]
            .iter()
            .enumerate()
            .all(|(value, &byte)| self.table[usize::from(byte)] == value as u8)
    }

    fn validate_strict(&self, input: &[u8]) -> Option<usize> {
        if !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return self.validate_strict_specials(input);
        }

        let mut symbols = 0;
        let mut padding = 0;
        let mut saw_padding = false;
        let mut last_value = 0;
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if !saw_padding && preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    symbols += run;
                    last_value = self.table[usize::from(input[source + run - 1])];
                    source += run;
                    continue;
                }
            }
            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];
            if value < 64 {
                if saw_padding {
                    return None;
                }
                symbols += 1;
                last_value = value;
            } else if byte == b'=' && !equals_is_data {
                if !self.padded {
                    return None;
                }
                saw_padding = true;
                padding += 1;
            } else if !self.ignored[usize::from(byte)] {
                return None;
            }
        }

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        Some(decoded_symbol_len(symbols))
    }

    fn validate_lenient(&self, input: &[u8], continue_after_padding: bool) -> Option<usize> {
        let mut symbols = 0;
        let mut quad_pos = 0;
        let mut leftchar = 0;
        let mut pads = 0;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    symbols += run;
                    pads = 0;
                    quad_pos = (quad_pos + run) % 4;
                    let last_value = self.table[usize::from(input[source + run - 1])];
                    leftchar = match quad_pos {
                        0 => 0,
                        1 => last_value,
                        2 => last_value & 0x0f,
                        3 => last_value & 0x03,
                        _ => unreachable!("Base64 quartet position is bounded"),
                    };
                    source += run;
                    continue;
                }
            }
            let byte = input[source];
            source += 1;
            if self.padded && byte == b'=' && self.table[usize::from(b'=')] >= 64 {
                pads += 1;
                if self.canonical && quad_pos >= 2 && quad_pos + pads >= 4 && leftchar != 0 {
                    return None;
                }
                if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                    return Some(decoded_symbol_len(symbols));
                }
                continue;
            }

            let value = self.table[usize::from(byte)];
            if value >= 64 {
                continue;
            }
            symbols += 1;
            pads = 0;
            match quad_pos {
                0 => {
                    quad_pos = 1;
                    leftchar = value;
                }
                1 => {
                    quad_pos = 2;
                    leftchar = value & 0x0f;
                }
                2 => {
                    quad_pos = 3;
                    leftchar = value & 0x03;
                }
                3 => {
                    quad_pos = 0;
                    leftchar = 0;
                }
                _ => unreachable!("Base64 quartet position is bounded"),
            }
        }

        if quad_pos == 1
            || (self.padded && quad_pos != 0 && quad_pos + pads < 4)
            || (self.canonical && matches!(quad_pos, 2 | 3) && leftchar != 0)
        {
            None
        } else {
            Some(decoded_symbol_len(symbols))
        }
    }

    fn validate_strict_specials(&self, input: &[u8]) -> Option<usize> {
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut scratch = [0_u8; ADVANCED_STAGING_CAPACITY / 4 * 3];
        let mut staged = 0;
        let mut symbols = 0;
        let mut last_value = 0;
        let equals_is_padding = self.padded && self.table[usize::from(b'=')] >= 64;
        let data_end = if equals_is_padding {
            memchr::memchr(b'=', input).unwrap_or(input.len())
        } else {
            input.len()
        };
        if self.strict_forbidden.find(&input[..data_end]).is_some() {
            return None;
        }

        let mut source = 0;
        while source < data_end {
            let special = self.strict_specials.find(&input[source..data_end]);
            let run_end = special.map_or(data_end, |offset| source + offset);
            while source < run_end {
                let copied = (run_end - source).min(ADVANCED_STAGING_CAPACITY - staged);
                staging[staged..staged + copied].copy_from_slice(&input[source..source + copied]);
                symbols += copied;
                staged += copied;
                source += copied;
                last_value = self.table[usize::from(input[source - 1])];
                if staged == ADVANCED_STAGING_CAPACITY {
                    if let Some(translation) = self.translation {
                        unsafe { translation.apply(&mut staging) };
                    }
                    if !validate_advanced_staging(&staging, &mut scratch) {
                        return None;
                    }
                    staged = 0;
                }
            }
            if source == data_end {
                break;
            }

            let byte = input[source];
            source += 1;
            debug_assert!(
                self.table[usize::from(byte)] >= 64 && self.ignored[usize::from(byte)],
                "strict special-byte search only returns discarded bytes"
            );
        }

        if staged != 0 {
            if let Some(translation) = self.translation {
                unsafe { translation.apply(&mut staging[..staged]) };
            }
            if !validate_advanced_staging(&staging[..staged], &mut scratch) {
                return None;
            }
        }

        let mut padding = 0;
        if data_end < input.len() {
            for &byte in &input[data_end..] {
                if byte == b'=' {
                    padding += 1;
                } else {
                    let value = self.table[usize::from(byte)];
                    if value < 64 || !self.ignored[usize::from(byte)] {
                        return None;
                    }
                }
            }
        }

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        Some(decoded_symbol_len(symbols))
    }

    fn decoded_len(&self, input: &[u8], continue_after_padding: bool) -> Option<usize> {
        if self.strict_mode {
            self.validate_strict(input)
        } else {
            self.validate_lenient(input, continue_after_padding)
        }
    }

    unsafe fn decode_checked_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> Option<usize> {
        if self.strict_mode {
            unsafe { self.decode_strict_checked_to_ptr(input, output) }
        } else {
            unsafe { self.decode_lenient_checked_to_ptr(input, output, continue_after_padding) }
        }
    }

    unsafe fn decode_strict_checked_to_ptr(&self, input: &[u8], output: *mut u8) -> Option<usize> {
        if !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return unsafe { self.decode_strict_specials_to_ptr(input, output) };
        }

        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut written = 0;
        let mut symbols = 0;
        let mut padding = 0;
        let mut saw_padding = false;
        let mut last_value = 0;
        let equals_is_data = self.table[usize::from(b'=')] < 64;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if !saw_padding && preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    symbols += run;
                    last_value = self.table[usize::from(input[source + run - 1])];
                    unsafe {
                        stage_advanced_symbols(
                            &input[source..source + run],
                            &mut staging,
                            &mut staged,
                            output,
                            &mut written,
                        )
                    };
                    source += run;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            let value = self.table[usize::from(byte)];
            if value < 64 {
                if saw_padding {
                    return None;
                }
                symbols += 1;
                last_value = value;
                unsafe {
                    stage_advanced_value(value, &mut staging, &mut staged, output, &mut written)
                };
            } else if byte == b'=' && !equals_is_data {
                if !self.padded {
                    return None;
                }
                saw_padding = true;
                padding += 1;
            } else if !self.ignored[usize::from(byte)] {
                return None;
            }
        }

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        unsafe { finish_advanced_staging(&staging, staged, output, written) }.into()
    }

    unsafe fn decode_strict_specials_to_ptr(&self, input: &[u8], output: *mut u8) -> Option<usize> {
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut written = 0;
        let mut symbols = 0;
        let mut last_value = 0;
        let equals_is_padding = self.padded && self.table[usize::from(b'=')] >= 64;
        let data_end = if equals_is_padding {
            memchr::memchr(b'=', input).unwrap_or(input.len())
        } else {
            input.len()
        };
        if self.strict_forbidden.find(&input[..data_end]).is_some() {
            return None;
        }

        let mut source = 0;
        while source < data_end {
            let special = self.strict_specials.find(&input[source..data_end]);
            let run_end = special.map_or(data_end, |offset| source + offset);
            while source < run_end {
                let copied = (run_end - source).min(ADVANCED_STAGING_CAPACITY - staged);
                staging[staged..staged + copied].copy_from_slice(&input[source..source + copied]);
                symbols += copied;
                staged += copied;
                source += copied;
                last_value = self.table[usize::from(input[source - 1])];
                if staged == ADVANCED_STAGING_CAPACITY {
                    if let Some(translation) = self.translation {
                        unsafe { translation.apply(&mut staging) };
                    }
                    written +=
                        unsafe { try_decode_advanced_staging(&staging, output.add(written))? };
                    staged = 0;
                }
            }
            if source == data_end {
                break;
            }

            let byte = input[source];
            source += 1;
            debug_assert!(
                self.table[usize::from(byte)] >= 64 && self.ignored[usize::from(byte)],
                "strict special-byte search only returns discarded bytes"
            );
        }

        let mut padding = 0;
        if data_end < input.len() {
            for &byte in &input[data_end..] {
                if byte == b'=' {
                    padding += 1;
                } else {
                    let value = self.table[usize::from(byte)];
                    if value < 64 || !self.ignored[usize::from(byte)] {
                        return None;
                    }
                }
            }
        }

        let remainder = symbols % 4;
        let expected_padding = match remainder {
            0 => 0,
            2 => 2,
            3 => 1,
            _ => return None,
        };
        if self.padded && padding != expected_padding {
            return None;
        }
        if self.canonical
            && ((remainder == 2 && last_value & 0x0f != 0)
                || (remainder == 3 && last_value & 0x03 != 0))
        {
            return None;
        }
        if staged != 0 {
            if let Some(translation) = self.translation {
                unsafe { translation.apply(&mut staging[..staged]) };
            }
            written +=
                unsafe { try_decode_advanced_staging(&staging[..staged], output.add(written))? };
        }
        Some(written)
    }

    unsafe fn decode_lenient_checked_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> Option<usize> {
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut written = 0;
        let mut quad_pos = 0;
        let mut leftchar = 0;
        let mut pads = 0;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        let mut source = 0;
        while source < input.len() {
            if preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    unsafe {
                        stage_advanced_symbols(
                            &input[source..source + run],
                            &mut staging,
                            &mut staged,
                            output,
                            &mut written,
                        )
                    };
                    pads = 0;
                    quad_pos = (quad_pos + run) & 3;
                    let last_value = self.table[usize::from(input[source + run - 1])];
                    leftchar = match quad_pos {
                        0 => 0,
                        1 => last_value,
                        2 => last_value & 0x0f,
                        3 => last_value & 0x03,
                        _ => unreachable!("Base64 quartet position is bounded"),
                    };
                    source += run;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            if self.padded && byte == b'=' && self.table[usize::from(b'=')] >= 64 {
                pads += 1;
                if self.canonical && quad_pos >= 2 && quad_pos + pads >= 4 && leftchar != 0 {
                    return None;
                }
                if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                    return Some(unsafe {
                        finish_advanced_staging(&staging, staged, output, written)
                    });
                }
                continue;
            }

            let value = self.table[usize::from(byte)];
            if value >= 64 {
                continue;
            }
            unsafe { stage_advanced_value(value, &mut staging, &mut staged, output, &mut written) };
            pads = 0;
            match quad_pos {
                0 => {
                    quad_pos = 1;
                    leftchar = value;
                }
                1 => {
                    quad_pos = 2;
                    leftchar = value & 0x0f;
                }
                2 => {
                    quad_pos = 3;
                    leftchar = value & 0x03;
                }
                3 => {
                    quad_pos = 0;
                    leftchar = 0;
                }
                _ => unreachable!("Base64 quartet position is bounded"),
            }
        }

        if quad_pos == 1
            || (self.padded && quad_pos != 0 && quad_pos + pads < 4)
            || (self.canonical && matches!(quad_pos, 2 | 3) && leftchar != 0)
        {
            None
        } else {
            Some(unsafe { finish_advanced_staging(&staging, staged, output, written) })
        }
    }

    unsafe fn decode_to_ptr(
        &self,
        input: &[u8],
        output: *mut u8,
        continue_after_padding: bool,
    ) -> usize {
        if self.strict_mode
            && !matches!(self.strict_specials, StrictSpecials::Many)
            && !matches!(self.strict_forbidden, StrictSpecials::Many)
        {
            return unsafe { self.decode_strict_specials_to_ptr(input, output) }
                .expect("validated strict advanced Base64 remains valid");
        }

        // Keep enough translated symbols on the stack to amortize SIMD decoder
        // dispatch without allocating a normalized copy of the whole input.
        let mut staging = [0_u8; ADVANCED_STAGING_CAPACITY];
        let mut staged = 0;
        let mut source = 0;
        let mut written = 0;
        let mut quad_pos = 0;
        let mut pads = 0;
        let preserves_alphanumeric = self.preserves_alphanumeric();

        while source < input.len() {
            if preserves_alphanumeric {
                // Construction selects a kernel supported by the current CPU.
                let run = unsafe { (self.alphanumeric_prefix)(&input[source..]) };
                if run != 0 {
                    let run_end = source + run;
                    while source < run_end {
                        let copied = (run_end - source).min(ADVANCED_STAGING_CAPACITY - staged);
                        staging[staged..staged + copied]
                            .copy_from_slice(&input[source..source + copied]);
                        staged += copied;
                        source += copied;
                        quad_pos = (quad_pos + copied) & 3;
                        if staged == ADVANCED_STAGING_CAPACITY {
                            written +=
                                unsafe { decode_advanced_staging(&staging, output.add(written)) };
                            staged = 0;
                        }
                    }
                    pads = 0;
                    continue;
                }
            }

            let byte = input[source];
            source += 1;
            if self.padded && byte == b'=' && self.table[usize::from(b'=')] >= 64 {
                pads += 1;
                if !continue_after_padding && quad_pos >= 2 && quad_pos + pads >= 4 {
                    break;
                }
                continue;
            }
            let value = self.table[usize::from(byte)];
            if value >= 64 {
                continue;
            }
            pads = 0;
            staging[staged] = STANDARD_ALPHABET[usize::from(value)];
            staged += 1;
            quad_pos = (quad_pos + 1) & 3;
            if staged == ADVANCED_STAGING_CAPACITY {
                written += unsafe { decode_advanced_staging(&staging, output.add(written)) };
                staged = 0;
            }
        }

        if staged != 0 {
            written += unsafe { decode_advanced_staging(&staging[..staged], output.add(written)) };
        }
        written
    }
}

#[inline]
unsafe fn decode_advanced_staging(input: &[u8], output: *mut u8) -> usize {
    unsafe { try_decode_advanced_staging(input, output) }
        .expect("validated advanced Base64 staging remains valid")
}

#[inline]
unsafe fn try_decode_advanced_staging(input: &[u8], output: *mut u8) -> Option<usize> {
    let layout = decode_unpadded_layout(input).ok()?;
    unsafe { decode_to_ptr_with_unpadded_layout(input, output, layout, DecodeAlphabet::Standard) }
        .ok()?;
    Some(layout.output_len())
}

fn validate_advanced_staging(
    input: &[u8],
    scratch: &mut [u8; ADVANCED_STAGING_CAPACITY / 4 * 3],
) -> bool {
    let Ok(layout) = decode_unpadded_layout(input) else {
        return false;
    };
    unsafe {
        decode_to_ptr_with_unpadded_layout(
            input,
            scratch.as_mut_ptr(),
            layout,
            DecodeAlphabet::Standard,
        )
    }
    .is_ok()
}

unsafe fn stage_advanced_symbols(
    input: &[u8],
    staging: &mut [u8; ADVANCED_STAGING_CAPACITY],
    staged: &mut usize,
    output: *mut u8,
    written: &mut usize,
) {
    let mut source = 0;
    while source < input.len() {
        let copied = (input.len() - source).min(ADVANCED_STAGING_CAPACITY - *staged);
        staging[*staged..*staged + copied].copy_from_slice(&input[source..source + copied]);
        *staged += copied;
        source += copied;
        if *staged == ADVANCED_STAGING_CAPACITY {
            *written += unsafe { decode_advanced_staging(staging, output.add(*written)) };
            *staged = 0;
        }
    }
}

unsafe fn stage_advanced_value(
    value: u8,
    staging: &mut [u8; ADVANCED_STAGING_CAPACITY],
    staged: &mut usize,
    output: *mut u8,
    written: &mut usize,
) {
    staging[*staged] = STANDARD_ALPHABET[usize::from(value)];
    *staged += 1;
    if *staged == ADVANCED_STAGING_CAPACITY {
        *written += unsafe { decode_advanced_staging(staging, output.add(*written)) };
        *staged = 0;
    }
}

unsafe fn finish_advanced_staging(
    staging: &[u8; ADVANCED_STAGING_CAPACITY],
    staged: usize,
    output: *mut u8,
    mut written: usize,
) -> usize {
    if staged != 0 {
        written += unsafe { decode_advanced_staging(&staging[..staged], output.add(written)) };
    }
    written
}

pub(super) fn try_decode_advanced<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return try_decode_advanced(py, &BytesLike::Owned(input), options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = lenient_continues_after_padding(py);
    let writer = BytesWriter::new(py, input.len())?;
    let output = unsafe { writer.data() };
    let Some(written) = (unsafe {
        input.with_bytes(|input| {
            decoder.decode_checked_to_ptr(input, output, continue_after_padding)
        })
    }) else {
        return Ok(None);
    };
    unsafe { writer.finish(py, written).map(Some) }
}

unsafe fn decode_advanced_slice_into(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    decoder: &AdvancedDecoder,
    continue_after_padding: bool,
) -> PyResult<Option<usize>> {
    let Some(required) = decoder.decoded_len(input, continue_after_padding) else {
        return Ok(None);
    };
    if provided < required {
        return Err(output_too_small(required, provided));
    }
    let written = unsafe { decoder.decode_to_ptr(input, output, continue_after_padding) };
    debug_assert_eq!(written, required);
    Ok(Some(written))
}

pub(super) fn try_decode_advanced_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<Option<usize>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return try_decode_advanced_into(py, &BytesLike::Owned(input), output, options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = lenient_continues_after_padding(py);
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_advanced_slice_into(
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                &decoder,
                continue_after_padding,
            )
        });
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_advanced_slice_into(input, output, provided, &decoder, continue_after_padding)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StrictDecodeError {
    InvalidLayout,
    InvalidAlphabet,
}

fn decode_strict_native<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Result<Bound<'py, PyBytes>, StrictDecodeError>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_strict_native(py, &BytesLike::Owned(input), alphabet);
    }
    let layout = match unsafe { input.with_bytes(decode_layout) } {
        Ok(layout) => layout,
        Err(Base64Error::InvalidInput | Base64Error::OutputTooSmall { .. }) => {
            return Ok(Err(StrictDecodeError::InvalidLayout));
        }
    };
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let (output, result) = unsafe {
        pybytes_with_len(py, layout.output_len(), |output| {
            input.with_bytes(|input| {
                let output_address = output as usize;
                let decode = move || {
                    decode_to_ptr_with_layout(
                        input,
                        output_address as *mut u8,
                        layout,
                        alphabet,
                        false,
                    )
                };
                if detach { py.detach(decode) } else { decode() }
            })
        })
    }?;
    Ok(match result {
        Ok(()) => Ok(output),
        Err(Base64Error::InvalidInput | Base64Error::OutputTooSmall { .. }) => {
            Err(StrictDecodeError::InvalidAlphabet)
        }
    })
}

pub(super) fn try_decode_strict<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    Ok(decode_strict_native(py, input, alphabet)?.ok())
}

pub(super) fn decode_strict<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Bound<'py, PyBytes>> {
    match decode_strict_native(py, input, alphabet)? {
        Ok(output) => Ok(output),
        Err(StrictDecodeError::InvalidLayout) => Err(decoding_error(py, "Incorrect padding")),
        Err(StrictDecodeError::InvalidAlphabet) => {
            Err(decoding_error(py, "Only base64 data is allowed"))
        }
    }
}

pub(super) fn decode_strict_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return Ok(decode_strict_slice_into(
            &input,
            output,
            alphabet,
            transactional_errors,
        ));
    }
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_strict_to_ptr(input, output, provided, alphabet, transactional_errors)
        })
    })
}

fn decode_strict_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    let layout = decode_layout(input)?;
    with_bytearray(output, || {
        let provided = unsafe { bytearray_size(output.as_ptr()) };
        decode_strict_with_layout_to_ptr(
            input,
            unsafe { bytearray_data(output.as_ptr()) },
            provided,
            layout,
            alphabet,
            transactional_errors,
        )
    })
}

fn decode_strict_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    let layout = decode_layout(input)?;
    decode_strict_with_layout_to_ptr(
        input,
        output,
        provided,
        layout,
        alphabet,
        transactional_errors,
    )
}

fn decode_strict_with_layout_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    if provided < layout.output_len() {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len(),
            provided,
        });
    }
    let output = unsafe { slice::from_raw_parts_mut(output, layout.output_len()) };
    if transactional_errors {
        decode_to_slice_with_layout_and_alphabet_transactional(input, output, layout, alphabet)?;
    } else {
        decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet)?;
    }
    Ok(layout.output_len())
}

fn decode_unpadded<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_unpadded(py, &BytesLike::Owned(input), alphabet);
    }
    let layout = unsafe { input.with_bytes(decode_unpadded_layout) }
        .map_err(|_| decoding_error(py, "Incorrect padding"))?;
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let (output, result) = unsafe {
        pybytes_with_len(py, layout.output_len(), |output| {
            input.with_bytes(|input| {
                let output_address = output as usize;
                let decode = move || {
                    decode_to_ptr_with_unpadded_layout(
                        input,
                        output_address as *mut u8,
                        layout,
                        alphabet,
                    )
                };
                if detach { py.detach(decode) } else { decode() }
            })
        })
    }?;
    result.map_err(|_| decoding_error(py, "Only base64 data is allowed"))?;
    Ok(output)
}

fn decode_unpadded_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return Ok(decode_unpadded_slice_into(
            &input,
            output,
            alphabet,
            transactional_errors,
        ));
    }
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_unpadded_to_ptr(input, output, provided, alphabet, transactional_errors)
        })
    })
}

fn decode_unpadded_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    if input.contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    let layout = decode_unpadded_layout(input)?;
    with_bytearray(output, || {
        let provided = unsafe { bytearray_size(output.as_ptr()) };
        decode_unpadded_with_layout_to_ptr(
            input,
            unsafe { bytearray_data(output.as_ptr()) },
            provided,
            layout,
            alphabet,
            transactional_errors,
        )
    })
}

fn decode_unpadded_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    if input.contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    let layout = decode_unpadded_layout(input)?;
    decode_unpadded_with_layout_to_ptr(
        input,
        output,
        provided,
        layout,
        alphabet,
        transactional_errors,
    )
}

fn decode_unpadded_with_layout_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    if provided < layout.output_len() {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len(),
            provided,
        });
    }
    let output = unsafe { slice::from_raw_parts_mut(output, layout.output_len()) };
    if transactional_errors {
        decode_to_slice_with_unpadded_layout_and_alphabet_transactional(
            input, output, layout, alphabet,
        )?;
    } else {
        decode_to_slice_with_unpadded_layout_and_alphabet(input, output, layout, alphabet)?;
    }
    Ok(layout.output_len())
}

pub(super) fn translate_altchars(input: &[u8], [plus, slash]: [u8; 2]) -> Option<Vec<u8>> {
    let first = memchr::memchr2(plus, slash, input)?;
    let mut translated = Vec::with_capacity(input.len());
    translated.extend_from_slice(&input[..first]);
    translated.extend(input[first..].iter().map(|&byte| {
        if byte == slash {
            b'/'
        } else if byte == plus {
            b'+'
        } else {
            byte
        }
    }));
    Some(translated)
}

pub(super) fn normalize_mime_whitespace(input: &BytesLike<'_, '_>) -> PyResult<Option<Vec<u8>>> {
    unsafe {
        input.with_bytes(|input| {
            let Some(first) = memchr::memchr3(b'\r', b'\n', b' ', input) else {
                return Ok(None);
            };
            let mut normalized = Vec::new();
            normalized
                .try_reserve_exact(input.len())
                .map_err(|_| PyMemoryError::new_err("Base64 input is too large"))?;
            normalized.extend_from_slice(&input[..first]);
            let search_start = first + 1;
            let mut start = search_start;
            for whitespace in memchr::memchr3_iter(b'\r', b'\n', b' ', &input[search_start..]) {
                let whitespace = search_start + whitespace;
                normalized.extend_from_slice(&input[start..whitespace]);
                start = whitespace + 1;
            }
            normalized.extend_from_slice(&input[start..]);
            Ok(Some(normalized))
        })
    }
}

pub(super) fn decode_strict_with_altchars<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    match altchars {
        None => decode_strict(py, input, DecodeAlphabet::Standard),
        Some([b'-', b'_']) => decode_strict(py, input, DecodeAlphabet::Mixed),
        Some(altchars) => {
            let translated =
                unsafe { input.with_bytes(|input| translate_altchars(input, altchars)) };
            if let Some(translated) = translated {
                decode_strict(py, &BytesLike::Owned(translated), DecodeAlphabet::Standard)
            } else {
                decode_strict(py, input, DecodeAlphabet::Standard)
            }
        }
    }
}

pub(super) fn decode_unpadded_with_altchars<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    match altchars {
        None => decode_unpadded(py, input, DecodeAlphabet::Standard),
        Some([b'-', b'_']) => decode_unpadded(py, input, DecodeAlphabet::Mixed),
        Some(altchars) => {
            let translated =
                unsafe { input.with_bytes(|input| translate_altchars(input, altchars)) };
            if let Some(translated) = translated {
                decode_unpadded(py, &BytesLike::Owned(translated), DecodeAlphabet::Standard)
            } else {
                decode_unpadded(py, input, DecodeAlphabet::Standard)
            }
        }
    }
}

pub(super) fn decode_unpadded_into_with_altchars(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    let translated = altchars
        .filter(|altchars| *altchars != *b"-_")
        .and_then(|altchars| unsafe {
            input.with_bytes(|input| translate_altchars(input, altchars))
        })
        .map(BytesLike::Owned);
    let direct_input = translated.as_ref().unwrap_or(input);
    let alphabet = if altchars == Some(*b"-_") {
        DecodeAlphabet::Mixed
    } else {
        DecodeAlphabet::Standard
    };
    decode_unpadded_into(direct_input, output, alphabet, transactional_errors)
}

pub(super) fn try_decode_urlsafe_315<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    strict_mode: bool,
    padded: bool,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    if (padded || !strict_mode)
        && let Some(output) = try_decode_strict(py, input, DecodeAlphabet::UrlSafe)?
    {
        return Ok(Some(output));
    }
    if !padded {
        match decode_unpadded(py, input, DecodeAlphabet::UrlSafe) {
            Ok(output) => return Ok(Some(output)),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
            Err(_) => {}
        }
    }
    Ok(None)
}

pub(super) fn try_decode_urlsafe_315_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    strict_mode: bool,
    padded: bool,
) -> PyResult<Option<usize>> {
    let transactional_errors = !strict_mode;
    if padded || !strict_mode {
        match decode_strict_into(input, output, DecodeAlphabet::UrlSafe, transactional_errors)? {
            Ok(written) => return Ok(Some(written)),
            Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                return Err(output_too_small(required, provided));
            }
            Err(Base64Error::OutputTooSmall { .. }) | Err(Base64Error::InvalidInput) => {}
        }
    }
    if !padded {
        match decode_unpadded_into(input, output, DecodeAlphabet::UrlSafe, transactional_errors)? {
            Ok(written) => return Ok(Some(written)),
            Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                return Err(output_too_small(required, provided));
            }
            Err(Base64Error::OutputTooSmall { .. }) | Err(Base64Error::InvalidInput) => {}
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{is_lenient_symbol, lenient_symbol_count};

    #[test]
    fn simd_lenient_symbol_count_matches_scalar_for_all_bytes_and_alignments() {
        let input: Vec<u8> = (0_u8..=u8::MAX).cycle().take(1024).collect();
        for altchars in [None, Some(*b"-_"), Some(*b"@#"), Some(*b"=_")] {
            for offset in 0..32 {
                for tail in 0..32 {
                    let input = &input[offset..input.len() - tail];
                    let expected = input
                        .iter()
                        .filter(|&&byte| is_lenient_symbol(byte, altchars))
                        .count();
                    assert_eq!(lenient_symbol_count(input, altchars), expected);
                }
            }
        }
    }
}
