use core::slice;

use pyo3::exceptions::PyMemoryError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::{STANDARD_ALPHABET, output_too_small, pybytes_with_len};
use super::fallback::decoding_error;
use super::output::BytesWriter;
use crate::base64::{
    Base64Error, DecodeAlphabet, DecodeLayout, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
};
use crate::bindings::buffer::{BytesLike, with_bytearray};
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
    if let Some(input) = input.snapshot_mutable() {
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
    if input.aliases(output) || input.requires_snapshot_for_output() {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
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
    if let Some(input) = input.snapshot_mutable() {
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
) -> Result<usize, Base64Error> {
    if input.aliases(output) || input.requires_snapshot_for_output() {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return decode_strict_slice_into(&input, output, alphabet, transactional_errors);
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_strict_to_ptr(input, output, provided, alphabet, transactional_errors)
        })
    }
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
    if let Some(input) = input.snapshot_mutable() {
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
) -> Result<usize, Base64Error> {
    if input.aliases(output) || input.requires_snapshot_for_output() {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return decode_unpadded_slice_into(&input, output, alphabet, transactional_errors);
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_unpadded_to_ptr(input, output, provided, alphabet, transactional_errors)
        })
    }
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
) -> Result<usize, Base64Error> {
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
        match decode_strict_into(input, output, DecodeAlphabet::UrlSafe, transactional_errors) {
            Ok(written) => return Ok(Some(written)),
            Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                return Err(output_too_small(required, provided));
            }
            Err(Base64Error::OutputTooSmall { .. }) | Err(Base64Error::InvalidInput) => {}
        }
    }
    if !padded {
        match decode_unpadded_into(input, output, DecodeAlphabet::UrlSafe, transactional_errors) {
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
