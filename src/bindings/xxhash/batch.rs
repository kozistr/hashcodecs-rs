#[cfg(not(Py_GIL_DISABLED))]
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyList};

use super::{batch_detach_safe, borrow_batch, parse_batch};
use crate::bindings::list_items;
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::{DETACH_THRESHOLD, bytes_data};
use crate::{xxh3_64_batch as xxh3_64_batch_hash, xxh3_128_batch as xxh3_128_batch_hash};

#[cfg(not(Py_GIL_DISABLED))]
fn exact_small_bytes<'a>(items: &'a Bound<'_, PyList>) -> Option<Vec<&'a [u8]>> {
    // The GIL keeps list slots alive for this small-input path. Larger inputs
    // retain owned references before releasing the GIL in the fallback below.
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr()) as usize;
        let mut total = 0_usize;
        let mut inputs = Vec::with_capacity(length);
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index as isize);
            if ffi::PyBytes_CheckExact(item) == 0 {
                return None;
            }
            let length = ffi::Py_SIZE(item) as usize;
            total = total.checked_add(length)?;
            if total >= DETACH_THRESHOLD {
                return None;
            }
            inputs.push(std::slice::from_raw_parts(bytes_data(item), length));
        }
        Some(inputs)
    }
}

pub(super) fn xxh3_64_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_bytes(items) {
        return PyList::new(py, xxh3_64_batch_hash(&inputs, seed));
    }
    let items = list_items(items);
    let parsed = parse_batch(py, &items)?;
    let detach = batch_detach_safe(&parsed);
    let inputs = borrow_batch(&parsed);
    let hashes = if detach {
        py.detach(|| xxh3_64_batch_hash(&inputs, seed))
    } else {
        xxh3_64_batch_hash(&inputs, seed)
    };
    PyList::new(py, hashes)
}

pub(super) fn xxh3_128_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_bytes(items) {
        let hashes = xxh3_128_batch_hash(&inputs, seed)
            .into_iter()
            .map(|[low, high]| PyInt::new(py, (u128::from(high) << 64) | u128::from(low)));
        return PyList::new(py, hashes);
    }
    let items = list_items(items);
    let parsed = parse_batch(py, &items)?;
    let detach = batch_detach_safe(&parsed);
    let inputs = borrow_batch(&parsed);
    let hashes = if detach {
        py.detach(|| xxh3_128_batch_hash(&inputs, seed))
    } else {
        xxh3_128_batch_hash(&inputs, seed)
    };
    let hashes = hashes
        .into_iter()
        .map(|[low, high]| PyInt::new(py, (u128::from(high) << 64) | u128::from(low)));
    PyList::new(py, hashes)
}
