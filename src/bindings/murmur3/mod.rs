use std::ptr;

use pyo3::ffi;
use pyo3::marker::Ungil;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::buffer::{BytesLike, bytes_like};
use super::{
    DETACH_THRESHOLD, METHOD_FLAGS, parse_hash_arguments, return_function_result, seed_u32,
    with_function_bytes,
};
use crate::{
    Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128, murmur3_x64_128, murmur3_x86_32,
    murmur3_x86_128,
};

mod methods;

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

fn with_input<T: Ungil>(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    operation: impl Ungil + Send + FnOnce(&[u8]) -> T,
) -> T {
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
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

fn bytes_result(digest: &[u8]) -> *mut ffi::PyObject {
    unsafe { ffi::PyBytes_FromStringAndSize(digest.as_ptr().cast(), digest.len() as isize) }
}

unsafe extern "C" fn murmur3_32(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"murmur3_32".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| murmur3_x86_32(bytes, seed));
        return_function_result(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLong(value as _)),
        )
    }
}

unsafe extern "C" fn murmur3_x86_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_hash_arguments(args, nargsf, keywords, c"murmur3_x86_128_digest".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| {
            x86_128_digest(murmur3_x86_128(bytes, seed))
        });
        return_function_result(py, result.map(|digest| bytes_result(&digest)))
    }
}

unsafe extern "C" fn murmur3_x64_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_hash_arguments(args, nargsf, keywords, c"murmur3_x64_128_digest".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| {
            x64_128_digest(murmur3_x64_128(bytes, seed))
        });
        return_function_result(py, result.map(|digest| bytes_result(&digest)))
    }
}

pub(super) use methods::add_to_module;

/// Incremental MurmurHash3 x86 32-bit hasher.
///
/// Args:
///     data: Optional initial contiguous bytes-like data.
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
pub(super) struct PyMurmur3X86Hasher32 {
    state: Murmur3X86Hasher32,
}

#[pymethods]
impl PyMurmur3X86Hasher32 {
    /// Initialize an incremental x86 32-bit hash state.
    ///
    /// Args:
    ///     data: Optional initial contiguous bytes-like data.
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
    ///     data: Contiguous bytes-like data to append.
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
///     data: Optional initial contiguous bytes-like data.
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
pub(super) struct PyMurmur3X86Hasher128 {
    state: Murmur3X86Hasher128,
}

#[pymethods]
impl PyMurmur3X86Hasher128 {
    /// Initialize an incremental x86 128-bit hash state.
    ///
    /// Args:
    ///     data: Optional initial contiguous bytes-like data.
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
    ///     data: Contiguous bytes-like data to append.
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
///     data: Optional initial contiguous bytes-like data.
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
pub(super) struct PyMurmur3X64Hasher128 {
    state: Murmur3X64Hasher128,
}

#[pymethods]
impl PyMurmur3X64Hasher128 {
    /// Initialize an incremental x64 128-bit hash state.
    ///
    /// Args:
    ///     data: Optional initial contiguous bytes-like data.
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
    ///     data: Contiguous bytes-like data to append.
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
