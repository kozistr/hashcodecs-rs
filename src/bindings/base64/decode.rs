use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes};

use super::parse_altchars;
use crate::bindings::buffer::ascii_or_bytes;

use self::policy::{DecodePolicy, PreparedDecoder};

mod advanced;
mod batch;
mod fallback;
mod lenient;
mod output;
mod policy;
mod strict;

use advanced::{decode_advanced, decode_advanced_into, decode_advanced_strict_into};
use lenient::{try_decode_lenient, try_decode_lenient_into};
use strict::{
    decode_strict, decode_strict_into, decode_strict_into_with_altchars,
    decode_strict_with_altchars, decode_unpadded_into_with_altchars, decode_unpadded_with_altchars,
    translate_altchars, try_decode_strict, try_decode_urlsafe_315, try_decode_urlsafe_315_into,
};

pub(super) use self::batch::{
    b64decode_batch, b64decode_batch_into, standard_b64decode_batch, standard_b64decode_batch_into,
    urlsafe_b64decode_batch, urlsafe_b64decode_batch_into,
};

pub(in crate::bindings::base64) fn translate_bytes(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    let translate = lenient::symbols::select_translate_bytes();
    unsafe { translate(input, source0, target0, source1, target1) };
}

pub(super) fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: Option<bool>,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let policy = DecodePolicy::new(altchars, validate, padded, ignorechars, canonical);
    PreparedDecoder::new(py, policy)?.decode_allocating(py, &input)
}

/// Decode with the standard Base64 alphabet.
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    PreparedDecoder::new(py, DecodePolicy::standard())?.decode_allocating(py, &input)
}

/// Decode standard Base64 into a reusable output.
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    PreparedDecoder::new(py, DecodePolicy::standard())?.decode_into(py, &input, output)
}

pub(super) fn urlsafe_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    PreparedDecoder::new(py, DecodePolicy::urlsafe(padded))?.decode_allocating(py, &input)
}

pub(super) fn urlsafe_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    PreparedDecoder::new(py, DecodePolicy::urlsafe(padded))?.decode_into(py, &input, output)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    validate: Option<bool>,
    padded: bool,
    ignorechars: Option<&Bound<'_, PyAny>>,
    canonical: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let policy = DecodePolicy::new(altchars, validate, padded, ignorechars, canonical);
    PreparedDecoder::new(py, policy)?.decode_into(py, &input, output)
}
