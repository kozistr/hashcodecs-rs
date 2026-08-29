use core::slice;

use pyo3::exceptions::PyMemoryError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::super::{output_too_small, pybytes_with_len};
use super::super::fallback::decoding_error;
use super::super::plan::DecodeOptions;
use super::{decode_advanced, decode_advanced_strict_into};
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

pub(in crate::bindings::base64::decode) fn try_decode_strict<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    Ok(decode_strict_native(py, input, alphabet)?.ok())
}

pub(in crate::bindings::base64::decode) fn decode_strict<'py>(
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

pub(in crate::bindings::base64::decode) fn decode_strict_into(
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

pub(in crate::bindings::base64::decode) fn translate_altchars(
    input: &[u8],
    [plus, slash]: [u8; 2],
) -> PyResult<Option<Vec<u8>>> {
    let Some(first) = memchr::memchr2(plus, slash, input) else {
        return Ok(None);
    };
    let mut translated = Vec::new();
    translated
        .try_reserve_exact(input.len())
        .map_err(|_| PyMemoryError::new_err("Base64 input is too large"))?;
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
    Ok(Some(translated))
}

pub(in crate::bindings::base64::decode) fn normalize_mime_whitespace(
    input: &BytesLike<'_, '_>,
) -> PyResult<Option<Vec<u8>>> {
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

pub(in crate::bindings::base64::decode) fn decode_strict_with_altchars<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    match altchars {
        None => decode_strict(py, input, DecodeAlphabet::Standard),
        Some([b'-', b'_']) => decode_strict(py, input, DecodeAlphabet::Mixed),
        Some(altchars) => decode_advanced(
            py,
            input,
            DecodeOptions::new(Some(altchars), Some(true), true, None, false),
        ),
    }
}

pub(in crate::bindings::base64::decode) fn decode_unpadded_with_altchars<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    match altchars {
        None => decode_unpadded(py, input, DecodeAlphabet::Standard),
        Some([b'-', b'_']) => decode_unpadded(py, input, DecodeAlphabet::Mixed),
        Some(altchars) => decode_advanced(
            py,
            input,
            DecodeOptions::new(Some(altchars), Some(true), false, None, false),
        ),
    }
}

pub(in crate::bindings::base64::decode) fn decode_strict_into_with_altchars(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    match altchars {
        None => decode_strict_into(
            input,
            output,
            DecodeAlphabet::Standard,
            transactional_errors,
        ),
        Some([b'-', b'_']) => {
            decode_strict_into(input, output, DecodeAlphabet::Mixed, transactional_errors)
        }
        Some(altchars) => {
            decode_advanced_strict_into(py, input, output, altchars, true, transactional_errors)
        }
    }
}

pub(in crate::bindings::base64::decode) fn decode_unpadded_into_with_altchars(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    match altchars {
        None => decode_unpadded_into(
            input,
            output,
            DecodeAlphabet::Standard,
            transactional_errors,
        ),
        Some([b'-', b'_']) => {
            decode_unpadded_into(input, output, DecodeAlphabet::Mixed, transactional_errors)
        }
        Some(altchars) => {
            decode_advanced_strict_into(py, input, output, altchars, false, transactional_errors)
        }
    }
}

pub(in crate::bindings::base64::decode) fn try_decode_urlsafe_315<'py>(
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

pub(in crate::bindings::base64::decode) fn try_decode_urlsafe_315_into(
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
