use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::super::output_too_small;
use super::super::output::BytesWriter;
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

#[cfg(target_arch = "aarch64")]
mod aarch64;
mod scalar;
mod state_machine;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

#[cfg(test)]
pub(super) use scalar::translate as translate_bytes_scalar;
pub(super) use scalar::{alphanumeric_prefix as alphanumeric_prefix_scalar, is_lenient_symbol};
pub(in crate::bindings::base64::decode) use state_machine::lenient_continues_after_padding;
#[cfg(test)]
pub(super) use state_machine::version_continues_after_padding;
pub(super) use state_machine::{
    LenientDecodeError, decode_lenient_to_ptr, decoded_symbol_len, lenient_decode_table,
    lenient_decoded_len,
};

pub(super) fn lenient_symbol_count(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if input.len() >= 32 && std::is_x86_feature_detected!("avx2") {
            return unsafe { x86::symbol_count_avx2(input, altchars) };
        }
        if input.len() >= 16 && std::is_x86_feature_detected!("sse2") {
            return unsafe { x86::symbol_count_sse2(input, altchars) };
        }
    }

    #[cfg(target_arch = "aarch64")]
    {
        if input.len() >= 16 {
            return unsafe { aarch64::symbol_count(input, altchars) };
        }
    }

    scalar::symbol_count(input, altchars)
}

pub(super) type AlphanumericPrefix = unsafe fn(&[u8]) -> usize;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_alphanumeric_prefix_for_x86(avx2: bool, sse2: bool) -> AlphanumericPrefix {
    if avx2 {
        return x86::alphanumeric_prefix_avx2;
    }
    if sse2 {
        return x86::alphanumeric_prefix_sse2;
    }
    alphanumeric_prefix_scalar
}

pub(super) fn select_alphanumeric_prefix() -> AlphanumericPrefix {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_alphanumeric_prefix_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    alphanumeric_prefix_scalar
}

pub(super) type TranslateBytes = unsafe fn(&mut [u8], u8, u8, u8, u8);

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn select_translate_bytes_for_x86(avx2: bool, sse2: bool) -> TranslateBytes {
    if avx2 {
        return x86::translate_avx2;
    }
    if sse2 {
        return x86::translate_sse2;
    }
    scalar::translate
}

pub(super) fn select_translate_bytes() -> TranslateBytes {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return select_translate_bytes_for_x86(
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("sse2"),
    );

    #[cfg(target_arch = "aarch64")]
    return aarch64::translate;

    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    scalar::translate
}

pub(in crate::bindings::base64::decode) fn try_decode_lenient<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    padded: bool,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return try_decode_lenient(py, &BytesLike::OwnedVec(input), altchars, padded);
    }
    let writer = BytesWriter::new(py, input.len())?;
    let output_address = unsafe { writer.data() } as usize;
    let table = lenient_decode_table(altchars);
    let continue_after_padding = lenient_continues_after_padding(py);
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let result = unsafe {
        input.with_bytes(|input| {
            let decode = move || {
                decode_lenient_to_ptr::<true>(
                    input,
                    output_address as *mut u8,
                    input.len().div_ceil(4) * 3,
                    &table,
                    padded,
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
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
) -> PyResult<Option<usize>> {
    let table = lenient_decode_table(altchars);
    let continue_after_padding = lenient_continues_after_padding(py);
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_lenient_slice_into(
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                &table,
                altchars,
                padded,
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
                &table,
                altchars,
                padded,
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
            padded,
            continue_after_padding,
        )
    }
    .ok())
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;

    #[test]
    fn x86_backend_selectors_cover_each_dispatch_tier() {
        for (avx2, sse2, expected) in [
            (
                true,
                true,
                x86::alphanumeric_prefix_avx2 as AlphanumericPrefix,
            ),
            (
                false,
                true,
                x86::alphanumeric_prefix_sse2 as AlphanumericPrefix,
            ),
            (
                false,
                false,
                alphanumeric_prefix_scalar as AlphanumericPrefix,
            ),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_alphanumeric_prefix_for_x86(avx2, sse2),
                expected,
            ));
        }

        for (avx2, sse2, expected) in [
            (true, true, x86::translate_avx2 as TranslateBytes),
            (false, true, x86::translate_sse2 as TranslateBytes),
            (false, false, scalar::translate as TranslateBytes),
        ] {
            assert!(std::ptr::fn_addr_eq(
                select_translate_bytes_for_x86(avx2, sse2),
                expected,
            ));
        }
    }
}
