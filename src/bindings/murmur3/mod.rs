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

static mut METHODS: [ffi::PyMethodDef; 4] = [
    ffi::PyMethodDef {
        ml_name: c"murmur3_32".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_32,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"murmur3_32($module, /, s, seed=0)\n--\n\nReturn the unsigned 32-bit x86 MurmurHash3 value for a bytes-like object.\n\nseed must be an unsigned 32-bit integer. MurmurHash3 is non-cryptographic.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"murmur3_x86_128_digest".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_x86_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"murmur3_x86_128_digest($module, /, s, seed=0)\n--\n\nReturn the 16-byte x86-128 MurmurHash3 digest for a bytes-like object.\n\nseed must be an unsigned 32-bit integer. MurmurHash3 is non-cryptographic.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"murmur3_x64_128_digest".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_x64_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"murmur3_x64_128_digest($module, /, s, seed=0)\n--\n\nReturn the 16-byte x64-128 MurmurHash3 digest for a bytes-like object.\n\nseed must be an unsigned 32-bit integer. MurmurHash3 is non-cryptographic.".as_ptr(),
    },
    ffi::PyMethodDef::zeroed(),
];

pub(super) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    let result = unsafe { ffi::PyModule_AddFunctions(module.as_ptr(), methods) };
    if result == -1 {
        Err(PyErr::fetch(module.py()))
    } else {
        Ok(())
    }
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
            with_input(py, &input, |input| state.update(input));
        }
        Ok(Self { state })
    }

    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        with_input(py, &input, |input| self.state.update(input));
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
            with_input(py, &input, |input| state.update(input));
        }
        Ok(Self { state })
    }

    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        with_input(py, &input, |input| self.state.update(input));
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
            with_input(py, &input, |input| state.update(input));
        }
        Ok(Self { state })
    }

    fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let input = bytes_like(py, data, "data")?;
        with_input(py, &input, |input| self.state.update(input));
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
