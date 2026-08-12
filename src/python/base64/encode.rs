use core::slice;

use pyo3::exceptions::{PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::{output_ptr, pybytes_with_len};
use crate::base64::{encode_to_ptr, encoded_len};
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
    let (output, ()) = unsafe {
        pybytes_with_len(py, output_len, |output| {
            input.with_bytes(|input| {
                let output_address = output as usize;
                let encode = move || {
                    let output = output_address as *mut u8;
                    let urlsafe = altchars == Some(*b"-_");
                    encode_configured_ptr(input, output, urlsafe, padded, wrapcol);
                    if let Some(altchars) = altchars.filter(|_| !urlsafe) {
                        // `encode_configured_ptr` initialized the complete allocation.
                        let output = slice::from_raw_parts_mut(output, output_len);
                        substitute_altchars(output, altchars);
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
    let output = output_ptr(output, required)?;
    let urlsafe = altchars == Some(*b"-_");
    unsafe { encode_configured_ptr(input, output, urlsafe, padded, wrapcol) };
    if let Some(altchars) = altchars.filter(|_| !urlsafe) {
        let output = unsafe { slice::from_raw_parts_mut(output, required) };
        substitute_altchars(output, altchars);
    }
    Ok(required)
}

#[inline]
fn substitute_altchars(output: &mut [u8], [plus, slash]: [u8; 2]) {
    let Some(first) = memchr::memchr2(b'+', b'/', output) else {
        return;
    };
    for byte in &mut output[first..] {
        if *byte == b'+' {
            *byte = plus;
        } else if *byte == b'/' {
            *byte = slash;
        }
    }
}

#[inline]
unsafe fn encode_unwrapped_ptr(input: &[u8], output: *mut u8, urlsafe: bool, padded: bool) {
    if padded {
        unsafe { encode_to_ptr(input, output, urlsafe) };
        return;
    }

    let complete_input_len = input.len() / 3 * 3;
    let complete_output_len = complete_input_len / 3 * 4;
    unsafe { encode_to_ptr(&input[..complete_input_len], output, urlsafe) };
    if complete_input_len != input.len() {
        let tail = &input[complete_input_len..];
        let mut encoded_tail = [0; 4];
        unsafe { encode_to_ptr(tail, encoded_tail.as_mut_ptr(), urlsafe) };
        let tail_len = unpadded_encoded_len(tail.len());
        unsafe {
            output
                .add(complete_output_len)
                .copy_from_nonoverlapping(encoded_tail.as_ptr(), tail_len)
        };
    }
}

unsafe fn encode_configured_ptr(
    mut input: &[u8],
    output: *mut u8,
    urlsafe: bool,
    padded: bool,
    wrapcol: Option<usize>,
) {
    let Some(width) = wrapcol else {
        unsafe { encode_unwrapped_ptr(input, output, urlsafe, padded) };
        return;
    };
    let input_per_line = width / 4 * 3;
    let mut destination = 0;
    while encoded_data_len(input.len(), padded) > width {
        let (line_input, rest_input) = input.split_at(input_per_line);
        unsafe { encode_to_ptr(line_input, output.add(destination), urlsafe) };
        destination += width;
        unsafe { output.add(destination).write(b'\n') };
        destination += 1;
        input = rest_input;
    }
    unsafe { encode_unwrapped_ptr(input, output.add(destination), urlsafe, padded) };
}
