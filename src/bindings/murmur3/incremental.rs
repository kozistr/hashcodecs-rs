use pyo3::marker::Ungil;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::digest::{hex_digest, x64_128_digest, x86_128_digest};
use crate::bindings::buffer::{BytesLike, bytes_like};
use crate::bindings::runtime::MURMUR3_DETACH_THRESHOLD;
use crate::murmur3::{Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128};

fn with_input<T: Ungil>(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    operation: impl Ungil + Send + FnOnce(&[u8]) -> T,
) -> T {
    let detach = input.detach_safe() && input.len() >= MURMUR3_DETACH_THRESHOLD;
    unsafe {
        input.with_bytes(|input| {
            if detach {
                py.detach(|| operation(input))
            } else {
                operation(input)
            }
        })
    }
}

/// Incremental MurmurHash3 x86 32-bit hasher.
///
/// Args:
///     data: Optional initial bytes-like data.
///     seed: Initial unsigned 32-bit seed.
///
/// Examples:
///     >>> hasher = murmur3_x86_32(b'hello', seed=7)
///     >>> hasher.update(b' world')
///     >>> hasher.hexdigest() == hasher.digest().hex()
///     True
#[pyclass(
    name = "murmur3_x86_32",
    module = "hashcodecs.murmur3",
    skip_from_py_object
)]
#[derive(Clone)]
pub(in crate::bindings) struct PyMurmur3X86Hasher32 {
    state: Murmur3X86Hasher32,
}

#[pymethods]
impl PyMurmur3X86Hasher32 {
    /// Initialize an incremental x86 32-bit hash state.
    ///
    /// Args:
    ///     data: Optional initial bytes-like data.
    ///     seed: Initial unsigned 32-bit seed.
    ///
    /// Raises:
    ///     TypeError: data is not bytes-like or seed is not an integer.
    ///     OverflowError: seed is outside 0 <= seed < 2**32.
    #[new]
    #[pyo3(signature = (data=None, seed=0))]
    fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        let mut state = Murmur3X86Hasher32::new(seed);
        if let Some(data) = data {
            let input = bytes_like(py, data, "data")?;
            with_input(py, &input, |input| state.update(input));
        }
        Ok(Self { state })
    }

    /// Append bytes to the hash state.
    ///
    /// Args:
    ///     data: Bytes-like data to append.
    ///
    /// Returns:
    ///     None.
    ///
    /// Raises:
    ///     TypeError: data is not bytes-like.
    ///
    /// Examples:
    ///     >>> hasher = murmur3_x86_32()
    ///     >>> hasher.update(b'hello')
    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        with_input(py, &input, |input| self.state.update(input));
        Ok(())
    }

    /// Return the current digest without consuming the state.
    ///
    /// Returns:
    ///     A four-byte little-endian digest.
    ///
    /// Examples:
    ///     >>> len(murmur3_x86_32(b'hello').digest())
    ///     4
    fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.state.digest().to_le_bytes())
    }

    /// Return the current digest as lowercase hexadecimal text.
    ///
    /// Returns:
    ///     An eight-character hexadecimal string.
    ///
    /// Examples:
    ///     >>> murmur3_x86_32(b'hello').hexdigest()
    ///     '47fa8b24'
    fn hexdigest(&self) -> String {
        hex_digest(&self.state.digest().to_le_bytes())
    }

    /// Return an independent copy of the current hash state.
    ///
    /// Returns:
    ///     A hasher with the same state.
    ///
    /// Examples:
    ///     >>> original = murmur3_x86_32(b'prefix')
    ///     >>> original.copy().digest() == original.digest()
    ///     True
    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    /// The digest size in bytes (4).
    const fn digest_size(&self) -> usize {
        4
    }

    #[getter]
    /// The algorithm block size in bytes (4).
    const fn block_size(&self) -> usize {
        4
    }

    #[getter]
    /// The algorithm name.
    const fn name(&self) -> &'static str {
        "murmur3_x86_32"
    }
}

/// Incremental MurmurHash3 x86 128-bit hasher.
///
/// Args:
///     data: Optional initial bytes-like data.
///     seed: Initial unsigned 32-bit seed.
///
/// Examples:
///     >>> hasher = murmur3_x86_128(b'hello', seed=7)
///     >>> hasher.update(b' world')
///     >>> len(hasher.digest())
///     16
#[pyclass(
    name = "murmur3_x86_128",
    module = "hashcodecs.murmur3",
    skip_from_py_object
)]
#[derive(Clone)]
pub(in crate::bindings) struct PyMurmur3X86Hasher128 {
    state: Murmur3X86Hasher128,
}

#[pymethods]
impl PyMurmur3X86Hasher128 {
    /// Initialize an incremental x86 128-bit hash state.
    ///
    /// Args:
    ///     data: Optional initial bytes-like data.
    ///     seed: Initial unsigned 32-bit seed.
    ///
    /// Raises:
    ///     TypeError: data is not bytes-like or seed is not an integer.
    ///     OverflowError: seed is outside 0 <= seed < 2**32.
    #[new]
    #[pyo3(signature = (data=None, seed=0))]
    fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        let mut state = Murmur3X86Hasher128::new(seed);
        if let Some(data) = data {
            let input = bytes_like(py, data, "data")?;
            with_input(py, &input, |input| state.update(input));
        }
        Ok(Self { state })
    }

    /// Append bytes to the hash state.
    ///
    /// Args:
    ///     data: Bytes-like data to append.
    ///
    /// Returns:
    ///     None.
    ///
    /// Raises:
    ///     TypeError: data is not bytes-like.
    ///
    /// Examples:
    ///     >>> hasher = murmur3_x86_128()
    ///     >>> hasher.update(b'hello')
    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        with_input(py, &input, |input| self.state.update(input));
        Ok(())
    }

    /// Return the current digest without consuming the state.
    ///
    /// Returns:
    ///     A 16-byte digest containing four little-endian 32-bit words.
    ///
    /// Examples:
    ///     >>> len(murmur3_x86_128(b'hello').digest())
    ///     16
    fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &x86_128_digest(self.state.digest()))
    }

    /// Return the current digest as lowercase hexadecimal text.
    ///
    /// Returns:
    ///     A 32-character hexadecimal string.
    ///
    /// Examples:
    ///     >>> len(murmur3_x86_128(b'hello').hexdigest())
    ///     32
    fn hexdigest(&self) -> String {
        hex_digest(&x86_128_digest(self.state.digest()))
    }

    /// Return an independent copy of the current hash state.
    ///
    /// Returns:
    ///     A hasher with the same state.
    ///
    /// Examples:
    ///     >>> original = murmur3_x86_128(b'prefix')
    ///     >>> original.copy().digest() == original.digest()
    ///     True
    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    /// The digest size in bytes (16).
    const fn digest_size(&self) -> usize {
        16
    }

    #[getter]
    /// The algorithm block size in bytes (16).
    const fn block_size(&self) -> usize {
        16
    }

    #[getter]
    /// The algorithm name.
    const fn name(&self) -> &'static str {
        "murmur3_x86_128"
    }
}

/// Incremental MurmurHash3 x64 128-bit hasher.
///
/// Args:
///     data: Optional initial bytes-like data.
///     seed: Initial unsigned 32-bit seed.
///
/// Examples:
///     >>> hasher = murmur3_x64_128(b'hello', seed=7)
///     >>> checkpoint = hasher.copy()
///     >>> hasher.update(b' world')
///     >>> hasher.digest() != checkpoint.digest()
///     True
#[pyclass(
    name = "murmur3_x64_128",
    module = "hashcodecs.murmur3",
    skip_from_py_object
)]
#[derive(Clone)]
pub(in crate::bindings) struct PyMurmur3X64Hasher128 {
    state: Murmur3X64Hasher128,
}

#[pymethods]
impl PyMurmur3X64Hasher128 {
    /// Initialize an incremental x64 128-bit hash state.
    ///
    /// Args:
    ///     data: Optional initial bytes-like data.
    ///     seed: Initial unsigned 32-bit seed.
    ///
    /// Raises:
    ///     TypeError: data is not bytes-like or seed is not an integer.
    ///     OverflowError: seed is outside 0 <= seed < 2**32.
    #[new]
    #[pyo3(signature = (data=None, seed=0))]
    fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
        let mut state = Murmur3X64Hasher128::new(seed);
        if let Some(data) = data {
            let input = bytes_like(py, data, "data")?;
            with_input(py, &input, |input| state.update(input));
        }
        Ok(Self { state })
    }

    /// Append bytes to the hash state.
    ///
    /// Args:
    ///     data: Bytes-like data to append.
    ///
    /// Returns:
    ///     None.
    ///
    /// Raises:
    ///     TypeError: data is not bytes-like.
    ///
    /// Examples:
    ///     >>> hasher = murmur3_x64_128()
    ///     >>> hasher.update(b'hello')
    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        with_input(py, &input, |input| self.state.update(input));
        Ok(())
    }

    /// Return the current digest without consuming the state.
    ///
    /// Returns:
    ///     A 16-byte digest containing two little-endian 64-bit words.
    ///
    /// Examples:
    ///     >>> len(murmur3_x64_128(b'hello').digest())
    ///     16
    fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &x64_128_digest(self.state.digest()))
    }

    /// Return the current digest as lowercase hexadecimal text.
    ///
    /// Returns:
    ///     A 32-character hexadecimal string.
    ///
    /// Examples:
    ///     >>> len(murmur3_x64_128(b'hello').hexdigest())
    ///     32
    fn hexdigest(&self) -> String {
        hex_digest(&x64_128_digest(self.state.digest()))
    }

    /// Return an independent copy of the current hash state.
    ///
    /// Returns:
    ///     A hasher with the same state.
    ///
    /// Examples:
    ///     >>> original = murmur3_x64_128(b'prefix')
    ///     >>> original.copy().digest() == original.digest()
    ///     True
    fn copy(&self) -> Self {
        self.clone()
    }

    #[getter]
    /// The digest size in bytes (16).
    const fn digest_size(&self) -> usize {
        16
    }

    #[getter]
    /// The algorithm block size in bytes (16).
    const fn block_size(&self) -> usize {
        16
    }

    #[getter]
    /// The algorithm name.
    const fn name(&self) -> &'static str {
        "murmur3_x64_128"
    }
}
