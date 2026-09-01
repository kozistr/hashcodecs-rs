use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use crate::base64::{Base64Error, decode_layout, decode_unpadded_layout};
use crate::bindings::base64::decode::fallback::decoding_error;
use crate::bindings::base64::decode::output::BytesWriter;
use crate::bindings::base64::decode::plan::DecodeOptions;
use crate::bindings::base64::output_too_small;
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

mod config;
mod scanner;
mod specials;
mod staging;

use config::AdvancedDecoder;
#[cfg(test)]
use config::Translation;
#[cfg(test)]
use specials::StrictSpecials;
#[cfg(test)]
use staging::{ADVANCED_STAGING_CAPACITY, StagingValidator, StagingWriter};

fn translated_strict_decoded_len(
    input: &[u8],
    altchars: [u8; 2],
    padded: bool,
) -> Result<usize, Base64Error> {
    if padded {
        if altchars.contains(&b'=') {
            return (input.len() & 3 == 0)
                .then(|| input.len() / 4 * 3)
                .ok_or(Base64Error::InvalidInput);
        }
        return decode_layout(input).map(|layout| layout.output_len());
    }
    if !altchars.contains(&b'=') && input.contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    decode_unpadded_layout(input).map(|layout| layout.output_len())
}

pub(super) fn decode_advanced_strict_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: [u8; 2],
    padded: bool,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return decode_advanced_strict_into(
            py,
            &BytesLike::OwnedVec(input),
            output,
            altchars,
            padded,
            transactional_errors,
        );
    }
    let decoder = AdvancedDecoder::new(
        py,
        DecodeOptions::new(Some(altchars), Some(true), padded, None, false),
    )?;
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            if transactional_errors {
                let Some(required) = decoder.decoded_len(input, false) else {
                    return Err(Base64Error::InvalidInput);
                };
                if provided < required {
                    return Err(Base64Error::OutputTooSmall { required, provided });
                }
                let written = decoder.decode_validated_to_ptr(input, output, false);
                debug_assert_eq!(written, required);
                return Ok(written);
            }

            let required = translated_strict_decoded_len(input, altchars, padded)?;
            if provided < required {
                return Err(Base64Error::OutputTooSmall { required, provided });
            }
            decoder
                .decode_checked_to_ptr(input, output, false)
                .ok_or(Base64Error::InvalidInput)
        })
    })
}

pub(in crate::bindings::base64::decode) fn decode_advanced<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_advanced(py, &BytesLike::OwnedVec(input), options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = super::lenient::compat::continues_after_padding(py);
    let writer = BytesWriter::new(py, input.len())?;
    let output_address = unsafe { writer.data() } as usize;
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let result = unsafe {
        input.with_bytes(|input| {
            let decode = move || {
                decoder.decode_checked_to_ptr(
                    input,
                    output_address as *mut u8,
                    continue_after_padding,
                )
            };
            if detach { py.detach(decode) } else { decode() }
        })
    };
    let Some(written) = result else {
        return Err(decoding_error(py, "Incorrect padding"));
    };
    unsafe { writer.finish(py, written) }
}

unsafe fn decode_advanced_slice_into(
    py: Python<'_>,
    input: &[u8],
    output: *mut u8,
    provided: usize,
    decoder: &AdvancedDecoder,
    continue_after_padding: bool,
) -> PyResult<usize> {
    let Some(required) = decoder.decoded_len(input, continue_after_padding) else {
        return Err(decoding_error(py, "Incorrect padding"));
    };
    if provided < required {
        return Err(output_too_small(required, provided));
    }
    let written = unsafe { decoder.decode_validated_to_ptr(input, output, continue_after_padding) };
    debug_assert_eq!(written, required);
    Ok(written)
}

pub(in crate::bindings::base64::decode) fn decode_advanced_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<usize> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_advanced_into(py, &BytesLike::OwnedVec(input), output, options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = super::lenient::compat::continues_after_padding(py);
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_advanced_slice_into(
                py,
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                &decoder,
                continue_after_padding,
            )
        });
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_advanced_slice_into(
                py,
                input,
                output,
                provided,
                &decoder,
                continue_after_padding,
            )
        })
    }
}

#[cfg(test)]
#[path = "advanced_tests.rs"]
mod tests;
