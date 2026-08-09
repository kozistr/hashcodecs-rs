use core::slice;

use pyo3::exceptions::{PyAssertionError, PyMemoryError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyByteArrayMethods, PyBytes, PyDict, PyType};

use super::DETACH_THRESHOLD;
use super::buffer::{BytesLike, ascii_or_bytes, contiguous_bytes_like};
use crate::base64::{
    Base64Error, DecodeAlphabet, decode_layout, decode_to_slice_with_layout_and_alphabet,
    encode_to_slice, encoded_len,
};

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

fn encode_with_altchars<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    let (output, ()) = pybytes_with_len(py, encoded_len(input.len()), |output| unsafe {
        input.with_bytes(|input| {
            let encode = || {
                let urlsafe = altchars == Some(*b"-_");
                encode_to_slice(input, output, urlsafe);
                if let Some([plus, slash]) = altchars.filter(|_| !urlsafe) {
                    for byte in output {
                        if *byte == b'+' {
                            *byte = plus;
                        } else if *byte == b'/' {
                            *byte = slash;
                        }
                    }
                }
            };
            if detach {
                py.detach(encode);
            } else {
                encode();
            }
        })
    })?;
    Ok(output)
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

fn encode_into_with_altchars(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
) -> PyResult<usize> {
    if input.aliases(output) {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return encode_slice_into_with_altchars(&input, output, altchars);
    }
    unsafe { input.with_bytes(|input| encode_slice_into_with_altchars(input, output, altchars)) }
}

fn encode_slice_into_with_altchars(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
) -> PyResult<usize> {
    let required = encoded_len(input.len());
    let output = unsafe { slice::from_raw_parts_mut(output_ptr(output, required)?, required) };
    let urlsafe = altchars == Some(*b"-_");
    encode_to_slice(input, output, urlsafe);
    if let Some([plus, slash]) = altchars.filter(|_| !urlsafe) {
        for byte in output {
            if *byte == b'+' {
                *byte = plus;
            } else if *byte == b'/' {
                *byte = slash;
            }
        }
    }
    Ok(required)
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

fn decode_with_binascii<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    strict_mode: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let translated = altchars
        .map(|altchars| unsafe { input.with_bytes(|input| translate_altchars(input, altchars)) });
    let data = if let Some(translated) = translated.as_deref() {
        PyBytes::new(py, translated)
    } else {
        unsafe { input.with_bytes(|input| PyBytes::new(py, input)) }
    };
    let input = data.as_bytes();
    let decode = py.import("binascii")?.getattr("a2b_base64")?;
    let output = if py.version_info() < (3, 11) {
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

#[pyfunction(signature = (s, altchars=None))]
pub(super) fn b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode_with_altchars(py, &input, parse_altchars(py, altchars, false)?)
}

#[pyfunction(signature = (s, output, altchars=None))]
pub(super) fn b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, false)?;
    encode_into_with_altchars(&input, output, altchars)
}

#[pyfunction(signature = (s, altchars=None, validate=false))]
pub(super) fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    if validate {
        return match decode_strict_with_altchars(py, &input, altchars) {
            Ok(output) => Ok(output),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
            Err(_) => decode_with_binascii(py, &input, altchars, true),
        };
    }

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
    decode_with_binascii(py, &input, altchars, false)
}

#[pyfunction(signature = (s, output, altchars=None, validate=false))]
pub(super) fn b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    validate: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
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

    match decode_strict_into(direct_input, output, alphabet) {
        Ok(written) => return Ok(written),
        Err(Base64Error::OutputTooSmall { required, provided }) => {
            return Err(output_too_small(required, provided));
        }
        Err(Base64Error::InvalidInput) => {}
    }

    let decoded = decode_with_binascii(py, &input, altchars, validate)?;
    copy_decoded_into(&decoded, output)
}
