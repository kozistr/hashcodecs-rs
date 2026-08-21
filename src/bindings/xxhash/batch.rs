use pyo3::prelude::*;
use pyo3::types::{PyInt, PyList};

use super::{batch_detach_safe, borrow_batch, parse_batch};
use crate::{xxh3_64_batch as xxh3_64_batch_hash, xxh3_128_batch as xxh3_128_batch_hash};

#[pyfunction(signature = (items, seed=0))]
pub fn xxh3_64_batch<'py>(
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
pub fn xxh3_128_batch<'py>(
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
