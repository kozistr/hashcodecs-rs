use pyo3::exceptions::{PyMemoryError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList};

use crate::bindings::buffer::{BytesLike, bytes_like, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size, list_from_fn, list_items};
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::objects::{exact_bytes_at, exact_small_bytes};
use crate::bindings::runtime::DETACH_THRESHOLD;
use crate::xxhash::{
    xxh3_64_batch as xxh3_64_batch_hash, xxh3_64_batch_each, xxh3_128_batch as xxh3_128_batch_hash,
    xxh3_128_batch_each,
};

#[cfg(not(Py_GIL_DISABLED))]
fn exact_small_inputs<'a>(items: &'a Bound<'_, PyList>) -> Option<Vec<&'a [u8]>> {
    // The GIL keeps list slots alive for this small-input path. Larger inputs
    // retain owned references before releasing the GIL in the fallback below.
    if !exact_small_bytes(items, DETACH_THRESHOLD) {
        return None;
    }
    Some(
        (0..items.len())
            .map(|index| unsafe { exact_bytes_at(items, index) })
            .collect(),
    )
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

fn direct_output_safe(inputs: &[BytesLike<'_, '_>], detach: bool) -> bool {
    !detach
        && inputs
            .iter()
            .all(|input| !matches!(input, BytesLike::Buffer(_)))
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> Vec<&'a [u8]> {
    inputs.iter().map(BytesLike::stable_bytes).collect()
}

fn xxh3_64_hashes(py: Python<'_>, items: &Bound<'_, PyList>, seed: u64) -> PyResult<Vec<u64>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_inputs(items) {
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
    if let Some(inputs) = exact_small_inputs(items) {
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
        write_packed_64_at(output, index, *hash);
    }
}

fn write_packed_128(output: &Bound<'_, PyByteArray>, hashes: &[[u64; 2]]) {
    let output = unsafe { bytearray_data(output.as_ptr()) };
    for (index, [low, high]) in hashes.iter().enumerate() {
        write_packed_128_at(output, index, [*low, *high]);
    }
}

fn write_packed_64_at(output: *mut u8, index: usize, hash: u64) {
    let bytes = hash.to_le_bytes();
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), output.add(index * 8), 8) };
}

fn write_packed_128_at(output: *mut u8, index: usize, [low, high]: [u64; 2]) {
    let offset = index * 16;
    let low = low.to_le_bytes();
    let high = high.to_le_bytes();
    unsafe {
        std::ptr::copy_nonoverlapping(low.as_ptr(), output.add(offset), 8);
        std::ptr::copy_nonoverlapping(high.as_ptr(), output.add(offset + 8), 8);
    }
}

fn write_direct_64(
    output: &Bound<'_, PyByteArray>,
    inputs: &[&[u8]],
    seed: u64,
) -> PyResult<usize> {
    with_bytearray(output, || {
        let written = packed_output_len(output, inputs.len(), 8)?;
        let output = unsafe { bytearray_data(output.as_ptr()) };
        let mut index = 0;
        xxh3_64_batch_each(inputs, seed, |hash| {
            write_packed_64_at(output, index, hash);
            index += 1;
        });
        debug_assert_eq!(index, inputs.len());
        Ok(written)
    })
}

fn write_direct_128(
    output: &Bound<'_, PyByteArray>,
    inputs: &[&[u8]],
    seed: u64,
) -> PyResult<usize> {
    with_bytearray(output, || {
        let written = packed_output_len(output, inputs.len(), 16)?;
        let output = unsafe { bytearray_data(output.as_ptr()) };
        let mut index = 0;
        xxh3_128_batch_each(inputs, seed, |hash| {
            write_packed_128_at(output, index, hash);
            index += 1;
        });
        debug_assert_eq!(index, inputs.len());
        Ok(written)
    })
}

pub(super) fn xxh3_64_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let hashes = xxh3_64_hashes(py, items, seed)?;
    let mut hashes = hashes.into_iter();
    list_from_fn(py, hashes.len(), |_| {
        Ok(PyInt::new(py, hashes.next().expect("hash count is exact")))
    })
}

pub(super) fn xxh3_128_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let hashes = xxh3_128_hashes(py, items, seed)?;
    let mut hashes = hashes.into_iter();
    list_from_fn(py, hashes.len(), |_| {
        let [low, high] = hashes.next().expect("hash count is exact");
        Ok(PyInt::new(py, (u128::from(high) << 64) | u128::from(low)))
    })
}

pub(super) fn xxh3_64_batch_into(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    output: &Bound<'_, PyByteArray>,
    seed: u64,
) -> PyResult<usize> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_inputs(items) {
        return write_direct_64(output, &inputs, seed);
    }
    let items = list_items(items);
    with_bytearray(output, || packed_output_len(output, items.len(), 8))?;
    let parsed = parse_batch(py, &items)?;
    let detach = batch_detach_safe(&parsed);
    let direct = direct_output_safe(&parsed, detach);
    let inputs = borrow_batch(&parsed);
    if direct {
        return write_direct_64(output, &inputs, seed);
    }
    let hashes = if detach {
        py.detach(|| xxh3_64_batch_hash(&inputs, seed))
    } else {
        xxh3_64_batch_hash(&inputs, seed)
    };
    with_bytearray(output, || {
        let written = packed_output_len(output, hashes.len(), 8)?;
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
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(inputs) = exact_small_inputs(items) {
        return write_direct_128(output, &inputs, seed);
    }
    let items = list_items(items);
    with_bytearray(output, || packed_output_len(output, items.len(), 16))?;
    let parsed = parse_batch(py, &items)?;
    let detach = batch_detach_safe(&parsed);
    let direct = direct_output_safe(&parsed, detach);
    let inputs = borrow_batch(&parsed);
    if direct {
        return write_direct_128(output, &inputs, seed);
    }
    let hashes = if detach {
        py.detach(|| xxh3_128_batch_hash(&inputs, seed))
    } else {
        xxh3_128_batch_hash(&inputs, seed)
    };
    with_bytearray(output, || {
        let written = packed_output_len(output, hashes.len(), 16)?;
        write_packed_128(output, &hashes);
        Ok(written)
    })
}
