//! Python encoding entry points and prepared encoding.

use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyAssertionError, PyOverflowError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::staging::{pybytes_with_len, with_output_ptr};
use crate::base64::{
    CustomEncodeAlphabet, STANDARD_ALPHABET, encode_to_ptr, encode_to_ptr_cached,
    encode_to_ptr_with_custom_alphabet, encode_wrapped_to_ptr_cached, encode_wrapped_to_ptr_custom,
    encoded_len,
};
use crate::bindings::buffer::{
    BytesLike, binascii_contiguous_bytes_like_exported, contiguous_bytes_like,
    contiguous_bytes_like_exported,
};
use crate::bindings::compatibility::{parse_altchars, python_at_least};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;
use crate::bindings::schema::Argument;

// The line-aware stores cross over the encode-then-move path above 4 MiB.
// Keep their code cold so adding the large-input path does not perturb the
// existing encoder's layout and throughput.
const DIRECT_WRAPPED_INPUT_THRESHOLD: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
enum EncodeAlphabet {
    Standard,
    UrlSafe,
    Custom(CustomEncodeAlphabet),
}

impl EncodeAlphabet {
    fn new(altchars: Option<[u8; 2]>) -> Self {
        match altchars {
            None => Self::Standard,
            Some(altchars) if altchars == *b"-_" => Self::UrlSafe,
            Some(altchars) => Self::Custom(CustomEncodeAlphabet::new(altchars)),
        }
    }

    fn from_table(table: [u8; 64]) -> Self {
        if table == *STANDARD_ALPHABET {
            Self::Standard
        } else if table[..62] == STANDARD_ALPHABET[..62] && table[62..] == *b"-_" {
            Self::UrlSafe
        } else {
            Self::Custom(CustomEncodeAlphabet::from_table(table))
        }
    }

    fn is_urlsafe(&self) -> bool {
        matches!(self, Self::UrlSafe)
    }
}

#[derive(Clone, Copy)]
enum EncodePadding {
    Padded,
    Unpadded,
}

impl EncodePadding {
    fn new(padded: bool) -> Self {
        if padded { Self::Padded } else { Self::Unpadded }
    }
}

#[derive(Clone, Copy)]
enum LineWrapping {
    None,
    Columns(usize),
}

impl LineWrapping {
    fn new(wrapcol: Option<usize>) -> Self {
        wrapcol.map_or(Self::None, Self::Columns)
    }
}

#[derive(Clone, Copy)]
pub(super) struct PreparedEncoder {
    alphabet: EncodeAlphabet,
    padding: EncodePadding,
    wrapping: LineWrapping,
}

impl PreparedEncoder {
    pub(super) fn new(altchars: Option<[u8; 2]>, padded: bool, wrapcol: Option<usize>) -> Self {
        Self::with_alphabet(EncodeAlphabet::new(altchars), padded, wrapcol)
    }

    fn with_alphabet(alphabet: EncodeAlphabet, padded: bool, wrapcol: Option<usize>) -> Self {
        Self {
            alphabet,
            padding: EncodePadding::new(padded),
            wrapping: LineWrapping::new(wrapcol),
        }
    }

    fn data_len(&self, input_len: usize) -> usize {
        match self.padding {
            EncodePadding::Padded => encoded_len(input_len),
            EncodePadding::Unpadded => unpadded_encoded_len(input_len),
        }
    }

    fn output_len(&self, input_len: usize) -> usize {
        let data_len = self.data_len(input_len);
        match (data_len, self.wrapping) {
            (0, _) | (_, LineWrapping::None) => data_len,
            (_, LineWrapping::Columns(width)) => data_len + (data_len - 1) / width,
        }
    }

    fn direct_wrap_width(&self, input_len: usize) -> Option<usize> {
        if input_len <= DIRECT_WRAPPED_INPUT_THRESHOLD {
            return None;
        }
        match self.wrapping {
            LineWrapping::None => None,
            LineWrapping::Columns(width) if self.data_len(input_len) > width => Some(width),
            LineWrapping::Columns(_) => None,
        }
    }

    unsafe fn encode_to_ptr(&self, input: &[u8], output: *mut u8) {
        match self.wrapping {
            LineWrapping::None => unsafe {
                encode_unwrapped_ptr::<false>(input, output, &self.alphabet, self.padding)
            },
            LineWrapping::Columns(width) => unsafe {
                encode_unwrapped_ptr::<true>(input, output, &self.alphabet, self.padding);
                wrap_encoded_ptr(output, self.data_len(input.len()), width);
            },
        }
    }

    #[cold]
    unsafe fn encode_direct_to_ptr(&self, input: &[u8], output: *mut u8, width: usize) {
        match &self.alphabet {
            EncodeAlphabet::Standard | EncodeAlphabet::UrlSafe => unsafe {
                encode_wrapped_to_ptr_cached(
                    input,
                    output,
                    self.alphabet.is_urlsafe(),
                    matches!(self.padding, EncodePadding::Padded),
                    width,
                )
            },
            EncodeAlphabet::Custom(alphabet) => unsafe {
                encode_wrapped_to_ptr_custom(
                    input,
                    output,
                    alphabet,
                    matches!(self.padding, EncodePadding::Padded),
                    width,
                )
            },
        }
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn encode_small_padded<'py>(
    py: Python<'py>,
    input: &[u8],
    encoder: &PreparedEncoder,
) -> PyResult<Bound<'py, PyBytes>> {
    debug_assert!(matches!(encoder.padding, EncodePadding::Padded));
    debug_assert!(matches!(encoder.wrapping, LineWrapping::None));
    let output_len = encoded_len(input.len());
    let (output, ()) = unsafe {
        pybytes_with_len(py, output_len, |output| {
            encode_ptr::<true>(input, output, &encoder.alphabet);
        })
    }?;
    Ok(output)
}

pub(super) fn encode<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let encoder = PreparedEncoder::new(altchars, padded, wrapcol);
    encode_with_prepared(py, input, &encoder)
}

pub(super) fn encode_with_prepared<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    encoder: &PreparedEncoder,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return encode_with_prepared(py, &BytesLike::OwnedVec(input), encoder);
    }
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let output_len = encoder.output_len(input.len());
    let (output, ()) = unsafe {
        pybytes_with_len(py, output_len, |output| {
            input.with_bytes(|input| {
                let output_address = output as usize;
                let encode = move || {
                    let output = output_address as *mut u8;
                    if let Some(width) = encoder.direct_wrap_width(input.len()) {
                        encoder.encode_direct_to_ptr(input, output, width);
                    } else {
                        encoder.encode_to_ptr(input, output);
                    }
                };
                if detach { py.detach(encode) } else { encode() }
            })
        })
    }?;
    Ok(output)
}

pub(super) fn encode_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<usize> {
    let encoder = PreparedEncoder::new(altchars, padded, wrapcol);
    if let Some(input) = input.snapshot_for_output(output)? {
        return encode_slice_into(&input, output, &encoder);
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            encode_slice_to_ptr(input, output, provided, &encoder)
        })
    }
}

pub(super) fn normalize_wrapcol(wrapcol: i128) -> PyResult<Option<usize>> {
    if wrapcol < 0 {
        return Err(PyValueError::new_err("Cannot convert negative int"));
    }
    let wrapcol = usize::try_from(wrapcol)
        .map_err(|_| PyOverflowError::new_err("Python int too large for C size_t"))?;
    if wrapcol == 0 {
        Ok(None)
    } else {
        Ok(Some((wrapcol / 4).max(1) * 4))
    }
}

fn parse_wrapcol(py: Python<'_>, wrapcol: Argument) -> PyResult<Option<usize>> {
    if wrapcol.as_ptr().is_null() {
        return Ok(None);
    }
    let indexed =
        unsafe { Bound::from_owned_ptr_or_err(py, ffi::PyNumber_Index(wrapcol.raw(py).as_ptr())) }?;
    let value = indexed
        .extract::<i128>()
        .map_err(|_| PyOverflowError::new_err("Python int too large for C size_t"))?;
    normalize_wrapcol(value)
}

#[inline]
fn unpadded_encoded_len(input_len: usize) -> usize {
    encoded_len(input_len) - usize::from(!input_len.is_multiple_of(3)) * (3 - input_len % 3)
}

fn encode_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    encoder: &PreparedEncoder,
) -> PyResult<usize> {
    let required = encoder.output_len(input.len());
    with_output_ptr(output, required, |output| {
        if let Some(width) = encoder.direct_wrap_width(input.len()) {
            unsafe { encoder.encode_direct_to_ptr(input, output, width) };
        } else {
            unsafe { encoder.encode_to_ptr(input, output) };
        }
    })?;
    Ok(required)
}

fn encode_slice_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    encoder: &PreparedEncoder,
) -> PyResult<usize> {
    let required = encoder.output_len(input.len());
    if provided < required {
        return Err(super::staging::output_too_small(required, provided));
    }
    if let Some(width) = encoder.direct_wrap_width(input.len()) {
        unsafe { encoder.encode_direct_to_ptr(input, output, width) };
    } else {
        unsafe { encoder.encode_to_ptr(input, output) };
    }
    Ok(required)
}

#[inline]
unsafe fn encode_unwrapped_ptr<const CACHED: bool>(
    input: &[u8],
    output: *mut u8,
    alphabet: &EncodeAlphabet,
    padding: EncodePadding,
) {
    if matches!(padding, EncodePadding::Padded) {
        unsafe { encode_ptr::<CACHED>(input, output, alphabet) };
        return;
    }

    let complete_input_len = input.len() / 3 * 3;
    let complete_output_len = complete_input_len / 3 * 4;
    unsafe { encode_ptr::<CACHED>(&input[..complete_input_len], output, alphabet) };
    if complete_input_len != input.len() {
        let tail = &input[complete_input_len..];
        let mut encoded_tail = [0; 4];
        unsafe { encode_ptr::<CACHED>(tail, encoded_tail.as_mut_ptr(), alphabet) };
        let tail_len = unpadded_encoded_len(tail.len());
        unsafe {
            output
                .add(complete_output_len)
                .copy_from_nonoverlapping(encoded_tail.as_ptr(), tail_len)
        };
    }
}

#[inline]
unsafe fn encode_ptr<const CACHED: bool>(input: &[u8], output: *mut u8, alphabet: &EncodeAlphabet) {
    match alphabet {
        EncodeAlphabet::Standard | EncodeAlphabet::UrlSafe => {
            if CACHED {
                unsafe { encode_to_ptr_cached(input, output, alphabet.is_urlsafe()) };
            } else {
                unsafe { encode_to_ptr(input, output, alphabet.is_urlsafe()) };
            }
        }
        EncodeAlphabet::Custom(alphabet) => unsafe {
            encode_to_ptr_with_custom_alphabet(input, output, alphabet, CACHED)
        },
    }
}

/// Expand an encoded prefix in place, moving from the end so source bytes are
/// never overwritten before they are copied.
unsafe fn wrap_encoded_ptr(output: *mut u8, data_len: usize, width: usize) {
    if data_len <= width {
        return;
    }
    let mut source = data_len;
    let mut destination = data_len + (data_len - 1) / width;
    while source != 0 {
        let remainder = source % width;
        let line_len = if remainder == 0 { width } else { remainder };
        source -= line_len;
        destination -= line_len;
        unsafe {
            output
                .add(source)
                .copy_to(output.add(destination), line_len)
        };
        if source != 0 {
            destination -= 1;
            unsafe { output.add(destination).write(b'\n') };
        }
    }
    debug_assert_eq!(destination, 0);
}

// Python 3.15 constructs a custom alphabet before consuming the input, but
// binascii consumes the constructed alphabet after the other arguments.
fn construct_b64encode_alphabet<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let length = value.len()?;
    if length != 2 {
        let value = value.repr()?.to_string();
        return Err(PyValueError::new_err(format!("invalid altchars: {value}")));
    }

    let prefix = PyBytes::new(py, &STANDARD_ALPHABET[..62]);
    unsafe { Bound::from_owned_ptr_or_err(py, ffi::PyNumber_Add(prefix.as_ptr(), value.as_ptr())) }
}

fn parse_b64encode_alphabet<'a, 'py>(
    value: &'a Bound<'py, PyAny>,
) -> PyResult<(EncodeAlphabet, BytesLike<'a, 'py>)> {
    #[cfg(Py_GIL_DISABLED)]
    let bytes = crate::bindings::buffer::binascii_contiguous_bytes_like_exported(value)?;
    #[cfg(not(Py_GIL_DISABLED))]
    let bytes = crate::bindings::buffer::binascii_contiguous_bytes_like(value)?;
    #[cfg(Py_GIL_DISABLED)]
    let bytes = bytes.into_stable_after_callbacks(false)?;
    if bytes.len() != STANDARD_ALPHABET.len() {
        return Err(PyValueError::new_err("alphabet must have length 64"));
    }

    let table = unsafe {
        bytes.with_bytes(|bytes| {
            let mut table = [0; 64];
            table.copy_from_slice(bytes);
            table
        })
    };

    Ok((EncodeAlphabet::from_table(table), bytes))
}

fn parse_legacy_b64encode_altchars(value: &Bound<'_, PyAny>) -> PyResult<Option<[u8; 2]>> {
    let length = value.len()?;
    if length != 2 {
        return Err(PyAssertionError::new_err(value.repr()?.to_string()));
    }

    let bytes = contiguous_bytes_like(value, "altchars")?;
    #[cfg(Py_GIL_DISABLED)]
    let bytes = bytes.into_stable()?;
    if bytes.len() != 2 {
        return Err(PyValueError::new_err(
            "maketrans arguments must have same length",
        ));
    }

    let altchars = unsafe { bytes.with_bytes(|bytes| [bytes[0], bytes[1]]) };
    Ok((altchars != *b"+/").then_some(altchars))
}

pub(super) fn encode_parsed<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyAny>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = crate::bindings::buffer::binascii_contiguous_bytes_like(input)?;
    encode(py, &input, altchars, padded, wrapcol)
}

/// Encode with the standard Base64 alphabet.
pub(super) fn standard_b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    encode_parsed(py, s, None, true, None)
}

/// Encode with the standard Base64 alphabet into a reusable output.
pub(super) fn standard_b64encode_into(
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(s, "s")?;
    let input = input.into_stable_after_callbacks(true)?;
    encode_into(&input, output, None, true, None)
}

/// Encode with the URL-safe Base64 alphabet.
pub(super) fn urlsafe_b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: Argument,
) -> PyResult<Bound<'py, PyBytes>> {
    if PyBytes::is_exact_type_of(s) {
        let input = BytesLike::Bytes(unsafe { s.cast_unchecked::<PyBytes>() });
        let padded = padded.truthy(py)?;
        return encode(py, &input, Some(*b"-_"), padded, None);
    }

    let input = binascii_contiguous_bytes_like_exported(s)?;
    let padded = padded.truthy(py)?;
    let input = input.into_stable_after_callbacks(false)?;
    encode(py, &input, Some(*b"-_"), padded, None)
}

/// Encode with the URL-safe Base64 alphabet into a reusable output.
pub(super) fn urlsafe_b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: Argument,
) -> PyResult<usize> {
    let input = contiguous_bytes_like_exported(s, "s")?;
    let padded = padded.truthy(py)?;
    let input = input.into_stable_after_callbacks(true)?;
    encode_into(&input, output, Some(*b"-_"), padded, None)
}

pub(super) fn b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    padded: Argument,
    wrapcol: Argument,
) -> PyResult<Bound<'py, PyBytes>> {
    if altchars.is_none() && PyBytes::is_exact_type_of(s) {
        let input = BytesLike::Bytes(unsafe { s.cast_unchecked::<PyBytes>() });
        let padded = padded.truthy(py)?;
        let wrapcol = parse_wrapcol(py, wrapcol)?;
        return encode_with_prepared(
            py,
            &input,
            &PreparedEncoder::with_alphabet(EncodeAlphabet::Standard, padded, wrapcol),
        );
    }

    let python_315 = python_at_least(py, (3, 15));
    let constructed_alphabet = if python_315 {
        altchars
            .map(|altchars| construct_b64encode_alphabet(py, altchars))
            .transpose()?
    } else {
        None
    };

    let input = binascii_contiguous_bytes_like_exported(s)?;
    let legacy_callbacks_follow_input = !python_315
        && (altchars.is_some() || !padded.as_ptr().is_null() || !wrapcol.as_ptr().is_null());
    let input = input.into_stable_before_callbacks(legacy_callbacks_follow_input)?;
    let legacy_altchars = if python_315 {
        None
    } else {
        altchars
            .map(parse_legacy_b64encode_altchars)
            .transpose()?
            .flatten()
    };
    let padded = padded.truthy(py)?;
    let wrapcol = parse_wrapcol(py, wrapcol)?;
    let parsed_alphabet = constructed_alphabet
        .as_ref()
        .map(parse_b64encode_alphabet)
        .transpose()?;
    let alphabet = parsed_alphabet.as_ref().map_or_else(
        || EncodeAlphabet::new(legacy_altchars),
        |(alphabet, _)| *alphabet,
    );
    let encoder = PreparedEncoder::with_alphabet(alphabet, padded, wrapcol);
    let input = input.into_stable_after_callbacks(false)?;
    let result = encode_with_prepared(py, &input, &encoder);
    #[cfg(Py_GIL_DISABLED)]
    {
        // CPython's free-threaded converter releases the input export before
        // the custom alphabet export after both have been consumed.
        drop(input);
        drop(parsed_alphabet);
    }
    result
}

pub(super) fn b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    padded: bool,
    wrapcol: i128,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(s, "s")?;
    let altchars = parse_altchars(py, altchars, false)?;
    let input = input.into_stable_after_callbacks(true)?;
    encode_into(
        &input,
        output,
        altchars,
        padded,
        normalize_wrapcol(wrapcol)?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_lookup_tables_cover_wrapping_byte_offsets_and_exact_boundaries() {
        let input = (0..=256)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();

        for altchars in [*b"@#", *b"==", [0, u8::MAX], [62, 63]] {
            for length in 0..=256 {
                for padded in [false, true] {
                    let encoder = PreparedEncoder::new(Some(altchars), padded, None);
                    let output_len = encoder.output_len(length);
                    let mut actual = vec![0xa5; output_len + 1];
                    unsafe { encoder.encode_to_ptr(&input[..length], actual.as_mut_ptr()) };

                    let mut expected = crate::base64::b64encode(&input[..length]).into_bytes();
                    if !padded {
                        while expected.last() == Some(&b'=') {
                            expected.pop();
                        }
                    }
                    for byte in &mut expected {
                        if *byte == b'+' {
                            *byte = altchars[0];
                        } else if *byte == b'/' {
                            *byte = altchars[1];
                        }
                    }

                    assert_eq!(&actual[..output_len], expected);
                    assert_eq!(actual[output_len], 0xa5);
                }
            }
        }
    }

    #[test]
    fn large_wrapped_custom_unpadded_output_uses_final_layout() {
        let input = (0..DIRECT_WRAPPED_INPUT_THRESHOLD + 2)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        let encoder = PreparedEncoder::new(Some(*b"@#"), false, Some(76));
        let mut actual = vec![0xa5; encoder.output_len(input.len()) + 1];
        unsafe { encoder.encode_direct_to_ptr(&input, actual.as_mut_ptr(), 76) };

        let mut contiguous = crate::base64::b64encode(&input).into_bytes();
        while contiguous.last() == Some(&b'=') {
            contiguous.pop();
        }
        for byte in &mut contiguous {
            if *byte == b'+' {
                *byte = b'@';
            } else if *byte == b'/' {
                *byte = b'#';
            }
        }

        let mut expected = Vec::with_capacity(encoder.output_len(input.len()));
        for (line, chunk) in contiguous.chunks(76).enumerate() {
            if line != 0 {
                expected.push(b'\n');
            }
            expected.extend_from_slice(chunk);
        }

        assert_eq!(&actual[..expected.len()], expected);
        assert_eq!(actual[expected.len()], 0xa5);
    }

    #[test]
    fn wrapped_arbitrary_alphabet_uses_complete_table() {
        let input = (0..=256)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        let alphabet = EncodeAlphabet::from_table([b'Z'; 64]);
        let encoder = PreparedEncoder::with_alphabet(alphabet, false, Some(76));
        let mut actual = vec![0xa5; encoder.output_len(input.len()) + 1];
        unsafe { encoder.encode_direct_to_ptr(&input, actual.as_mut_ptr(), 76) };

        let data_len = unpadded_encoded_len(input.len());
        let mut expected = Vec::with_capacity(encoder.output_len(input.len()));
        for (line, length) in (0..data_len).step_by(76).enumerate() {
            if line != 0 {
                expected.push(b'\n');
            }
            expected.extend(std::iter::repeat_n(b'Z', (data_len - length).min(76)));
        }

        assert_eq!(&actual[..expected.len()], expected);
        assert_eq!(actual[expected.len()], 0xa5);
    }

    #[test]
    fn large_wrapped_standard_padded_output_uses_final_layout() {
        let input = (0..DIRECT_WRAPPED_INPUT_THRESHOLD + 1)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        let encoder = PreparedEncoder::new(None, true, Some(76));
        let mut actual = vec![0xa5; encoder.output_len(input.len()) + 1];
        unsafe { encoder.encode_direct_to_ptr(&input, actual.as_mut_ptr(), 76) };

        let contiguous = crate::base64::b64encode(&input).into_bytes();
        let mut expected = Vec::with_capacity(encoder.output_len(input.len()));
        for (line, chunk) in contiguous.chunks(76).enumerate() {
            if line != 0 {
                expected.push(b'\n');
            }
            expected.extend_from_slice(chunk);
        }

        assert_eq!(&actual[..expected.len()], expected);
        assert_eq!(actual[expected.len()], 0xa5);
    }

    #[test]
    fn short_wrapped_custom_output_uses_scalar_fallback_and_exact_store() {
        let input = [0xfb; 15];
        let alphabet = CustomEncodeAlphabet::new(*b"@#");
        let mut actual = [0xa5; 25];
        unsafe { encode_wrapped_to_ptr_custom(&input, actual.as_mut_ptr(), &alphabet, true, 4) };

        let mut contiguous = crate::base64::b64encode(&input).into_bytes();
        for byte in &mut contiguous {
            match *byte {
                b'+' => *byte = b'@',
                b'/' => *byte = b'#',
                _ => {}
            }
        }
        let mut expected = Vec::new();
        for (line, chunk) in contiguous.chunks(4).enumerate() {
            if line != 0 {
                expected.push(b'\n');
            }
            expected.extend_from_slice(chunk);
        }

        assert_eq!(&actual[..expected.len()], expected);
        assert_eq!(actual[expected.len()], 0xa5);
    }
}
