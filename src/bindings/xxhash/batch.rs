use pyo3::exceptions::{PyMemoryError, PyValueError};
use pyo3::prelude::*;
#[cfg(not(Py_GIL_DISABLED))]
use pyo3::types::PyBytes;
use pyo3::types::{PyByteArray, PyInt, PyList};

use crate::bindings::buffer::{BytesLike, bytes_like, with_bytearray};
use crate::bindings::objects::{
    batch_results, bytearray_data, bytearray_size, list_from_fn, list_items,
};
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::objects::{
    exact_bytes_at, exact_bytes_total, exact_bytes_up_to, list_from_callback,
};
use crate::bindings::runtime::XXH3_DETACH_THRESHOLD;
use crate::xxhash::{xxh3_64_batch_for_each, xxh3_128_batch_for_each};

const BATCH_TOO_LARGE: &str = "XXH3 batch is too large";

#[cfg(not(Py_GIL_DISABLED))]
enum ExactBytesBatch<'a, 'py> {
    Borrowed(Vec<&'a [u8]>),
    Retained(Vec<Bound<'py, PyBytes>>),
}

#[cfg(not(Py_GIL_DISABLED))]
// The borrowed variant is only for packed output: it must be consumed without
// allocating Python objects or detaching from the interpreter.
fn exact_bytes_batch_into<'a, 'py>(
    items: &'a Bound<'py, PyList>,
) -> PyResult<Option<ExactBytesBatch<'a, 'py>>> {
    let Some(total) = exact_bytes_total(items) else {
        return Ok(None);
    };
    if total >= XXH3_DETACH_THRESHOLD {
        let retained = exact_bytes_up_to(items, usize::MAX)?
            .expect("the exact-bytes scan and retention observe the same GIL-protected list");
        return Ok(Some(ExactBytesBatch::Retained(retained)));
    }
    let mut inputs = batch_results(items.len(), BATCH_TOO_LARGE)?;
    inputs.extend((0..items.len()).map(|index| unsafe { exact_bytes_at(items, index) }));
    Ok(Some(ExactBytesBatch::Borrowed(inputs)))
}

#[cfg(not(Py_GIL_DISABLED))]
fn borrow_retained<'a, 'py>(retained: &'a [Bound<'py, PyBytes>]) -> PyResult<Vec<&'a [u8]>> {
    let mut inputs = batch_results(retained.len(), BATCH_TOO_LARGE)?;
    inputs.extend(retained.iter().map(|item| item.as_bytes()));
    Ok(inputs)
}

fn parse_batch<'a, 'py>(items: &'a [Bound<'py, PyAny>]) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    let mut inputs = batch_results(items.len(), BATCH_TOO_LARGE)?;
    for item in items {
        let input = bytes_like(item, "items element")?;
        #[cfg(Py_GIL_DISABLED)]
        let input = input.into_stable()?;
        inputs.push(input);
    }
    Ok(inputs)
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= XXH3_DETACH_THRESHOLD
}

fn direct_output_safe(
    inputs: &[BytesLike<'_, '_>],
    output: &Bound<'_, PyByteArray>,
    detach: bool,
) -> bool {
    !detach && inputs.iter().all(|input| !input.overlaps(output))
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> PyResult<Vec<&'a [u8]>> {
    let mut borrowed = batch_results(inputs.len(), BATCH_TOO_LARGE)?;
    borrowed.extend(inputs.iter().map(BytesLike::stable_bytes));
    Ok(borrowed)
}

fn xxh3_64_batch_results(inputs: &[&[u8]], seed: u64) -> PyResult<Vec<u64>> {
    let mut hashes = batch_results(inputs.len(), BATCH_TOO_LARGE)?;
    xxh3_64_batch_for_each(inputs, seed, |hash| hashes.push(hash));
    Ok(hashes)
}

fn xxh3_128_batch_results(inputs: &[&[u8]], seed: u64) -> PyResult<Vec<[u64; 2]>> {
    let mut hashes = batch_results(inputs.len(), BATCH_TOO_LARGE)?;
    xxh3_128_batch_for_each(inputs, seed, |hash| hashes.push(hash));
    Ok(hashes)
}

// Callers must retain immutable input owners across Python allocations: list
// allocation can run GC finalizers even while the GIL is held.
#[cfg(not(Py_GIL_DISABLED))]
fn xxh3_64_list<'py>(py: Python<'py>, inputs: &[&[u8]], seed: u64) -> PyResult<Bound<'py, PyList>> {
    list_from_callback(py, inputs.len(), |append| {
        xxh3_64_batch_for_each(inputs, seed, |hash| append(PyInt::new(py, hash)));
    })
}

#[cfg(not(Py_GIL_DISABLED))]
fn xxh3_128_list<'py>(
    py: Python<'py>,
    inputs: &[&[u8]],
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    list_from_callback(py, inputs.len(), |append| {
        xxh3_128_batch_for_each(inputs, seed, |[low, high]| {
            append(PyInt::new(py, (u128::from(high) << 64) | u128::from(low)));
        });
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
        xxh3_64_batch_for_each(inputs, seed, |hash| {
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
        xxh3_128_batch_for_each(inputs, seed, |hash| {
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
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(total) = exact_bytes_total(items) {
        let retained = exact_bytes_up_to(items, usize::MAX)?
            .expect("the exact-bytes scan and retention observe the same GIL-protected list");
        let inputs = borrow_retained(&retained)?;
        if total < XXH3_DETACH_THRESHOLD {
            return xxh3_64_list(py, &inputs, seed);
        }
        let hashes = py.detach(|| xxh3_64_batch_results(&inputs, seed))?;
        let mut hashes = hashes.into_iter();
        return list_from_fn(py, hashes.len(), |_| {
            Ok(PyInt::new(py, hashes.next().expect("hash count is exact")))
        });
    }
    let items = list_items(items)?;
    let parsed = parse_batch(&items)?;
    let detach = batch_detach_safe(&parsed);
    // Finish reading mutable storage before any Python container allocation
    // can run a finalizer that resizes or mutates it. Retaining its owner alone
    // does not stabilize a bytearray's backing allocation.
    let hashes = {
        let inputs = borrow_batch(&parsed)?;
        if detach {
            py.detach(|| xxh3_64_batch_results(&inputs, seed))?
        } else {
            xxh3_64_batch_results(&inputs, seed)?
        }
    };
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
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(total) = exact_bytes_total(items) {
        let retained = exact_bytes_up_to(items, usize::MAX)?
            .expect("the exact-bytes scan and retention observe the same GIL-protected list");
        let inputs = borrow_retained(&retained)?;
        if total < XXH3_DETACH_THRESHOLD {
            return xxh3_128_list(py, &inputs, seed);
        }
        let hashes = py.detach(|| xxh3_128_batch_results(&inputs, seed))?;
        let mut hashes = hashes.into_iter();
        return list_from_fn(py, hashes.len(), |_| {
            let [low, high] = hashes.next().expect("hash count is exact");
            Ok(PyInt::new(py, (u128::from(high) << 64) | u128::from(low)))
        });
    }
    let items = list_items(items)?;
    let parsed = parse_batch(&items)?;
    let detach = batch_detach_safe(&parsed);
    // As in the 64-bit path, mutable borrows end before Python allocation.
    let hashes = {
        let inputs = borrow_batch(&parsed)?;
        if detach {
            py.detach(|| xxh3_128_batch_results(&inputs, seed))?
        } else {
            xxh3_128_batch_results(&inputs, seed)?
        }
    };
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
    if let Some(exact) = exact_bytes_batch_into(items)? {
        return match exact {
            ExactBytesBatch::Borrowed(inputs) => write_direct_64(output, &inputs, seed),
            ExactBytesBatch::Retained(retained) => {
                with_bytearray(output, || packed_output_len(output, retained.len(), 8))?;
                let inputs = borrow_retained(&retained)?;
                let hashes = py.detach(|| xxh3_64_batch_results(&inputs, seed))?;
                with_bytearray(output, || {
                    let written = packed_output_len(output, hashes.len(), 8)?;
                    write_packed_64(output, &hashes);
                    Ok(written)
                })
            }
        };
    }
    let items = list_items(items)?;
    with_bytearray(output, || packed_output_len(output, items.len(), 8))?;
    let parsed = parse_batch(&items)?;
    let detach = batch_detach_safe(&parsed);
    let direct = direct_output_safe(&parsed, output, detach);
    let inputs = borrow_batch(&parsed)?;
    if direct {
        return write_direct_64(output, &inputs, seed);
    }
    let hashes = if detach {
        py.detach(|| xxh3_64_batch_results(&inputs, seed))?
    } else {
        xxh3_64_batch_results(&inputs, seed)?
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
    if let Some(exact) = exact_bytes_batch_into(items)? {
        return match exact {
            ExactBytesBatch::Borrowed(inputs) => write_direct_128(output, &inputs, seed),
            ExactBytesBatch::Retained(retained) => {
                with_bytearray(output, || packed_output_len(output, retained.len(), 16))?;
                let inputs = borrow_retained(&retained)?;
                let hashes = py.detach(|| xxh3_128_batch_results(&inputs, seed))?;
                with_bytearray(output, || {
                    let written = packed_output_len(output, hashes.len(), 16)?;
                    write_packed_128(output, &hashes);
                    Ok(written)
                })
            }
        };
    }
    let items = list_items(items)?;
    with_bytearray(output, || packed_output_len(output, items.len(), 16))?;
    let parsed = parse_batch(&items)?;
    let detach = batch_detach_safe(&parsed);
    let direct = direct_output_safe(&parsed, output, detach);
    let inputs = borrow_batch(&parsed)?;
    if direct {
        return write_direct_128(output, &inputs, seed);
    }
    let hashes = if detach {
        py.detach(|| xxh3_128_batch_results(&inputs, seed))?
    } else {
        xxh3_128_batch_results(&inputs, seed)?
    };
    with_bytearray(output, || {
        let written = packed_output_len(output, hashes.len(), 16)?;
        write_packed_128(output, &hashes);
        Ok(written)
    })
}

#[cfg(all(test, not(Py_GIL_DISABLED)))]
mod tests {
    use super::*;

    #[test]
    fn gil_batch_retains_exact_bytearrays() {
        Python::initialize();
        Python::attach(|py| {
            let items = [PyByteArray::new(py, b"mutable").into_any()];
            let parsed = parse_batch(&items).unwrap();
            assert!(matches!(parsed.first(), Some(BytesLike::ByteArray(_))));
        });
    }
}
