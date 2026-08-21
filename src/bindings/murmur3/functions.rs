use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::{bytes_like, with_input, x64_128_digest, x86_128_digest};
use crate::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128};

#[pyfunction(signature = (s, seed=0))]
pub fn murmur3_32(py: Python<'_>, s: &Bound<'_, PyAny>, seed: u32) -> PyResult<u32> {
    let input = bytes_like(py, s, "s")?;
    Ok(with_input(py, &input, |input| murmur3_x86_32(input, seed)))
}

#[pyfunction(signature = (s, seed=0))]
pub fn murmur3_x86_128_digest<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = bytes_like(py, s, "s")?;
    let words = with_input(py, &input, |input| murmur3_x86_128(input, seed));
    let digest = x86_128_digest(words);
    Ok(PyBytes::new(py, &digest))
}

#[pyfunction(signature = (s, seed=0))]
pub fn murmur3_x64_128_digest<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = bytes_like(py, s, "s")?;
    let words = with_input(py, &input, |input| murmur3_x64_128(input, seed));
    let digest = x64_128_digest(words);
    Ok(PyBytes::new(py, &digest))
}
