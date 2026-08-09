use core::slice;

use pyo3::exceptions::{PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::{output_ptr, pybytes_with_len};
use crate::base64::{encode_to_slice, encoded_len};
use crate::python::DETACH_THRESHOLD;
use crate::python::buffer::BytesLike;

pub(super) fn encode<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    let output_len = encoded_output_len(input.len(), padded, wrapcol);
    let (output, ()) = pybytes_with_len(py, output_len, |output| unsafe {
        input.with_bytes(|input| {
            let encode = || {
                let urlsafe = altchars == Some(*b"-_");
                encode_configured(input, output, urlsafe, padded, wrapcol);
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

pub(super) fn encode_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<usize> {
    if input.aliases(output) {
        let input = unsafe { input.with_bytes(<[u8]>::to_vec) };
        return encode_slice_into(&input, output, altchars, padded, wrapcol);
    }
    unsafe { input.with_bytes(|input| encode_slice_into(input, output, altchars, padded, wrapcol)) }
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

#[inline]
fn unpadded_encoded_len(input_len: usize) -> usize {
    encoded_len(input_len) - usize::from(!input_len.is_multiple_of(3)) * (3 - input_len % 3)
}

#[inline]
fn encoded_data_len(input_len: usize, padded: bool) -> usize {
    if padded {
        encoded_len(input_len)
    } else {
        unpadded_encoded_len(input_len)
    }
}

#[inline]
fn encoded_output_len(input_len: usize, padded: bool, wrapcol: Option<usize>) -> usize {
    let data_len = encoded_data_len(input_len, padded);
    match (data_len, wrapcol) {
        (0, _) | (_, None) => data_len,
        (_, Some(width)) => data_len + (data_len - 1) / width,
    }
}

fn encode_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<usize> {
    let required = encoded_output_len(input.len(), padded, wrapcol);
    let output = unsafe { slice::from_raw_parts_mut(output_ptr(output, required)?, required) };
    let urlsafe = altchars == Some(*b"-_");
    encode_configured(input, output, urlsafe, padded, wrapcol);
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

#[inline]
fn encode_unwrapped(input: &[u8], output: &mut [u8], urlsafe: bool, padded: bool) {
    if padded {
        encode_to_slice(input, output, urlsafe);
        return;
    }

    let complete_input_len = input.len() / 3 * 3;
    let complete_output_len = complete_input_len / 3 * 4;
    encode_to_slice(
        &input[..complete_input_len],
        &mut output[..complete_output_len],
        urlsafe,
    );
    if complete_input_len != input.len() {
        let tail = &input[complete_input_len..];
        let mut encoded_tail = [0; 4];
        encode_to_slice(tail, &mut encoded_tail, urlsafe);
        output[complete_output_len..]
            .copy_from_slice(&encoded_tail[..unpadded_encoded_len(tail.len())]);
    }
}

fn encode_configured(
    mut input: &[u8],
    mut output: &mut [u8],
    urlsafe: bool,
    padded: bool,
    wrapcol: Option<usize>,
) {
    let Some(width) = wrapcol else {
        encode_unwrapped(input, output, urlsafe, padded);
        return;
    };
    let input_per_line = width / 4 * 3;
    while encoded_data_len(input.len(), padded) > width {
        let (line_input, rest_input) = input.split_at(input_per_line);
        let (line_output, rest_output) = output.split_at_mut(width);
        encode_to_slice(line_input, line_output, urlsafe);
        rest_output[0] = b'\n';
        input = rest_input;
        output = &mut rest_output[1..];
    }
    encode_unwrapped(input, output, urlsafe, padded);
}
