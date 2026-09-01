use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes};

use super::parse_altchars;
use crate::bindings::buffer::ascii_or_bytes;

use self::plan::{DecodeOptions, DecodePlan};

mod batch;
mod fallback;
pub(super) mod native;
mod output;
mod plan;

pub(super) use self::batch::{
    b64decode_batch, b64decode_batch_into, standard_b64decode_batch, standard_b64decode_batch_into,
    urlsafe_b64decode_batch, urlsafe_b64decode_batch_into,
};

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
    let options = DecodeOptions::new(altchars, validate, padded, ignorechars, canonical);
    DecodePlan::new(py, &input, options).execute_allocating(py)
}

/// Decode with the standard Base64 alphabet.
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(py, &input, DecodeOptions::standard()).execute_allocating(py)
}

/// Decode standard Base64 into a reusable output.
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(py, &input, DecodeOptions::standard()).execute_into(py, output)
}

pub(super) fn urlsafe_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(py, &input, DecodeOptions::urlsafe(padded)).execute_allocating(py)
}

pub(super) fn urlsafe_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(py, &input, DecodeOptions::urlsafe(padded)).execute_into(py, output)
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
    let options = DecodeOptions::new(altchars, validate, padded, ignorechars, canonical);
    DecodePlan::new(py, &input, options).execute_into(py, output)
}
