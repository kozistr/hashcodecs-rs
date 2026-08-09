use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::DETACH_THRESHOLD;
use super::buffer::bytes_like;
use crate::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128};

#[pyfunction(signature = (s, seed=0))]
pub(super) fn murmur3_32(py: Python<'_>, s: &Bound<'_, PyAny>, seed: u32) -> PyResult<u32> {
    let input = bytes_like(py, s, "s")?;
    let hash = || murmur3_x86_32(input.as_bytes(), seed);
    Ok(if input.as_bytes().len() >= DETACH_THRESHOLD {
        py.detach(hash)
    } else {
        hash()
    })
}

#[pyfunction(signature = (s, seed=0))]
pub(super) fn murmur3_x86_128_digest<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = bytes_like(py, s, "s")?;
    let hash = || murmur3_x86_128(input.as_bytes(), seed);
    let words = if input.as_bytes().len() >= DETACH_THRESHOLD {
        py.detach(hash)
    } else {
        hash()
    };
    let mut digest = [0_u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok(PyBytes::new(py, &digest))
}

#[pyfunction(signature = (s, seed=0))]
pub(super) fn murmur3_x64_128_digest<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = bytes_like(py, s, "s")?;
    let hash = || murmur3_x64_128(input.as_bytes(), seed);
    let words = if input.as_bytes().len() >= DETACH_THRESHOLD {
        py.detach(hash)
    } else {
        hash()
    };
    let mut digest = [0_u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    Ok(PyBytes::new(py, &digest))
}
