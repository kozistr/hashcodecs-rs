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

#[derive(Clone, Copy)]
enum EncodeAlphabet {
    Standard,
    UrlSafe,
    Custom([u8; 2]),
}

impl EncodeAlphabet {
    fn new(altchars: Option<[u8; 2]>) -> Self {
        match altchars {
            None => Self::Standard,
            Some(altchars) if altchars == *b"-_" => Self::UrlSafe,
            Some(altchars) => Self::Custom(altchars),
        }
    }

    fn is_urlsafe(self) -> bool {
        matches!(self, Self::UrlSafe)
    }
}

#[derive(Clone, Copy)]
enum EncodePadding {
    Padded,
    Unpadded,
}

impl EncodePadding {
    fn new(padded: bool) -> Self {
        if padded { Self::Padded } else { Self::Unpadded }
    }
}

#[derive(Clone, Copy)]
enum LineWrapping {
    None,
    Columns(usize),
}

impl LineWrapping {
    fn new(wrapcol: Option<usize>) -> Self {
        wrapcol.map_or(Self::None, Self::Columns)
    }
}

#[derive(Clone, Copy)]
pub(super) struct PreparedEncoder {
    alphabet: EncodeAlphabet,
    padding: EncodePadding,
    wrapping: LineWrapping,
}

impl PreparedEncoder {
    pub(super) fn new(altchars: Option<[u8; 2]>, padded: bool, wrapcol: Option<usize>) -> Self {
        Self {
            alphabet: EncodeAlphabet::new(altchars),
            padding: EncodePadding::new(padded),
            wrapping: LineWrapping::new(wrapcol),
        }
    }

    fn data_len(self, input_len: usize) -> usize {
        match self.padding {
            EncodePadding::Padded => encoded_len(input_len),
            EncodePadding::Unpadded => unpadded_encoded_len(input_len),
        }
    }

    fn output_len(self, input_len: usize) -> usize {
        let data_len = self.data_len(input_len);
        match (data_len, self.wrapping) {
            (0, _) | (_, LineWrapping::None) => data_len,
            (_, LineWrapping::Columns(width)) => data_len + (data_len - 1) / width,
        }
    }

    unsafe fn encode_to_ptr(self, input: &[u8], output: *mut u8) {
        match self.wrapping {
            LineWrapping::None => {
                if matches!(self.alphabet, EncodeAlphabet::Custom(_)) {
                    // The substitution pass immediately rereads the encoded
                    // bytes, so they must remain cache-resident.
                    unsafe {
                        encode_unwrapped_ptr::<true>(
                            input,
                            output,
                            self.alphabet.is_urlsafe(),
                            self.padding,
                        )
                    };
                } else {
                    unsafe {
                        encode_unwrapped_ptr::<false>(
                            input,
                            output,
                            self.alphabet.is_urlsafe(),
                            self.padding,
                        )
                    };
                }
            }
            LineWrapping::Columns(width) => unsafe {
                encode_unwrapped_ptr::<true>(
                    input,
                    output,
                    self.alphabet.is_urlsafe(),
                    self.padding,
                );
                wrap_encoded_ptr(output, self.data_len(input.len()), width);
            },
        }
        if let EncodeAlphabet::Custom(altchars) = self.alphabet {
            let output = unsafe { slice::from_raw_parts_mut(output, self.output_len(input.len())) };
            substitute_altchars(output, altchars);
        }
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn encode_exact<'py>(
    py: Python<'py>,
    input: &[u8],
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let encoder = PreparedEncoder::new(altchars, padded, wrapcol);
    let output_len = encoder.output_len(input.len());
    let (output, ()) = unsafe {
        pybytes_with_len(py, output_len, |output| {
            encoder.encode_to_ptr(input, output);
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
    let encoder = PreparedEncoder::new(altchars, padded, wrapcol);
    let output_len = encoder.output_len(input.len());
    let (output, ()) = unsafe {
        pybytes_with_len(py, output_len, |output| {
            input.with_bytes(|input| {
                let output_address = output as usize;
                let encode = move || {
                    let output = output_address as *mut u8;
                    encoder.encode_to_ptr(input, output);
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
    let encoder = PreparedEncoder::new(altchars, padded, wrapcol);
    if let Some(input) = input.snapshot_for_output(output)? {
        return encode_slice_into(&input, output, encoder);
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            encode_slice_to_ptr(input, output, provided, encoder)
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

fn encode_slice_into(
    input: &[u8],
    output: &Bound<'_, PyByteArray>,
    encoder: PreparedEncoder,
) -> PyResult<usize> {
    let required = encoder.output_len(input.len());
    with_output_ptr(output, required, |output| {
        unsafe { encoder.encode_to_ptr(input, output) };
    })?;
    Ok(required)
}

fn encode_slice_to_ptr(
    input: &[u8],
    output: *mut u8,
    provided: usize,
    encoder: PreparedEncoder,
) -> PyResult<usize> {
    let required = encoder.output_len(input.len());
    if provided < required {
        return Err(super::output_too_small(required, provided));
    }
    unsafe { encoder.encode_to_ptr(input, output) };
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
    padding: EncodePadding,
) {
    if matches!(padding, EncodePadding::Padded) {
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
