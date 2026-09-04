use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::output_too_small;
use super::output::BytesWriter;
use super::policy::Padding;
use crate::bindings::base64::PythonSemantics;
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

mod state_machine;
pub(super) mod symbols;
#[cfg(target_arch = "aarch64")]
mod symbols_aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(in crate::bindings::base64::decode) mod symbols_x86;

pub(super) use state_machine::{
    LenientDecodeError, decode_lenient_to_ptr, decoded_symbol_len, lenient_decode_table,
    lenient_decoded_len,
};

pub(in crate::bindings::base64::decode) fn try_decode_lenient<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    padding: Padding,
    table: &[u8; 256],
    semantics: PythonSemantics,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return try_decode_lenient(
            py,
            &BytesLike::OwnedVec(input),
            altchars,
            padding,
            table,
            semantics,
        );
    }
    let writer = BytesWriter::new(py, input.len())?;
    let output_address = unsafe { writer.data() } as usize;
    let continue_after_padding = semantics.continues_after_padding;
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let result = unsafe {
        input.with_bytes(|input| {
            let decode = move || {
                decode_lenient_to_ptr::<true>(
                    input,
                    output_address as *mut u8,
                    input.len().div_ceil(4) * 3,
                    table,
                    altchars,
                    padding.is_padded(),
                    continue_after_padding,
                )
            };
            if detach { py.detach(decode) } else { decode() }
        })
    };
    match result {
        Ok(written) => unsafe { writer.finish(py, written).map(Some) },
        Err(LenientDecodeError::InvalidInput | LenientDecodeError::OutputTooSmall) => Ok(None),
    }
}

pub(in crate::bindings::base64::decode) fn try_decode_lenient_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padding: Padding,
    table: &[u8; 256],
    semantics: PythonSemantics,
) -> PyResult<Option<usize>> {
    let continue_after_padding = semantics.continues_after_padding;
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_lenient_slice_into(
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                table,
                altchars,
                padding.is_padded(),
                continue_after_padding,
            )
        });
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_lenient_slice_into(
                input,
                output,
                provided,
                table,
                altchars,
                padding.is_padded(),
                continue_after_padding,
            )
        })
    }
}

/// Decode lenient input without partially writing an undersized destination.
///
/// # Safety
///
/// `output` must be valid for writes of `provided` bytes and must not overlap
/// `input`.
unsafe fn decode_lenient_slice_into(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    table: &[u8; 256],
    altchars: Option<[u8; 2]>,
    padded: bool,
    continue_after_padding: bool,
) -> PyResult<Option<usize>> {
    let maximum = input.len().div_ceil(4) * 3;
    if provided < maximum {
        let required = lenient_decoded_len(input, altchars, padded, continue_after_padding);
        match required {
            Ok(required) if provided < required => {
                return Err(output_too_small(required, provided));
            }
            Ok(_) => {}
            Err(LenientDecodeError::InvalidInput | LenientDecodeError::OutputTooSmall) => {
                return Ok(None);
            }
        }
    }
    Ok(unsafe {
        decode_lenient_to_ptr::<true>(
            input,
            output,
            provided,
            table,
            altchars,
            padded,
            continue_after_padding,
        )
    }
    .ok())
}
