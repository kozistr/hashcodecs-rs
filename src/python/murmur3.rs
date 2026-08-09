use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::DETACH_THRESHOLD;
use super::buffer::bytes_like;
use crate::{
    Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128, murmur3_x64_128, murmur3_x86_32,
    murmur3_x86_128,
};

fn x86_128_digest(words: [u32; 4]) -> [u8; 16] {
    let mut digest = [0_u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

fn x64_128_digest(words: [u64; 2]) -> [u8; 16] {
    let mut digest = [0_u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[pyclass(
    name = "murmur3_x86_32",
    module = "hashcodecs.murmur3",
    skip_from_py_object
)]
#[derive(Clone)]
pub(super) struct PyMurmur3X86Hasher32 {
    state: Murmur3X86Hasher32,
}

#[pymethods]
impl PyMurmur3X86Hasher32 {
    #[new]
    #[pyo3(signature = (data=None, seed=0))]
    fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        let mut state = Murmur3X86Hasher32::new(seed);
        if let Some(data) = data {
            let input = bytes_like(py, data, "data")?;
            if input.as_bytes().len() >= DETACH_THRESHOLD {
                py.detach(|| state.update(input.as_bytes()));
            } else {
                state.update(input.as_bytes());
            }
        }
        Ok(Self { state })
    }

    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        if input.as_bytes().len() >= DETACH_THRESHOLD {
            py.detach(|| self.state.update(input.as_bytes()));
        } else {
            self.state.update(input.as_bytes());
        }
        Ok(())
    }

    fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.state.digest().to_le_bytes())
    }

    fn hexdigest(&self) -> String {
        hex_digest(&self.state.digest().to_le_bytes())
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    const fn digest_size(&self) -> usize {
        4
    }

    #[getter]
    const fn block_size(&self) -> usize {
        4
    }

    #[getter]
    const fn name(&self) -> &'static str {
        "murmur3_x86_32"
    }
}

#[pyclass(
    name = "murmur3_x86_128",
    module = "hashcodecs.murmur3",
    skip_from_py_object
)]
#[derive(Clone)]
pub(super) struct PyMurmur3X86Hasher128 {
    state: Murmur3X86Hasher128,
}

#[pymethods]
impl PyMurmur3X86Hasher128 {
    #[new]
    #[pyo3(signature = (data=None, seed=0))]
    fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        let mut state = Murmur3X86Hasher128::new(seed);
        if let Some(data) = data {
            let input = bytes_like(py, data, "data")?;
            if input.as_bytes().len() >= DETACH_THRESHOLD {
                py.detach(|| state.update(input.as_bytes()));
            } else {
                state.update(input.as_bytes());
            }
        }
        Ok(Self { state })
    }

    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        if input.as_bytes().len() >= DETACH_THRESHOLD {
            py.detach(|| self.state.update(input.as_bytes()));
        } else {
            self.state.update(input.as_bytes());
        }
        Ok(())
    }

    fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &x86_128_digest(self.state.digest()))
    }

    fn hexdigest(&self) -> String {
        hex_digest(&x86_128_digest(self.state.digest()))
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    const fn digest_size(&self) -> usize {
        16
    }

    #[getter]
    const fn block_size(&self) -> usize {
        16
    }

    #[getter]
    const fn name(&self) -> &'static str {
        "murmur3_x86_128"
    }
}

#[pyclass(
    name = "murmur3_x64_128",
    module = "hashcodecs.murmur3",
    skip_from_py_object
)]
#[derive(Clone)]
pub(super) struct PyMurmur3X64Hasher128 {
    state: Murmur3X64Hasher128,
}

#[pymethods]
impl PyMurmur3X64Hasher128 {
    #[new]
    #[pyo3(signature = (data=None, seed=0))]
    fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        let mut state = Murmur3X64Hasher128::new(seed);
        if let Some(data) = data {
            let input = bytes_like(py, data, "data")?;
            if input.as_bytes().len() >= DETACH_THRESHOLD {
                py.detach(|| state.update(input.as_bytes()));
            } else {
                state.update(input.as_bytes());
            }
        }
        Ok(Self { state })
    }

    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        if input.as_bytes().len() >= DETACH_THRESHOLD {
            py.detach(|| self.state.update(input.as_bytes()));
        } else {
            self.state.update(input.as_bytes());
        }
        Ok(())
    }

    fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &x64_128_digest(self.state.digest()))
    }

    fn hexdigest(&self) -> String {
        hex_digest(&x64_128_digest(self.state.digest()))
    }

    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    const fn digest_size(&self) -> usize {
        16
    }

    #[getter]
    const fn block_size(&self) -> usize {
        16
    }

    #[getter]
    const fn name(&self) -> &'static str {
        "murmur3_x64_128"
    }
}

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
    let digest = x86_128_digest(words);
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
    let digest = x64_128_digest(words);
    Ok(PyBytes::new(py, &digest))
}
