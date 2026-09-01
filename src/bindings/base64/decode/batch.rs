use pyo3::prelude::*;
use pyo3::types::{PyAny, PyInt, PyList};

use super::super::{BatchInputKind, batch_outputs, parse_altchars, prepare_batch_inputs};
use super::plan::{DecodeOptions, DecodePlan};
use crate::bindings::buffer::ascii_or_bytes;
use crate::bindings::objects::{list_from_fn, list_items};

/// Decode each ASCII string or bytes-like item and return the results in input order.
///
/// ``items`` must be a list. ``altchars`` and ``validate`` apply to every item.
/// The function stops at the first error and discards the partial result list.
/// The function uses one thread. It releases the GIL for each immutable item of at least 256 KiB.
/// It retains the GIL for smaller or mutable items. Do not change ``items`` during the call.
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
    let items = list_items(items)?;
    let length = items.len();
    let mut items = items.into_iter();
    let options = DecodeOptions::new(altchars, Some(validate), true, None, false);
    list_from_fn(py, length, |_| {
        let item = items.next().expect("batch item count is exact");
        let input = ascii_or_bytes(py, &item, "s")?;
        DecodePlan::new(&input, options).execute_allocating(py)
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

/// Decode each item into its matching bytearray and return the byte counts.
///
/// ``items`` and ``outputs`` must be lists of equal length. Each destination must be a different bytearray.
/// Each destination keeps its size. The function changes only the written prefix.
/// The function stops at the first error. It does not restore destinations that it changed.
/// It can change part of the failing destination. The function retains the GIL because outputs are mutable.
/// It copies all inputs that overlap a destination before it writes to the first destination.
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
    let items = list_items(items)?;
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut prepared = prepare_batch_inputs(&items, &outputs, BatchInputKind::AsciiOrBytes)?
        .into_iter()
        .peekable();
    let options = DecodeOptions::new(altchars, Some(validate), true, None, false);
    list_from_fn(py, items.len(), |index| {
        let output = outputs.get(index);
        match prepared
            .peek()
            .is_some_and(|(prepared_index, _)| *prepared_index == index)
            .then(|| prepared.next().expect("matching prepared input exists").1)
        {
            Some(Ok(input)) => Ok(PyInt::new(
                py,
                DecodePlan::new(&input, options).execute_into(py, output)?,
            )),
            Some(Err(error)) => Err(error),
            None => {
                let input = ascii_or_bytes(py, &items[index], "s")?;
                Ok(PyInt::new(
                    py,
                    DecodePlan::new(&input, options).execute_into(py, output)?,
                ))
            }
        }
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
