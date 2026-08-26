use core::{ptr, slice};

use pyo3::exceptions::{PyDeprecationWarning, PyFutureWarning, PyMemoryError};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyInt, PyList, PyType};

use super::{
    STANDARD_ALPHABET, batch_outputs, output_too_small, parse_altchars, pybytes_with_len,
    python_at_least, with_output_ptr,
};
use crate::base64::{
    Base64Error, DecodeAlphabet, DecodeLayout, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
};
use crate::bindings::buffer::{BytesLike, ascii_or_bytes, contiguous_bytes_like, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size, list_from_fn, list_items};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

use self::plan::{DecodeExecution, DecodeOptions, DecodeOutput, DecodePlan};

mod plan;

struct BytesWriter(*mut ffi::compat::PyBytesWriter);

impl BytesWriter {
    fn new(py: Python<'_>, input_len: usize) -> PyResult<Self> {
        let capacity = input_len
            .div_ceil(4)
            .checked_mul(3)
            .and_then(|length| ffi::Py_ssize_t::try_from(length).ok())
            .ok_or_else(|| PyMemoryError::new_err("Base64 output is too large"))?;
        let writer = unsafe { ffi::compat::PyBytesWriter_Create(capacity) };
        if writer.is_null() {
            Err(PyErr::fetch(py))
        } else {
            Ok(Self(writer))
        }
    }

    unsafe fn data(&self) -> *mut u8 {
        unsafe { ffi::compat::PyBytesWriter_GetData(self.0).cast() }
    }

    unsafe fn finish<'py>(
        mut self,
        py: Python<'py>,
        length: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let length = ffi::Py_ssize_t::try_from(length)
            .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
        let writer = self.0;
        self.0 = ptr::null_mut();
        let output = unsafe { ffi::compat::PyBytesWriter_FinishWithSize(writer, length) };
        Ok(unsafe { Bound::from_owned_ptr_or_err(py, output)?.cast_into_unchecked() })
    }
}

impl Drop for BytesWriter {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ffi::compat::PyBytesWriter_Discard(self.0) };
        }
    }
}

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

fn lenient_continues_after_padding(py: Python<'_>) -> bool {
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

fn try_decode_lenient<'py>(
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

fn try_decode_lenient_into(
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

fn try_decode_strict<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    Ok(decode_strict_native(py, input, alphabet)?.ok())
}

fn decode_strict<'py>(
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

fn decode_strict_into(
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

fn decoding_error(py: Python<'_>, message: &'static str) -> PyErr {
    match py
        .import("binascii")
        .and_then(|module| module.getattr("Error"))
        .and_then(|value| value.cast_into::<PyType>().map_err(Into::into))
    {
        Ok(error_type) => PyErr::from_type(error_type, (message,)),
        Err(error) => error,
    }
}

fn translate_altchars(input: &[u8], [plus, slash]: [u8; 2]) -> Option<Vec<u8>> {
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

fn normalize_mime_whitespace(input: &BytesLike<'_, '_>) -> PyResult<Option<Vec<u8>>> {
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

fn decode_strict_with_altchars<'py>(
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

fn decode_unpadded_with_altchars<'py>(
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

fn decode_unpadded_into_with_altchars(
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

#[inline]
fn warn_legacy_altchars(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    ignorechars_specified: bool,
    strict_mode: bool,
) -> PyResult<()> {
    if ignorechars_specified {
        return Ok(());
    }
    let Some(altchars) = altchars else {
        return Ok(());
    };
    if !python_at_least(py, (3, 15)) {
        return Ok(());
    }
    let badchar = unsafe {
        input.with_bytes(|input| {
            b"+/"
                .iter()
                .copied()
                .find(|byte| !altchars.contains(byte) && input.contains(byte))
        })
    };
    let Some(badchar) = badchar else {
        return Ok(());
    };
    let mode = if strict_mode { "True" } else { "False" };
    let outcome = if strict_mode {
        "will be an error"
    } else {
        "will be discarded"
    };
    let altchars = PyBytes::new(py, &altchars).repr()?.to_string();
    let message = format!(
        "invalid character '{}' in Base64 data with altchars={altchars} and validate={mode} {outcome} in future Python versions",
        char::from(badchar),
    );
    let category = if strict_mode {
        py.get_type::<PyDeprecationWarning>()
    } else {
        py.get_type::<PyFutureWarning>()
    };
    py.import("warnings")?
        .call_method1("warn", (message, category, 1))?;
    Ok(())
}

fn decode_with_binascii<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable() {
        return decode_with_binascii(
            py,
            &BytesLike::Owned(input),
            altchars,
            strict_mode,
            padded,
            ignorechars,
            canonical,
        );
    }
    if !python_at_least(py, (3, 15)) && (!padded || ignorechars.is_some() || canonical) {
        return decode_advanced_legacy(
            py,
            input,
            altchars,
            strict_mode,
            padded,
            ignorechars,
            canonical,
        );
    }
    let custom_alphabet = altchars.is_some() && ignorechars.is_some();
    let translated = if custom_alphabet {
        None
    } else {
        altchars.and_then(|altchars| unsafe {
            input.with_bytes(|input| translate_altchars(input, altchars))
        })
    };
    let data = if let Some(translated) = translated.as_deref() {
        PyBytes::new(py, translated)
    } else {
        unsafe { input.with_bytes(|input| PyBytes::new(py, input)) }
    };
    let input = data.as_bytes();
    let decode = py
        .import(intern!(py, "binascii"))?
        .getattr(intern!(py, "a2b_base64"))?;
    let output = if python_at_least(py, (3, 15)) {
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict_mode", strict_mode)?;
        kwargs.set_item("padded", padded)?;
        kwargs.set_item("canonical", canonical)?;
        if let Some(ignorechars) = ignorechars {
            kwargs.set_item("ignorechars", ignorechars)?;
        } else {
            kwargs.set_item("ignorechars", b"")?;
        }
        if let Some([plus, slash]) = altchars.filter(|_| custom_alphabet) {
            let mut alphabet = *STANDARD_ALPHABET;
            alphabet[62] = plus;
            alphabet[63] = slash;
            kwargs.set_item("alphabet", PyBytes::new(py, &alphabet))?;
        }
        decode.call((data,), Some(&kwargs))?
    } else if !python_at_least(py, (3, 11)) {
        if strict_mode && !strict_base64_310(input) {
            return Err(decoding_error(py, "Non-base64 digit found"));
        }
        decode.call1((data,))?
    } else {
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict_mode", strict_mode)?;
        decode.call((data,), Some(&kwargs))?
    };
    output.cast_into::<PyBytes>().map_err(Into::into)
}

fn decode_advanced_legacy<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
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

    let mut decode_map = [-1_i16; 256];
    for (value, &byte) in STANDARD_ALPHABET[..62].iter().enumerate() {
        decode_map[usize::from(byte)] = value as i16;
    }
    let custom_alphabet = altchars.is_some() && ignorechars.is_some();
    if !custom_alphabet {
        decode_map[usize::from(b'+')] = 62;
        decode_map[usize::from(b'/')] = 63;
    }
    if let Some([plus, slash]) = altchars {
        decode_map[usize::from(plus)] = 62;
        decode_map[usize::from(slash)] = 63;
    }

    let mut normalized = Vec::with_capacity(input.len() + 2);
    unsafe {
        input.with_bytes(|input| {
            for &byte in input {
                let value = decode_map[usize::from(byte)];
                if value >= 0 {
                    normalized.push(STANDARD_ALPHABET[value as usize]);
                } else if byte == b'=' {
                    normalized.push(byte);
                } else if strict_mode && !ignored[usize::from(byte)] {
                    return Err(decoding_error(py, "Only base64 data is allowed"));
                }
            }
            Ok(())
        })
    }?;

    let data_len = normalized
        .iter()
        .position(|&byte| byte == b'=')
        .unwrap_or(normalized.len());
    if !padded && strict_mode && data_len != normalized.len() {
        return Err(decoding_error(py, "Padding not allowed"));
    }
    if data_len % 4 == 1 {
        return Err(decoding_error(py, "Incorrect padding"));
    }
    if canonical && !canonical_padding(&normalized[..data_len]) {
        return Err(decoding_error(py, "Non-zero padding bits"));
    }
    if !padded && data_len == normalized.len() {
        normalized.resize(normalized.len() + (4 - data_len % 4) % 4, b'=');
    } else if !padded {
        let required_padding = (4 - data_len % 4) % 4;
        let present_padding = normalized.len() - data_len;
        if present_padding < required_padding {
            normalized.resize(normalized.len() + required_padding - present_padding, b'=');
        }
    }
    decode_with_binascii(
        py,
        &BytesLike::Owned(normalized),
        None,
        strict_mode,
        true,
        None,
        false,
    )
}

fn canonical_padding(input: &[u8]) -> bool {
    let Some(&last) = input.last() else {
        return true;
    };
    let value = STANDARD_ALPHABET
        .iter()
        .position(|&byte| byte == last)
        .expect("normalized Base64 input uses the standard alphabet");
    match input.len() % 4 {
        2 => value & 0x0f == 0,
        3 => value & 0x03 == 0,
        _ => true,
    }
}

fn copy_decoded_into(
    decoded: &Bound<'_, PyBytes>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let decoded = decoded.as_bytes();
    with_output_ptr(output, decoded.len(), |output| {
        let output = unsafe { slice::from_raw_parts_mut(output, decoded.len()) };
        output.copy_from_slice(decoded);
    })?;
    Ok(decoded.len())
}

fn strict_base64_310(input: &[u8]) -> bool {
    let padding = input
        .iter()
        .position(|&byte| byte == b'=')
        .unwrap_or(input.len());
    input[..padding]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        && input[padding..].len() <= 2
        && input[padding..].iter().all(|&byte| byte == b'=')
}

fn try_decode_urlsafe_315<'py>(
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

fn decode_plan_allocating_inner<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    options: DecodeOptions<'_, 'py>,
) -> PyResult<Bound<'py, PyBytes>> {
    let DecodeOptions {
        altchars,
        padded,
        ignorechars,
        canonical,
        ..
    } = options;
    let strict_mode = options.strict_mode();
    let empty_ignorechars = ignorechars.is_some_and(|value| {
        value
            .cast::<PyBytes>()
            .is_ok_and(|bytes| bytes.as_bytes().is_empty())
    });
    if altchars.is_none()
        && padded
        && ignorechars.is_none_or(|_| empty_ignorechars)
        && (canonical || empty_ignorechars)
    {
        match decode_strict(py, input, DecodeAlphabet::Standard) {
            Ok(output) => {
                let canonical_input = !canonical
                    || unsafe {
                        input.with_bytes(|input| {
                            // A successful strict decode guarantees that padding is
                            // confined to the final two bytes.
                            let padding = usize::from(input.ends_with(b"="))
                                + usize::from(input.ends_with(b"=="));
                            let data_len = input.len() - padding;
                            canonical_padding(&input[..data_len])
                        })
                    };
                if canonical_input {
                    return Ok(output);
                }
                return Err(decoding_error(py, "Non-zero padding bits"));
            }
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
            Err(_) => {}
        }
    }

    if ignorechars.is_none() && !canonical && strict_mode {
        if !padded {
            return match decode_unpadded_with_altchars(py, input, altchars) {
                Ok(output) => Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
                Err(_) => decode_with_binascii(py, input, altchars, true, false, None, false),
            };
        }
        return match decode_strict_with_altchars(py, input, altchars) {
            Ok(output) => Ok(output),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
            Err(_) => decode_with_binascii(py, input, altchars, true, true, None, false),
        };
    }

    if ignorechars.is_none() && !canonical && !strict_mode {
        let direct = match altchars {
            None => Some(DecodeAlphabet::Standard),
            Some([b'-', b'_']) => Some(DecodeAlphabet::Mixed),
            Some(_) => None,
        };
        if let Some(alphabet) = direct
            && let Some(output) = try_decode_strict(py, input, alphabet)?
        {
            return Ok(output);
        }
        if padded
            && let Some(alphabet) = direct
            && let Some(normalized) = normalize_mime_whitespace(input)?
            && let Some(output) = try_decode_strict(py, &BytesLike::Owned(normalized), alphabet)?
        {
            return Ok(output);
        }
        if !padded {
            match decode_unpadded_with_altchars(py, input, altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        }
        if let Some(output) = try_decode_lenient(py, input, altchars, padded)? {
            return Ok(output);
        }
    }
    decode_with_binascii(
        py,
        input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )
}

pub(super) fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: Option<bool>,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let options = DecodeOptions::new(altchars, validate, padded, ignorechars, canonical);
    DecodePlan::new(&input, options)
        .execute(py, DecodeExecution::Allocate)
        .map(DecodeOutput::into_bytes)
}

/// Decode with the standard Base64 alphabet.
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::standard())
        .execute(py, DecodeExecution::Allocate)
        .map(DecodeOutput::into_bytes)
}

/// Decode standard Base64 into a reusable output.
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::standard())
        .execute(py, DecodeExecution::Into(output))
        .map(DecodeOutput::into_written)
}

pub(super) fn urlsafe_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::urlsafe(padded))
        .execute(py, DecodeExecution::Allocate)
        .map(DecodeOutput::into_bytes)
}

pub(super) fn urlsafe_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::urlsafe(padded))
        .execute(py, DecodeExecution::Into(output))
        .map(DecodeOutput::into_written)
}

/// Decode each ASCII string or bytes-like item and return results in input order.
///
/// ``items`` must be a list. ``altchars`` and ``validate`` apply to every item.
/// Processing is fail-fast: an error discards the partial result and is raised
/// immediately. Processing is single-threaded. Immutable items of at least
/// 256 KiB release the GIL independently; smaller and mutable items do not. Do
/// not mutate ``items`` concurrently while this function is running.
pub(super) fn b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    b64decode_batch_parsed(py, items, altchars, validate)
}

fn b64decode_batch_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items);
    let length = items.len();
    let mut items = items.into_iter();
    let options = DecodeOptions::new(altchars, Some(validate), true, None, false);
    list_from_fn(py, length, |_| {
        let item = items.next().expect("batch item count is exact");
        let input = ascii_or_bytes(py, &item, "s")?;
        DecodePlan::new(&input, options)
            .execute(py, DecodeExecution::Allocate)
            .map(DecodeOutput::into_bytes)
    })
}

pub(super) fn standard_b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_parsed(py, items, None, false)
}

pub(super) fn urlsafe_b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_parsed(py, items, Some(*b"-_"), false)
}

fn try_decode_urlsafe_315_into(
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

fn decode_plan_into_inner(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<usize> {
    let DecodeOptions {
        altchars,
        padded,
        ignorechars,
        canonical,
        ..
    } = options;
    let strict_mode = options.strict_mode();
    let transactional_errors = !strict_mode;
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

    let direct = if ignorechars.is_none() && !canonical && (padded || !strict_mode) {
        decode_strict_into(direct_input, output, alphabet, transactional_errors)
    } else {
        Err(Base64Error::InvalidInput)
    };
    match direct {
        Ok(written) => return Ok(written),
        Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
            return Err(output_too_small(required, provided));
        }
        Err(Base64Error::OutputTooSmall { .. }) => {}
        Err(Base64Error::InvalidInput) => {}
    }

    if padded
        && !strict_mode
        && ignorechars.is_none()
        && !canonical
        && matches!(altchars, None | Some([b'-', b'_']))
        && let Some(normalized) = normalize_mime_whitespace(input)?
    {
        let normalized = BytesLike::Owned(normalized);
        match decode_strict_into(&normalized, output, alphabet, true) {
            Ok(written) => return Ok(written),
            Err(Base64Error::OutputTooSmall { .. }) | Err(Base64Error::InvalidInput) => {}
        }
    }

    if !padded && ignorechars.is_none() && !canonical {
        match decode_unpadded_into_with_altchars(input, output, altchars, transactional_errors) {
            Ok(written) => return Ok(written),
            Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                return Err(output_too_small(required, provided));
            }
            Err(Base64Error::OutputTooSmall { .. }) => {}
            Err(Base64Error::InvalidInput) => {}
        }
    }

    if !strict_mode
        && ignorechars.is_none()
        && !canonical
        && let Some(written) = try_decode_lenient_into(
            input,
            output,
            altchars,
            padded,
            lenient_continues_after_padding(py),
        )?
    {
        return Ok(written);
    }

    let decoded = decode_with_binascii(
        py,
        input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )?;
    copy_decoded_into(&decoded, output)
}

/// Decode each item into its matching reusable bytearray and return byte counts.
///
/// ``items`` and ``outputs`` must be equal-length lists, and destinations must
/// be distinct bytearrays. Each destination keeps its size; only its written
/// prefix is changed. Processing is fail-fast and non-transactional: an error
/// leaves earlier destinations modified, and the failing destination may be
/// partly written. The GIL remains held because outputs are mutable. Inputs are
/// snapshotted before the first destination write.
pub(super) fn b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    b64decode_batch_into_parsed(py, items, outputs, altchars, validate)
}

fn b64decode_batch_into_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items);
    let outputs = batch_outputs(items.len(), outputs)?;
    let inputs = items
        .iter()
        .map(|item| ascii_or_bytes(py, item, "s").map(BytesLike::into_stable_for_batch_output))
        .collect::<PyResult<Vec<_>>>()?;
    let length = inputs.len();
    let mut pairs = inputs.into_iter().zip(outputs.iter());
    let options = DecodeOptions::new(altchars, Some(validate), true, None, false);
    list_from_fn(py, length, |_| {
        let (input, output) = pairs.next().expect("batch item count is exact");
        Ok(PyInt::new(
            py,
            DecodePlan::new(&input, options)
                .execute(py, DecodeExecution::Into(output))?
                .into_written(),
        ))
    })
}

pub(super) fn standard_b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_into_parsed(py, items, outputs, None, false)
}

pub(super) fn urlsafe_b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_into_parsed(py, items, outputs, Some(*b"-_"), false)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    validate: Option<bool>,
    padded: bool,
    ignorechars: Option<&Bound<'_, PyAny>>,
    canonical: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let options = DecodeOptions::new(altchars, validate, padded, ignorechars, canonical);
    DecodePlan::new(&input, options)
        .execute(py, DecodeExecution::Into(output))
        .map(DecodeOutput::into_written)
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
