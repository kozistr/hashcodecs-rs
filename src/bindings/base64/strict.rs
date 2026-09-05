use core::slice;

use pyo3::exceptions::PyMemoryError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::policy::ErrorWrites;
use super::staging::pybytes_with_len;
use crate::base64::{
    Base64Error, DecodeAlphabet, DecodeLayout, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_validated_blocks,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks, decode_unpadded_layout,
};
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

pub(super) fn decode_strict<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Result<Bound<'py, PyBytes>, Base64Error>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_strict(py, &BytesLike::OwnedVec(input), alphabet);
    }
    let layout = match unsafe { input.with_bytes(decode_layout) } {
        Ok(layout) => layout,
        Err(Base64Error::InvalidInput | Base64Error::OutputTooSmall { .. }) => {
            return Ok(Err(Base64Error::InvalidInput));
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
            Err(Base64Error::InvalidInput)
        }
    })
}

pub(super) fn decode_strict_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return Ok(decode_strict_slice_into(
            &input,
            output,
            alphabet,
            error_writes,
        ));
    }
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_strict_to_ptr(input, output, provided, alphabet, error_writes)
        })
    })
}

fn decode_strict_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
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
            error_writes,
        )
    })
}

fn decode_strict_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> Result<usize, Base64Error> {
    let layout = decode_layout(input)?;
    decode_strict_with_layout_to_ptr(input, output, provided, layout, alphabet, error_writes)
}

fn decode_strict_with_layout_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> Result<usize, Base64Error> {
    if provided < layout.output_len() {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len(),
            provided,
        });
    }
    let output = unsafe { slice::from_raw_parts_mut(output, layout.output_len()) };
    if error_writes.validated_prefix_only() {
        decode_to_slice_with_layout_and_alphabet_validated_blocks(input, output, layout, alphabet)?;
    } else {
        decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet)?;
    }
    Ok(layout.output_len())
}

pub(super) fn decode_unpadded<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    alphabet: DecodeAlphabet,
) -> PyResult<Result<Bound<'py, PyBytes>, Base64Error>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_unpadded(py, &BytesLike::OwnedVec(input), alphabet);
    }
    let layout = match unsafe { input.with_bytes(decode_unpadded_layout) } {
        Ok(layout) => layout,
        Err(error) => return Ok(Err(error)),
    };
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
    Ok(result.map(|()| output))
}

pub(super) fn decode_unpadded_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return Ok(decode_unpadded_slice_into(
            &input,
            output,
            alphabet,
            error_writes,
        ));
    }
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_unpadded_to_ptr(input, output, provided, alphabet, error_writes)
        })
    })
}

fn decode_unpadded_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
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
            error_writes,
        )
    })
}

fn decode_unpadded_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> Result<usize, Base64Error> {
    if input.contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    let layout = decode_unpadded_layout(input)?;
    decode_unpadded_with_layout_to_ptr(input, output, provided, layout, alphabet, error_writes)
}

fn decode_unpadded_with_layout_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> Result<usize, Base64Error> {
    if provided < layout.output_len() {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len(),
            provided,
        });
    }
    let output = unsafe { slice::from_raw_parts_mut(output, layout.output_len()) };
    if error_writes.validated_prefix_only() {
        decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks(
            input, output, layout, alphabet,
        )?;
    } else {
        decode_to_slice_with_unpadded_layout_and_alphabet(input, output, layout, alphabet)?;
    }
    Ok(layout.output_len())
}

pub(super) fn translate_altchars(
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
