use pyo3::prelude::*;
use pyo3::types::{PyAny, PyInt, PyList};

use super::super::{batch_outputs, parse_altchars};
use super::plan::{DecodeExecution, DecodeOptions, DecodeOutput, DecodePlan};
use crate::bindings::buffer::{BytesLike, ascii_or_bytes};
use crate::bindings::objects::{list_from_fn, list_items};

/// Decode each ASCII string or bytes-like item and return results in input order.
///
/// ``items`` must be a list. ``altchars`` and ``validate`` apply to every item.
/// Processing is fail-fast: an error discards the partial result and is raised
/// immediately. Processing is single-threaded. Immutable items of at least
/// 256 KiB release the GIL independently; smaller and mutable items do not. Do
/// not mutate ``items`` concurrently while this function is running.
pub(in crate::bindings::base64) fn b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    b64decode_batch_parsed(py, items, altchars, validate)
}

fn b64decode_batch_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items);
    let length = items.len();
    let mut items = items.into_iter();
    let options = DecodeOptions::new(altchars, Some(validate), true, None, false);
    list_from_fn(py, length, |_| {
        let item = items.next().expect("batch item count is exact");
        let input = ascii_or_bytes(py, &item, "s")?;
        DecodePlan::new(&input, options)
            .execute(py, DecodeExecution::Allocate)
            .map(DecodeOutput::into_bytes)
    })
}

pub(in crate::bindings::base64) fn standard_b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_parsed(py, items, None, false)
}

pub(in crate::bindings::base64) fn urlsafe_b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_parsed(py, items, Some(*b"-_"), false)
}

/// Decode each item into its matching reusable bytearray and return byte counts.
///
/// ``items`` and ``outputs`` must be equal-length lists, and destinations must
/// be distinct bytearrays. Each destination keeps its size; only its written
/// prefix is changed. Processing is fail-fast and non-transactional: an error
/// leaves earlier destinations modified, and the failing destination may be
/// partly written. The GIL remains held because outputs are mutable. Inputs are
/// snapshotted before the first destination write.
pub(in crate::bindings::base64) fn b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    b64decode_batch_into_parsed(py, items, outputs, altchars, validate)
}

fn b64decode_batch_into_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items);
    let outputs = batch_outputs(items.len(), outputs)?;
    let inputs = items
        .iter()
        .map(|item| ascii_or_bytes(py, item, "s").map(BytesLike::into_stable_for_batch_output))
        .collect::<PyResult<Vec<_>>>()?;
    let length = inputs.len();
    let mut pairs = inputs.into_iter().zip(outputs.iter());
    let options = DecodeOptions::new(altchars, Some(validate), true, None, false);
    list_from_fn(py, length, |_| {
        let (input, output) = pairs.next().expect("batch item count is exact");
        Ok(PyInt::new(
            py,
            DecodePlan::new(&input, options)
                .execute(py, DecodeExecution::Into(output))?
                .into_written(),
        ))
    })
}

pub(in crate::bindings::base64) fn standard_b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_into_parsed(py, items, outputs, None, false)
}

pub(in crate::bindings::base64) fn urlsafe_b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64decode_batch_into_parsed(py, items, outputs, Some(*b"-_"), false)
}
