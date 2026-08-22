use core::slice;

use pyo3::exceptions::{PyDeprecationWarning, PyFutureWarning, PyMemoryError};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyType};

use super::{
    STANDARD_ALPHABET, batch_outputs, batch_results, output_ptr, output_too_small, parse_altchars,
    pybytes_with_len, python_at_least,
};
use crate::base64::{
    Base64Error, DecodeAlphabet, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
};
use crate::bindings::buffer::{BytesLike, ascii_or_bytes, contiguous_bytes_like};
use crate::bindings::{DETACH_THRESHOLD, bytearray_data, bytearray_size, list_items};

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
    let provided = unsafe { bytearray_size(output.as_ptr()) };
    if provided < layout.output_len {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len,
            provided,
        });
    }
    let output =
        unsafe { slice::from_raw_parts_mut(bytearray_data(output.as_ptr()), layout.output_len) };
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
    let provided = unsafe { bytearray_size(output.as_ptr()) };
    if provided < layout.output_len {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len,
            provided,
        });
    }
    let output =
        unsafe { slice::from_raw_parts_mut(bytearray_data(output.as_ptr()), layout.output_len) };
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
    let strict_mode = validate.unwrap_or(ignorechars.is_some());
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
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    decode_parsed(py, &input, None, false, true, None, false)
}

/// Decode standard Base64 into a reusable output.
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
    let mut decoded = batch_results(items.len())?;
    for item in list_items(items) {
        let input = ascii_or_bytes(py, &item, "s")?;
        decoded.push(decode_parsed(
            py, &input, altchars, validate, true, None, false,
        )?);
    }
    PyList::new(py, decoded)
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
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut written = batch_results(items.len())?;
    for (item, output) in list_items(items).into_iter().zip(outputs.iter()) {
        let input = ascii_or_bytes(py, &item, "s")?;
        written.push(decode_parsed_into(
            py, &input, output, altchars, validate, true, None, false,
        )?);
    }
    PyList::new(py, written)
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
    let strict_mode = validate.unwrap_or(ignorechars.is_some());
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

/// Decode with the URL-safe Base64 alphabet.
pub(super) fn urlsafe_b64decode_pre_315<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    urlsafe_b64decode_impl(py, s, padded)
}

/// Decode with the URL-safe Base64 alphabet.
pub(super) fn urlsafe_b64decode_315<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    urlsafe_b64decode_impl(py, s, padded)
}

/// Decode URL-safe Base64 into a reusable output.
pub(super) fn urlsafe_b64decode_into_pre_315(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    urlsafe_b64decode_into_impl(py, s, output, padded)
}

/// Decode URL-safe Base64 into a reusable output.
pub(super) fn urlsafe_b64decode_into_315(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    urlsafe_b64decode_into_impl(py, s, output, padded)
}
