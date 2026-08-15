use pyo3::prelude::*;
use pyo3::types::{PyInt, PyList};

use super::DETACH_THRESHOLD;
use super::buffer::{BytesLike, bytes_like};
use crate::{
    xxh3_64 as xxh3_64_hash, xxh3_64_batch as xxh3_64_batch_hash, xxh3_128 as xxh3_128_hash,
    xxh3_128_batch as xxh3_128_batch_hash,
};

fn hash64(py: Python<'_>, value: &Bound<'_, PyAny>, seed: u64) -> PyResult<u64> {
    let input = bytes_like(py, value, "s")?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    Ok(unsafe {
        input.with_bytes(|bytes| {
            if detach {
                py.detach(|| xxh3_64_hash(bytes, seed))
            } else {
                xxh3_64_hash(bytes, seed)
            }
        })
    })
}

fn hash128(py: Python<'_>, value: &Bound<'_, PyAny>, seed: u64) -> PyResult<u128> {
    let input = bytes_like(py, value, "s")?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    Ok(unsafe {
        input.with_bytes(|bytes| {
            let [low, high] = if detach {
                py.detach(|| xxh3_128_hash(bytes, seed))
            } else {
                xxh3_128_hash(bytes, seed)
            };
            (u128::from(high) << 64) | u128::from(low)
        })
    })
}

fn parse_batch<'a, 'py>(
    py: Python<'py>,
    items: &'a [Bound<'py, PyAny>],
) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    items
        .iter()
        .map(|item| bytes_like(py, item, "items element"))
        .collect()
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= DETACH_THRESHOLD
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> Vec<&'a [u8]> {
    inputs
        .iter()
        .map(|input| unsafe { input.as_bytes() })
        .collect()
}

#[pyfunction(signature = (s, seed=0))]
pub(super) fn xxh3_64(py: Python<'_>, s: &Bound<'_, PyAny>, seed: u64) -> PyResult<u64> {
    hash64(py, s, seed)
}

#[pyfunction(signature = (s, seed=0))]
pub(super) fn xxh3_128<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u64,
) -> PyResult<Bound<'py, PyInt>> {
    Ok(PyInt::new(py, hash128(py, s, seed)?))
}

#[pyfunction(signature = (items, seed=0))]
pub(super) fn xxh3_64_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let items = items.iter().collect::<Vec<_>>();
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

#[pyfunction(signature = (items, seed=0))]
pub(super) fn xxh3_128_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let items = items.iter().collect::<Vec<_>>();
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
