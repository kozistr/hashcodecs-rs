use pyo3::prelude::*;
use pyo3::types::{PyAny, PyInt, PyList};

#[cfg(not(Py_GIL_DISABLED))]
use super::super::EXACT_BYTES_BATCH_MAX;
use super::super::batch::{BatchInputKind, batch_outputs, prepare_batch_inputs};
use super::super::{encode_parsed, encode_parsed_into, parse_altchars};
#[cfg(not(Py_GIL_DISABLED))]
use super::encode_exact;
use crate::bindings::buffer::contiguous_bytes_like;
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::objects::exact_bytes_up_to;
use crate::bindings::objects::{list_from_fn, list_items};

/// Encode each bytes-like item and return the results in input order.
///
/// ``items`` must be a list. ``altchars`` applies to every item.
/// The function stops at the first error and discards the partial result list.
/// The function uses one thread. It releases the GIL for each immutable item of at least 256 KiB.
/// It retains the GIL for smaller or mutable items. Do not change ``items`` during the call.
pub(in crate::bindings::base64) fn b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    b64encode_batch_parsed(py, items, altchars)
}

fn b64encode_batch_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyList>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(items) = exact_bytes_up_to(items, EXACT_BYTES_BATCH_MAX)? {
        // Validation retains every input before allocating the output list.
        // Creating a GC-tracked Python object can run finalizers which mutate
        // the original list.
        let length = items.len();
        let mut items = items.into_iter();
        return list_from_fn(py, length, |_| {
            let item = items.next().expect("batch item count is exact");
            encode_exact(py, item.as_bytes(), altchars, true, None)
        });
    }
    let items = list_items(items)?;
    let length = items.len();
    let mut items = items.into_iter();
    list_from_fn(py, length, |_| {
        encode_parsed(
            py,
            &items.next().expect("batch item count is exact"),
            altchars,
            true,
            None,
        )
    })
}

pub(in crate::bindings::base64) fn standard_b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_parsed(py, items, None)
}

pub(in crate::bindings::base64) fn urlsafe_b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_parsed(py, items, Some(*b"-_"))
}

/// Encode each item into its matching bytearray and return the byte counts.
///
/// ``items`` and ``outputs`` must be lists of equal length. Each destination must be a different bytearray.
/// Each destination keeps its size. The function changes only the written prefix.
/// The function stops at the first error. It does not restore destinations that it changed.
/// The function retains the GIL because outputs are mutable.
/// It copies all inputs that overlap a destination before it writes to the first destination.
pub(in crate::bindings::base64) fn b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    b64encode_batch_into_parsed(py, items, outputs, altchars)
}

fn b64encode_batch_into_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items)?;
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut prepared = prepare_batch_inputs(&items, &outputs, BatchInputKind::Contiguous)?
        .into_iter()
        .peekable();
    list_from_fn(py, items.len(), |index| {
        let output = outputs.get(index);
        match prepared
            .peek()
            .is_some_and(|(prepared_index, _)| *prepared_index == index)
            .then(|| prepared.next().expect("matching prepared input exists").1)
        {
            Some(Ok(input)) => Ok(PyInt::new(
                py,
                encode_parsed_into(&input, output, altchars, true, None)?,
            )),
            Some(Err(error)) => Err(error),
            None => {
                let input = contiguous_bytes_like(py, &items[index], "s")?;
                Ok(PyInt::new(
                    py,
                    encode_parsed_into(&input, output, altchars, true, None)?,
                ))
            }
        }
    })
}

pub(in crate::bindings::base64) fn standard_b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_into_parsed(py, items, outputs, None)
}

pub(in crate::bindings::base64) fn urlsafe_b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_into_parsed(py, items, outputs, Some(*b"-_"))
}
