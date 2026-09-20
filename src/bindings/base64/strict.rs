use core::slice;

use pyo3::exceptions::PyMemoryError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::policy::ErrorWrites;
use super::staging::pybytes_with_len;
use crate::base64::{
    Base64Error, DecodeAlphabet, decode_layout, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_validated_blocks,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks, decode_unpadded_layout,
};
use crate::bindings::buffer::BytesLike;
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
    let detach_safe = input.detach_safe();
    unsafe {
        input.with_bytes(|input| {
            let layout = match decode_layout(input) {
                Ok(layout) => layout,
                Err(Base64Error::InvalidInput | Base64Error::OutputTooSmall { .. }) => {
                    return Ok(Err(Base64Error::InvalidInput));
                }
            };
            let detach = detach_safe && input.len() >= BASE64_DETACH_THRESHOLD;
            // bytes allocation is not GC-tracked and cannot invoke Python
            // finalizers. Keep one input borrow through layout and decoding.
            let (output, result) = pybytes_with_len(py, layout.output_len(), |output| {
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
            })?;
            Ok(match result {
                Ok(()) => Ok(output),
                Err(Base64Error::InvalidInput | Base64Error::OutputTooSmall { .. }) => {
                    Err(Base64Error::InvalidInput)
                }
            })
        })
    }
}

/// Decode an input stabilized by `PreparedDecoder::decode_into`.
///
/// # Safety
/// The input must not overlap the output. Mutable inputs on free-threaded
/// builds must have been snapshotted before entering the decoder.
pub(super) unsafe fn decode_strict_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> Result<usize, Base64Error> {
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            let layout = decode_layout(input)?;
            if provided < layout.output_len() {
                return Err(Base64Error::OutputTooSmall {
                    required: layout.output_len(),
                    provided,
                });
            }
            let output = slice::from_raw_parts_mut(output, layout.output_len());
            if error_writes.validated_prefix_only() {
                decode_to_slice_with_layout_and_alphabet_validated_blocks(
                    input, output, layout, alphabet,
                )?;
            } else {
                decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet)?;
            }
            Ok(layout.output_len())
        })
    }
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

/// Decode an unpadded input stabilized by `PreparedDecoder::decode_into`.
///
/// # Safety
/// The input must not overlap the output. Mutable inputs on free-threaded
/// builds must have been snapshotted before entering the decoder.
pub(super) unsafe fn decode_unpadded_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    alphabet: DecodeAlphabet,
    error_writes: ErrorWrites,
) -> Result<usize, Base64Error> {
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            if input.contains(&b'=') {
                return Err(Base64Error::InvalidInput);
            }
            let layout = decode_unpadded_layout(input)?;
            if provided < layout.output_len() {
                return Err(Base64Error::OutputTooSmall {
                    required: layout.output_len(),
                    provided,
                });
            }
            let output = slice::from_raw_parts_mut(output, layout.output_len());
            if error_writes.validated_prefix_only() {
                decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks(
                    input, output, layout, alphabet,
                )?;
            } else {
                decode_to_slice_with_unpadded_layout_and_alphabet(input, output, layout, alphabet)?;
            }
            Ok(layout.output_len())
        })
    }
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
