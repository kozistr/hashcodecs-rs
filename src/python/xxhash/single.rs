use pyo3::prelude::*;
use pyo3::types::PyInt;

use super::{hash64, hash128};

#[pyfunction(signature = (s, seed=0))]
pub fn xxh3_64(py: Python<'_>, s: &Bound<'_, PyAny>, seed: u64) -> PyResult<u64> {
    hash64(py, s, seed)
}

#[pyfunction(signature = (s, seed=0))]
pub fn xxh3_128<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u64,
) -> PyResult<Bound<'py, PyInt>> {
    Ok(PyInt::new(py, hash128(py, s, seed)?))
}
