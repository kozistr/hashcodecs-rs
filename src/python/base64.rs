use core::slice;
use std::collections::HashSet;
use std::sync::OnceLock;

use pyo3::PyTypeInfo;
use pyo3::exceptions::{
    PyAssertionError, PyDeprecationWarning, PyFutureWarning, PyMemoryError, PyTypeError,
    PyValueError,
};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyByteArrayMethods, PyBytes, PyDict, PyList, PyType};

use super::DETACH_THRESHOLD;
use super::buffer::{BytesLike, ascii_or_bytes, contiguous_bytes_like};
use crate::base64::{
    Base64Error, DecodeAlphabet, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
};

mod encode;

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static PYTHON_VERSION: OnceLock<(u8, u8)> = OnceLock::new();

#[inline]
pub(super) fn python_at_least(py: Python<'_>, version: (u8, u8)) -> bool {
    *PYTHON_VERSION.get_or_init(|| {
        let version_info = py.version_info();
        (version_info.major, version_info.minor)
    }) >= version
}

fn extract_truthy(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    value.is_truthy()
}

fn extract_optional_truthy(value: &Bound<'_, PyAny>) -> PyResult<Option<bool>> {
    value.is_truthy().map(Some)
}

fn extract_optional_object(value: &Bound<'_, PyAny>) -> PyResult<Option<Py<PyAny>>> {
    Ok(Some(value.clone().unbind()))
}

fn parse_altchars(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
    allow_text: bool,
) -> PyResult<Option<[u8; 2]>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let bytes = if allow_text {
        ascii_or_bytes(py, value, "altchars")?
    } else {
        contiguous_bytes_like(py, value, "altchars")?
    };
    if bytes.len() != 2 {
        if python_at_least(py, (3, 15)) {
            let value = if allow_text {
                unsafe { bytes.with_bytes(|bytes| PyBytes::new(py, bytes).repr()) }?.to_string()
            } else {
                value.repr()?.to_string()
            };
            return Err(PyValueError::new_err(format!("invalid altchars: {value}",)));
        }
        return Err(PyAssertionError::new_err(
            "altchars must be a bytes-like object or ASCII string of length 2",
        ));
    }
    let altchars = unsafe { bytes.with_bytes(|bytes| [bytes[0], bytes[1]]) };
    Ok((altchars != *b"+/").then_some(altchars))
}

type PreparedAltchars = Result<Option<[u8; 2]>, PyErr>;

// Python 3.15 constructs a custom alphabet before consuming the input, but
// binascii validates its byte length afterward. The inner result preserves
// that otherwise-observable error ordering without sending valid calls back
// through Python's encoder.
fn prepare_b64encode_altchars(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<PreparedAltchars> {
    let length = value.len()?;
    if length != 2 {
        let value = value.repr()?.to_string();
        if python_at_least(py, (3, 15)) {
            return Err(PyValueError::new_err(format!("invalid altchars: {value}")));
        }
        return Err(PyAssertionError::new_err(value));
    }

    if python_at_least(py, (3, 15))
        && !PyBytes::is_exact_type_of(value)
        && !PyByteArray::is_exact_type_of(value)
    {
        let prefix = PyBytes::new(py, &STANDARD_ALPHABET[..62]);
        let alphabet = unsafe {
            Bound::from_owned_ptr_or_err(py, ffi::PyNumber_Add(prefix.as_ptr(), value.as_ptr()))?
        };
        let alphabet = match alphabet.cast_into::<PyBytes>() {
            Ok(alphabet) => alphabet,
            Err(error) => return Ok(Err(error.into())),
        };
        if alphabet.as_bytes().len() != STANDARD_ALPHABET.len() {
            return Ok(Err(PyValueError::new_err("alphabet must have length 64")));
        }
        let alphabet = alphabet.as_bytes();
        let altchars = [alphabet[62], alphabet[63]];
        return Ok(Ok((altchars != *b"+/").then_some(altchars)));
    }

    let bytes = contiguous_bytes_like(py, value, "altchars")?;
    if bytes.len() != 2 {
        let message = if python_at_least(py, (3, 15)) {
            "alphabet must have length 64"
        } else {
            "maketrans arguments must have same length"
        };
        return Err(PyValueError::new_err(message));
    }
    let altchars = unsafe { bytes.with_bytes(|bytes| [bytes[0], bytes[1]]) };
    Ok(Ok((altchars != *b"+/").then_some(altchars)))
}

/// Allocate an uninitialized Python `bytes` payload for direct initialization.
///
/// # Safety
/// If the returned Python object can escape, `init` must have initialized all
/// `length` bytes. An initialization error may leave bytes unwritten only when
/// the caller discards the object without reading its payload.
unsafe fn pybytes_with_len<'py, T>(
    py: Python<'py>,
    length: usize,
    init: impl FnOnce(*mut u8) -> T,
) -> PyResult<(Bound<'py, PyBytes>, T)> {
    let length = ffi::Py_ssize_t::try_from(length)
        .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
    unsafe {
        let raw = ffi::PyBytes_FromStringAndSize(core::ptr::null(), length);
        let bytes: Bound<'py, PyBytes> =
            Bound::from_owned_ptr_or_err(py, raw)?.cast_into_unchecked();
        let buffer = ffi::PyBytes_AsString(raw).cast::<u8>();
        debug_assert!(!buffer.is_null());

        // CPython leaves the payload uninitialized when passed a null source.
        // Keep it behind a raw pointer until the initializer has written every
        // byte instead of creating a Rust `&mut [u8]` with invalid contents.
        let initialized = init(buffer);
        Ok((bytes, initialized))
    }
}

fn output_ptr(output: &Bound<'_, PyByteArray>, required: usize) -> PyResult<*mut u8> {
    let provided = output.len();
    if provided < required {
        return Err(output_too_small(required, provided));
    }
    Ok(unsafe { ffi::PyByteArray_AsString(output.as_ptr()).cast() })
}

fn output_too_small(required: usize, provided: usize) -> PyErr {
    PyValueError::new_err(format!(
        "Base64 output requires {required} bytes but the destination has {provided}"
    ))
}

fn decode_strict<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Bound<'py, PyBytes>> {
    let layout = unsafe { input.with_bytes(decode_layout) }
        .map_err(|_| decoding_error(py, "Incorrect padding"))?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    let (output, result) = unsafe {
        pybytes_with_len(py, layout.output_len, |output| {
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
    result.map_err(|_| decoding_error(py, "Only base64 data is allowed"))?;
    Ok(output)
}

fn decode_strict_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    transactional_errors: bool,
) -> Result<usize, Base64Error> {
    if input.aliases(output) {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return decode_strict_slice_into(&input, output, alphabet, transactional_errors);
    }
    unsafe {
        input.with_bytes(|input| {
            decode_strict_slice_into(input, output, alphabet, transactional_errors)
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
    let provided = output.len();
    if provided < layout.output_len {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len,
            provided,
        });
    }
    let output = unsafe {
        slice::from_raw_parts_mut(
            ffi::PyByteArray_AsString(output.as_ptr()).cast(),
            layout.output_len,
        )
    };
    let output = &mut output[..layout.output_len];
    if transactional_errors {
        decode_to_slice_with_layout_and_alphabet_transactional(input, output, layout, alphabet)?;
    } else {
        decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet)?;
    }
    Ok(layout.output_len)
}

fn decode_unpadded<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Bound<'py, PyBytes>> {
    let layout = unsafe { input.with_bytes(decode_unpadded_layout) }
        .map_err(|_| decoding_error(py, "Incorrect padding"))?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    let (output, result) = unsafe {
        pybytes_with_len(py, layout.output_len, |output| {
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
    if input.aliases(output) {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return decode_unpadded_slice_into(&input, output, alphabet, transactional_errors);
    }
    unsafe {
        input.with_bytes(|input| {
            decode_unpadded_slice_into(input, output, alphabet, transactional_errors)
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
    let provided = output.len();
    if provided < layout.output_len {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len,
            provided,
        });
    }
    let output = unsafe {
        slice::from_raw_parts_mut(
            ffi::PyByteArray_AsString(output.as_ptr()).cast(),
            layout.output_len,
        )
    };
    if transactional_errors {
        decode_to_slice_with_unpadded_layout_and_alphabet_transactional(
            input, output, layout, alphabet,
        )?;
    } else {
        decode_to_slice_with_unpadded_layout_and_alphabet(input, output, layout, alphabet)?;
    }
    Ok(layout.output_len)
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
    let output =
        unsafe { slice::from_raw_parts_mut(output_ptr(output, decoded.len())?, decoded.len()) };
    output.copy_from_slice(decoded);
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

fn batch_results<T>(length: usize) -> PyResult<Vec<T>> {
    let mut results = Vec::new();
    results
        .try_reserve_exact(length)
        .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
    Ok(results)
}

fn batch_outputs<'py>(
    items_length: usize,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Vec<Bound<'py, PyByteArray>>> {
    if outputs.len() != items_length {
        return Err(PyValueError::new_err(
            "items and outputs must have the same length",
        ));
    }

    let mut parsed = batch_results(outputs.len())?;
    let mut identities = HashSet::new();
    identities
        .try_reserve(outputs.len())
        .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
    for (index, output) in outputs.iter().enumerate() {
        let output = output
            .cast_into::<PyByteArray>()
            .map_err(|_| PyTypeError::new_err(format!("outputs[{index}] must be a bytearray")))?;
        if !identities.insert(output.as_ptr()) {
            return Err(PyValueError::new_err(
                "outputs must contain distinct bytearrays",
            ));
        }
        parsed.push(output);
    }
    Ok(parsed)
}

fn encode_parsed<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyAny>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = contiguous_bytes_like(py, input, "s")?;
    encode::encode(py, &input, altchars, padded, wrapcol)
}

fn encode_parsed_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<usize> {
    encode::encode_into(input, output, altchars, padded, wrapcol)
}

/// Encode with the standard Base64 alphabet.
#[pyfunction]
pub(super) fn standard_b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    encode_parsed(py, s, None, true, None)
}

/// Encode with the standard Base64 alphabet into a reusable output.
#[pyfunction]
pub(super) fn standard_b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode_parsed_into(&input, output, None, true, None)
}

/// Encode with the URL-safe Base64 alphabet.
#[pyfunction(signature = (s, *, padded=true))]
pub(super) fn urlsafe_b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    encode_parsed(py, s, Some(*b"-_"), padded, None)
}

/// Encode with the URL-safe Base64 alphabet into a reusable output.
#[pyfunction(signature = (s, output, *, padded=true))]
pub(super) fn urlsafe_b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode_parsed_into(&input, output, Some(*b"-_"), padded, None)
}

#[pyfunction(signature = (s, altchars=None, *, padded=true, wrapcol=0))]
pub(super) fn b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
    wrapcol: i128,
) -> PyResult<Bound<'py, PyBytes>> {
    let Some(altchars) = altchars else {
        let input = contiguous_bytes_like(py, s, "s")?;
        let wrapcol = encode::normalize_wrapcol(wrapcol)?;
        return encode::encode(py, &input, None, padded, wrapcol);
    };
    let parse_altchars_first = python_at_least(py, (3, 15));
    let parsed_altchars = parse_altchars_first
        .then(|| prepare_b64encode_altchars(py, altchars))
        .transpose()?;
    let input = contiguous_bytes_like(py, s, "s")?;
    let altchars = if let Some(parsed_altchars) = parsed_altchars {
        parsed_altchars?
    } else {
        prepare_b64encode_altchars(py, altchars)??
    };
    let wrapcol = encode::normalize_wrapcol(wrapcol)?;
    encode::encode(py, &input, altchars, padded, wrapcol)
}

/// Encode each bytes-like item and return results in input order.
///
/// ``items`` must be a list. ``altchars`` applies to every item. Processing is
/// fail-fast: an error discards the partial result and is raised immediately.
/// Processing is single-threaded. Immutable items of at least 64 KiB release
/// the GIL independently; smaller and mutable items do not. Do not mutate
/// ``items`` concurrently while this function is running.
#[pyfunction(signature = (items, altchars=None))]
pub(super) fn b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    let mut encoded = batch_results(items.len())?;
    for item in items.iter() {
        encoded.push(encode_parsed(py, &item, altchars, true, None)?);
    }
    PyList::new(py, encoded)
}

/// Encode each item into its matching reusable bytearray and return byte counts.
///
/// ``items`` and ``outputs`` must be equal-length lists, and destinations must
/// be distinct bytearrays. Each destination keeps its size; only its written
/// prefix is changed. Processing is fail-fast and non-transactional: an error
/// leaves earlier destinations modified. The GIL remains held because outputs
/// are mutable. Do not share backing storage across different item/output pairs.
#[pyfunction(signature = (items, outputs, altchars=None))]
pub(super) fn b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut written = batch_results(items.len())?;
    for (item, output) in items.iter().zip(outputs.iter()) {
        let input = contiguous_bytes_like(py, &item, "s")?;
        written.push(encode_parsed_into(&input, output, altchars, true, None)?);
    }
    PyList::new(py, written)
}

#[pyfunction(signature = (s, output, altchars=None, *, padded=true, wrapcol=0))]
pub(super) fn b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
    wrapcol: i128,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, false)?;
    encode_parsed_into(
        &input,
        output,
        altchars,
        padded,
        encode::normalize_wrapcol(wrapcol)?,
    )
}

fn decode_parsed<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    if ignorechars.is_none()
        && !canonical
        && altchars == Some(*b"-_")
        && python_at_least(py, (3, 15))
        && let Some(output) = try_decode_urlsafe_315(py, input, strict_mode, padded)?
    {
        // A successful strict URL-safe decode proves that no legacy `+` or
        // `/` characters were present, so no warning scan is necessary.
        return Ok(output);
    }
    let output = decode_parsed_inner(
        py,
        input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )?;
    warn_legacy_altchars(py, input, altchars, ignorechars.is_some(), strict_mode)?;
    Ok(output)
}

fn try_decode_urlsafe_315<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    strict_mode: bool,
    padded: bool,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    if padded || !strict_mode {
        match decode_strict(py, input, DecodeAlphabet::UrlSafe) {
            Ok(output) => return Ok(Some(output)),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
            Err(_) => {}
        }
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

#[allow(clippy::too_many_arguments)]
fn decode_parsed_inner<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
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
        if let Some(alphabet) = direct {
            match decode_strict(py, input, alphabet) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
            if padded && let Some(normalized) = normalize_mime_whitespace(input)? {
                match decode_strict(py, &BytesLike::Owned(normalized), alphabet) {
                    Ok(output) => return Ok(output),
                    Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                    Err(_) => {}
                }
            }
        }
        if !padded {
            match decode_unpadded_with_altchars(py, input, altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
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

#[pyfunction(
    signature = (s, altchars=None, validate=None, *, padded=true, ignorechars=None, canonical=false),
    text_signature = "(s, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)"
)]
pub(super) fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    #[pyo3(from_py_with = extract_optional_truthy)] validate: Option<bool>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
    #[pyo3(from_py_with = extract_optional_object)] ignorechars: Option<Py<PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let strict_mode = validate.unwrap_or(ignorechars.is_some());
    let ignorechars = ignorechars.as_ref().map(|value| value.bind(py));
    decode_parsed(
        py,
        &input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )
}

/// Decode with the standard Base64 alphabet.
#[pyfunction]
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    decode_parsed(py, &input, None, false, true, None, false)
}

/// Decode standard Base64 into a reusable output.
#[pyfunction]
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    decode_parsed_into(py, &input, output, None, false, true, None, false)
}

fn urlsafe_b64decode_impl<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    decode_parsed(py, &input, Some(*b"-_"), false, padded, None, false)
}

fn urlsafe_b64decode_into_impl(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    decode_parsed_into(py, &input, output, Some(*b"-_"), false, padded, None, false)
}

/// Decode each ASCII string or bytes-like item and return results in input order.
///
/// ``items`` must be a list. ``altchars`` and ``validate`` apply to every item.
/// Processing is fail-fast: an error discards the partial result and is raised
/// immediately. Processing is single-threaded. Immutable items of at least
/// 64 KiB release the GIL independently; smaller and mutable items do not. Do
/// not mutate ``items`` concurrently while this function is running.
#[pyfunction(signature = (items, altchars=None, validate=false))]
pub(super) fn b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    let mut decoded = batch_results(items.len())?;
    for item in items.iter() {
        let input = ascii_or_bytes(py, &item, "s")?;
        decoded.push(decode_parsed(
            py, &input, altchars, validate, true, None, false,
        )?);
    }
    PyList::new(py, decoded)
}

#[allow(clippy::too_many_arguments)]
fn decode_parsed_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
    padded: bool,
    ignorechars: Option<&Bound<'_, PyAny>>,
    canonical: bool,
) -> PyResult<usize> {
    if ignorechars.is_none()
        && !canonical
        && altchars == Some(*b"-_")
        && python_at_least(py, (3, 15))
        && let Some(written) = try_decode_urlsafe_315_into(input, output, strict_mode, padded)?
    {
        // The strict URL-safe decoder rejects legacy standard-alphabet bytes.
        return Ok(written);
    }
    let written = decode_parsed_into_inner(
        py,
        input,
        output,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )?;
    warn_legacy_altchars(py, input, altchars, ignorechars.is_some(), strict_mode)?;
    Ok(written)
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

#[allow(clippy::too_many_arguments)]
fn decode_parsed_into_inner(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
    padded: bool,
    ignorechars: Option<&Bound<'_, PyAny>>,
    canonical: bool,
) -> PyResult<usize> {
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
/// partly written. The GIL remains held because outputs are mutable. Do not
/// share backing storage across different item/output pairs.
#[pyfunction(signature = (items, outputs, altchars=None, validate=false))]
pub(super) fn b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut written = batch_results(items.len())?;
    for (item, output) in items.iter().zip(outputs.iter()) {
        let input = ascii_or_bytes(py, &item, "s")?;
        written.push(decode_parsed_into(
            py, &input, output, altchars, validate, true, None, false,
        )?);
    }
    PyList::new(py, written)
}

#[pyfunction(signature = (s, output, altchars=None, validate=None, *, padded=true, ignorechars=None, canonical=false))]
#[allow(clippy::too_many_arguments)]
pub(super) fn b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    #[pyo3(from_py_with = extract_optional_truthy)] validate: Option<bool>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
    #[pyo3(from_py_with = extract_optional_object)] ignorechars: Option<Py<PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] canonical: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let strict_mode = validate.unwrap_or(ignorechars.is_some());
    let ignorechars = ignorechars.as_ref().map(|value| value.bind(py));
    decode_parsed_into(
        py,
        &input,
        output,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )
}

#[pyfunction(name = "urlsafe_b64decode", signature = (s, *, padded=true))]
/// Decode with the URL-safe Base64 alphabet.
pub(super) fn urlsafe_b64decode_pre_315<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    urlsafe_b64decode_impl(py, s, padded)
}

#[pyfunction(name = "urlsafe_b64decode", signature = (s, *, padded=false))]
/// Decode with the URL-safe Base64 alphabet.
pub(super) fn urlsafe_b64decode_315<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    urlsafe_b64decode_impl(py, s, padded)
}

#[pyfunction(name = "urlsafe_b64decode_into", signature = (s, output, *, padded=true))]
/// Decode URL-safe Base64 into a reusable output.
pub(super) fn urlsafe_b64decode_into_pre_315(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
) -> PyResult<usize> {
    urlsafe_b64decode_into_impl(py, s, output, padded)
}

#[pyfunction(name = "urlsafe_b64decode_into", signature = (s, output, *, padded=false))]
/// Decode URL-safe Base64 into a reusable output.
pub(super) fn urlsafe_b64decode_into_315(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
) -> PyResult<usize> {
    urlsafe_b64decode_into_impl(py, s, output, padded)
}

#[cfg(test)]
mod tests {
    use super::batch_results;

    #[test]
    fn oversized_batch_capacity_is_an_error() {
        assert!(batch_results::<u8>(usize::MAX).is_err());
    }
}
