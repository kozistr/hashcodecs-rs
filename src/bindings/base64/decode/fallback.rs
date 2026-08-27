use pyo3::exceptions::{PyDeprecationWarning, PyFutureWarning};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyType};

use super::super::{STANDARD_ALPHABET, python_at_least};
use super::native::translate_altchars;
use crate::bindings::buffer::{BytesLike, contiguous_bytes_like};

pub(super) fn decoding_error(py: Python<'_>, message: &'static str) -> PyErr {
    match py
        .import("binascii")
        .and_then(|module| module.getattr("Error"))
        .and_then(|value| value.cast_into::<PyType>().map_err(Into::into))
    {
        Ok(error_type) => PyErr::from_type(error_type, (message,)),
        Err(error) => error,
    }
}

#[inline]
pub(super) fn warn_legacy_altchars(
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

pub(super) fn decode_with_binascii<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
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
    } else if let Some(bytes) = input.python_bytes() {
        bytes.clone()
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

pub(super) fn canonical_padding(input: &[u8]) -> bool {
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
