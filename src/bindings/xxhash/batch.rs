use pyo3::exceptions::{PyMemoryError, PyValueError};
#[cfg(not(Py_GIL_DISABLED))]
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList};

use crate::bindings::buffer::{BytesLike, bytes_like, with_bytearray};
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::objects::bytes_data;
use crate::bindings::objects::{bytearray_data, bytearray_size, list_items};
use crate::bindings::runtime::DETACH_THRESHOLD;
use crate::xxhash::{xxh3_64_batch as xxh3_64_batch_hash, xxh3_128_batch as xxh3_128_batch_hash};

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

fn parse_batch<'a, 'py>(
    py: Python<'py>,
    items: &'a [Bound<'py, PyAny>],
) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    let inputs = items
        .iter()
        .map(|item| bytes_like(py, item, "items element"))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(inputs
        .into_iter()
        .map(|input| {
            if input.detach_safe() || matches!(input, BytesLike::Buffer(_)) {
                input
            } else {
                BytesLike::Owned(unsafe { input.with_bytes(<[u8]>::to_vec) })
            }
        })
        .collect())
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= DETACH_THRESHOLD
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> Vec<&'a [u8]> {
    inputs.iter().map(BytesLike::stable_bytes).collect()
}

fn xxh3_64_hashes(py: Python<'_>, items: &Bound<'_, PyList>, seed: u64) -> PyResult<Vec<u64>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_bytes(items) {
        return Ok(xxh3_64_batch_hash(&inputs, seed));
    }
    let items = list_items(items);
    let parsed = parse_batch(py, &items)?;
    let detach = batch_detach_safe(&parsed);
    let inputs = borrow_batch(&parsed);
    Ok(if detach {
        py.detach(|| xxh3_64_batch_hash(&inputs, seed))
    } else {
        xxh3_64_batch_hash(&inputs, seed)
    })
}

fn xxh3_128_hashes(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    seed: u64,
) -> PyResult<Vec<[u64; 2]>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_bytes(items) {
        return Ok(xxh3_128_batch_hash(&inputs, seed));
    }
    let items = list_items(items);
    let parsed = parse_batch(py, &items)?;
    let detach = batch_detach_safe(&parsed);
    let inputs = borrow_batch(&parsed);
    Ok(if detach {
        py.detach(|| xxh3_128_batch_hash(&inputs, seed))
    } else {
        xxh3_128_batch_hash(&inputs, seed)
    })
}

fn packed_output_len(
    output: &Bound<'_, PyByteArray>,
    items: usize,
    digest_size: usize,
) -> PyResult<usize> {
    let required = items
        .checked_mul(digest_size)
        .ok_or_else(|| PyMemoryError::new_err("XXH3 batch output is too large"))?;
    let provided = unsafe { bytearray_size(output.as_ptr()) };
    if provided < required {
        return Err(PyValueError::new_err(format!(
            "XXH3 batch output requires {required} bytes but the destination has {provided}"
        )));
    }
    Ok(required)
}

fn write_packed_64(output: &Bound<'_, PyByteArray>, hashes: &[u64]) {
    let output = unsafe { bytearray_data(output.as_ptr()) };
    for (index, hash) in hashes.iter().enumerate() {
        let bytes = hash.to_le_bytes();
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), output.add(index * 8), 8) };
    }
}

fn write_packed_128(output: &Bound<'_, PyByteArray>, hashes: &[[u64; 2]]) {
    let output = unsafe { bytearray_data(output.as_ptr()) };
    for (index, [low, high]) in hashes.iter().enumerate() {
        let offset = index * 16;
        let low = low.to_le_bytes();
        let high = high.to_le_bytes();
        unsafe {
            std::ptr::copy_nonoverlapping(low.as_ptr(), output.add(offset), 8);
            std::ptr::copy_nonoverlapping(high.as_ptr(), output.add(offset + 8), 8);
        }
    }
}

pub(super) fn xxh3_64_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    PyList::new(py, xxh3_64_hashes(py, items, seed)?)
}

pub(super) fn xxh3_128_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let hashes = xxh3_128_hashes(py, items, seed)?
        .into_iter()
        .map(|[low, high]| PyInt::new(py, (u128::from(high) << 64) | u128::from(low)));
    PyList::new(py, hashes)
}

pub(super) fn xxh3_64_batch_into(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    output: &Bound<'_, PyByteArray>,
    seed: u64,
) -> PyResult<usize> {
    let hashes = xxh3_64_hashes(py, items, seed)?;
    with_bytearray(output, || {
        let written = packed_output_len(output, items.len(), 8)?;
        write_packed_64(output, &hashes);
        Ok(written)
    })
}

pub(super) fn xxh3_128_batch_into(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    output: &Bound<'_, PyByteArray>,
    seed: u64,
) -> PyResult<usize> {
    let hashes = xxh3_128_hashes(py, items, seed)?;
    with_bytearray(output, || {
        let written = packed_output_len(output, items.len(), 16)?;
        write_packed_128(output, &hashes);
        Ok(written)
    })
}
