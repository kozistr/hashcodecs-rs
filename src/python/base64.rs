use core::slice;

use pyo3::exceptions::{
    PyAssertionError, PyDeprecationWarning, PyFutureWarning, PyMemoryError, PyValueError,
};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyByteArrayMethods, PyBytes, PyDict, PyType};

use super::DETACH_THRESHOLD;
use super::buffer::{BytesLike, ascii_or_bytes, contiguous_bytes_like};
use crate::base64::{
    Base64Error, DecodeAlphabet, decode_layout, decode_to_slice_with_layout_and_alphabet,
};

mod encode;

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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
        if py.version_info() >= (3, 15) {
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
    Ok(Some(unsafe {
        bytes.with_bytes(|bytes| [bytes[0], bytes[1]])
    }))
}

fn pybytes_with_len<'py, T>(
    py: Python<'py>,
    length: usize,
    init: impl FnOnce(&mut [u8]) -> T,
) -> PyResult<(Bound<'py, PyBytes>, T)> {
    let length = ffi::Py_ssize_t::try_from(length)
        .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
    unsafe {
        let raw = ffi::PyBytes_FromStringAndSize(core::ptr::null(), length);
        let bytes: Bound<'py, PyBytes> =
            Bound::from_owned_ptr_or_err(py, raw)?.cast_into_unchecked();
        let buffer = ffi::PyBytes_AsString(raw).cast::<u8>();
        debug_assert!(!buffer.is_null());

        // The object is never exposed until the initializer has written every byte.
        let initialized = init(slice::from_raw_parts_mut(buffer, length as usize));
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
    let (output, result) = pybytes_with_len(py, layout.output_len, |output| unsafe {
        input.with_bytes(|input| {
            let mut decode =
                || decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet);
            if detach { py.detach(decode) } else { decode() }
        })
    })?;
    result.map_err(|_| decoding_error(py, "Only base64 data is allowed"))?;
    Ok(output)
}

fn decode_strict_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
) -> Result<usize, Base64Error> {
    if input.aliases(output) {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return decode_strict_slice_into(&input, output, alphabet);
    }
    unsafe { input.with_bytes(|input| decode_strict_slice_into(input, output, alphabet)) }
}

fn decode_strict_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
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
    decode_to_slice_with_layout_and_alphabet(
        input,
        &mut output[..layout.output_len],
        layout,
        alphabet,
    )?;
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

fn translate_altchars(input: &[u8], [plus, slash]: [u8; 2]) -> Vec<u8> {
    input
        .iter()
        .map(|&byte| {
            if byte == slash {
                b'/'
            } else if byte == plus {
                b'+'
            } else {
                byte
            }
        })
        .collect()
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
            decode_strict(py, &BytesLike::Owned(translated), DecodeAlphabet::Standard)
        }
    }
}

fn padded_input(input: &BytesLike<'_, '_>) -> Option<BytesLike<'static, 'static>> {
    unsafe {
        input.with_bytes(|input| {
            if input.contains(&b'=') {
                return None;
            }
            let padding = (4 - input.len() % 4) % 4;
            if padding == 3 {
                return None;
            }
            let mut padded = Vec::with_capacity(input.len() + padding);
            padded.extend_from_slice(input);
            padded.resize(input.len() + padding, b'=');
            Some(BytesLike::Owned(padded))
        })
    }
}

fn decode_unpadded_with_altchars<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    let Some(input) = padded_input(input) else {
        return Err(decoding_error(py, "Incorrect padding"));
    };
    decode_strict_with_altchars(py, &input, altchars)
}

fn decode_unpadded_into_with_altchars(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
) -> Result<usize, Base64Error> {
    let Some(input) = padded_input(input) else {
        return Err(Base64Error::InvalidInput);
    };
    let translated = altchars
        .filter(|altchars| *altchars != *b"-_")
        .map(|altchars| unsafe { input.with_bytes(|input| translate_altchars(input, altchars)) });
    let translated = translated.map(BytesLike::Owned);
    let direct_input = translated.as_ref().unwrap_or(&input);
    let alphabet = if altchars == Some(*b"-_") {
        DecodeAlphabet::Mixed
    } else {
        DecodeAlphabet::Standard
    };
    decode_strict_into(direct_input, output, alphabet)
}

fn warn_legacy_altchars(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    ignorechars_specified: bool,
    strict_mode: bool,
) -> PyResult<()> {
    if py.version_info() < (3, 15) || ignorechars_specified {
        return Ok(());
    }
    let Some(altchars) = altchars else {
        return Ok(());
    };
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
    if py.version_info() < (3, 15) && (!padded || ignorechars.is_some() || canonical) {
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
        altchars.map(|altchars| unsafe {
            input.with_bytes(|input| translate_altchars(input, altchars))
        })
    };
    let data = if let Some(translated) = translated.as_deref() {
        PyBytes::new(py, translated)
    } else {
        unsafe { input.with_bytes(|input| PyBytes::new(py, input)) }
    };
    let input = data.as_bytes();
    let decode = py.import("binascii")?.getattr("a2b_base64")?;
    let output = if py.version_info() >= (3, 15) {
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
    } else if py.version_info() < (3, 11) {
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

#[pyfunction(signature = (s, altchars=None, *, padded=true, wrapcol=0))]
pub(super) fn b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    #[pyo3(from_py_with = extract_truthy)] padded: bool,
    wrapcol: i128,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode::encode(
        py,
        &input,
        parse_altchars(py, altchars, false)?,
        padded,
        encode::normalize_wrapcol(wrapcol)?,
    )
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
    encode::encode_into(
        &input,
        output,
        altchars,
        padded,
        encode::normalize_wrapcol(wrapcol)?,
    )
}

#[pyfunction(signature = (s, altchars=None, validate=None, *, padded=true, ignorechars=None, canonical=false))]
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
    warn_legacy_altchars(py, &input, altchars, ignorechars.is_some(), strict_mode)?;
    let ignorechars = ignorechars.as_ref().map(|value| value.bind(py));
    if ignorechars.is_none() && !canonical && strict_mode {
        if !padded {
            return match decode_unpadded_with_altchars(py, &input, altchars) {
                Ok(output) => Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
                Err(_) => decode_with_binascii(py, &input, altchars, true, false, None, false),
            };
        }
        return match decode_strict_with_altchars(py, &input, altchars) {
            Ok(output) => Ok(output),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
            Err(_) => decode_with_binascii(py, &input, altchars, true, true, None, false),
        };
    }

    if ignorechars.is_none() && !canonical && !strict_mode {
        let direct = match altchars {
            None => Some(DecodeAlphabet::Standard),
            Some([b'-', b'_']) => Some(DecodeAlphabet::Mixed),
            Some(_) => None,
        };
        if let Some(alphabet) = direct
            && let Ok(output) = decode_strict(py, &input, alphabet)
        {
            return Ok(output);
        }
        if !padded && let Ok(output) = decode_unpadded_with_altchars(py, &input, altchars) {
            return Ok(output);
        }
    }
    decode_with_binascii(
        py,
        &input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )
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
    warn_legacy_altchars(py, &input, altchars, ignorechars.is_some(), strict_mode)?;
    let ignorechars = ignorechars.as_ref().map(|value| value.bind(py));
    let translated = altchars
        .filter(|altchars| *altchars != *b"-_")
        .map(|altchars| unsafe { input.with_bytes(|input| translate_altchars(input, altchars)) });
    let translated = translated.map(BytesLike::Owned);
    let direct_input = translated.as_ref().unwrap_or(&input);
    let alphabet = if altchars == Some(*b"-_") {
        DecodeAlphabet::Mixed
    } else {
        DecodeAlphabet::Standard
    };

    let direct = if ignorechars.is_none() && !canonical && (padded || !strict_mode) {
        decode_strict_into(direct_input, output, alphabet)
    } else {
        Err(Base64Error::InvalidInput)
    };
    match direct {
        Ok(written) => return Ok(written),
        Err(Base64Error::OutputTooSmall { required, provided }) => {
            return Err(output_too_small(required, provided));
        }
        Err(Base64Error::InvalidInput) => {}
    }

    if !padded && ignorechars.is_none() && !canonical {
        match decode_unpadded_into_with_altchars(&input, output, altchars) {
            Ok(written) => return Ok(written),
            Err(Base64Error::OutputTooSmall { required, provided }) => {
                return Err(output_too_small(required, provided));
            }
            Err(Base64Error::InvalidInput) => {}
        }
    }

    let decoded = decode_with_binascii(
        py,
        &input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )?;
    copy_decoded_into(&decoded, output)
}
