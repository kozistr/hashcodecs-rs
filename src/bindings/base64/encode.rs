use core::slice;

use pyo3::exceptions::{PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::decode::translate_bytes;
use super::{pybytes_with_len, with_output_ptr};
use crate::base64::{encode_to_ptr, encode_to_ptr_cached, encoded_len};
use crate::bindings::buffer::BytesLike;
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

mod batch;

pub(super) use self::batch::{
    b64encode_batch, b64encode_batch_into, b64encode_batch_into_parsed, b64encode_batch_parsed,
};

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn encode_exact<'py>(
    py: Python<'py>,
    input: &[u8],
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let output_len = encoded_output_len(input.len(), padded, wrapcol);
    let (output, ()) = unsafe {
        pybytes_with_len(py, output_len, |output| {
            let urlsafe = altchars == Some(*b"-_");
            encode_configured_ptr(input, output, urlsafe, padded, wrapcol);
            if let Some(altchars) = altchars.filter(|_| !urlsafe) {
                let output = slice::from_raw_parts_mut(output, output_len);
                substitute_altchars(output, altchars);
            }
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
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return encode(py, &BytesLike::OwnedVec(input), altchars, padded, wrapcol);
    }
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
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
    if let Some(input) = input.snapshot_for_output(output)? {
        return encode_slice_into(&input, output, altchars, padded, wrapcol);
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            encode_slice_to_ptr(input, output, provided, altchars, padded, wrapcol)
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
    with_output_ptr(output, required, |output| {
        let urlsafe = altchars == Some(*b"-_");
        unsafe { encode_configured_ptr(input, output, urlsafe, padded, wrapcol) };
        if let Some(altchars) = altchars.filter(|_| !urlsafe) {
            let output = unsafe { slice::from_raw_parts_mut(output, required) };
            substitute_altchars(output, altchars);
        }
    })?;
    Ok(required)
}

fn encode_slice_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<usize> {
    let required = encoded_output_len(input.len(), padded, wrapcol);
    if provided < required {
        return Err(super::output_too_small(required, provided));
    }
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
    translate_bytes(output, b'+', plus, b'/', slash);
}

#[inline]
unsafe fn encode_unwrapped_ptr<const CACHED: bool>(
    input: &[u8],
    output: *mut u8,
    urlsafe: bool,
    padded: bool,
) {
    if padded {
        unsafe { encode_ptr::<CACHED>(input, output, urlsafe) };
        return;
    }

    let complete_input_len = input.len() / 3 * 3;
    let complete_output_len = complete_input_len / 3 * 4;
    unsafe { encode_ptr::<CACHED>(&input[..complete_input_len], output, urlsafe) };
    if complete_input_len != input.len() {
        let tail = &input[complete_input_len..];
        let mut encoded_tail = [0; 4];
        unsafe { encode_ptr::<CACHED>(tail, encoded_tail.as_mut_ptr(), urlsafe) };
        let tail_len = unpadded_encoded_len(tail.len());
        unsafe {
            output
                .add(complete_output_len)
                .copy_from_nonoverlapping(encoded_tail.as_ptr(), tail_len)
        };
    }
}

#[inline]
unsafe fn encode_ptr<const CACHED: bool>(input: &[u8], output: *mut u8, urlsafe: bool) {
    if CACHED {
        unsafe { encode_to_ptr_cached(input, output, urlsafe) };
    } else {
        unsafe { encode_to_ptr(input, output, urlsafe) };
    }
}

unsafe fn encode_configured_ptr(
    input: &[u8],
    output: *mut u8,
    urlsafe: bool,
    padded: bool,
    wrapcol: Option<usize>,
) {
    let Some(width) = wrapcol else {
        unsafe { encode_unwrapped_ptr::<false>(input, output, urlsafe, padded) };
        return;
    };
    let data_len = encoded_data_len(input.len(), padded);
    unsafe { encode_unwrapped_ptr::<true>(input, output, urlsafe, padded) };
    unsafe { wrap_encoded_ptr(output, data_len, width) };
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
