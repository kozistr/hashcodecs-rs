use pyo3::exceptions::{PyBufferError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyByteArrayMethods, PyBytes, PyList, PyString, PyTuple};

pub(super) enum BytesLike<'a, 'py> {
    Bytes(&'a Bound<'py, PyBytes>),
    ByteArray(&'a Bound<'py, PyByteArray>),
    Owned(Vec<u8>),
}

impl BytesLike<'_, '_> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.as_bytes().len(),
            Self::ByteArray(bytes) => bytes.len(),
            Self::Owned(bytes) => bytes.len(),
        }
    }

    pub(super) fn detach_safe(&self) -> bool {
        !matches!(self, Self::ByteArray(_))
    }

    pub(super) fn aliases(&self, output: &Bound<'_, PyByteArray>) -> bool {
        matches!(self, Self::ByteArray(input) if input.as_ptr() == output.as_ptr())
    }

    /// The callback must not run arbitrary Python or release the GIL when the input is mutable.
    pub(super) unsafe fn with_bytes<T>(&self, callback: impl FnOnce(&[u8]) -> T) -> T {
        match self {
            Self::Bytes(bytes) => callback(bytes.as_bytes()),
            Self::ByteArray(bytes) => callback(unsafe { bytes.as_bytes() }),
            Self::Owned(bytes) => callback(bytes),
        }
    }
}

pub(super) fn bytes_like<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a, 'py>> {
    if value.is_instance_of::<PyList>() || value.is_instance_of::<PyTuple>() {
        return Err(type_error(argument));
    }
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(BytesLike::Bytes(bytes));
    }
    if let Ok(bytes) = value.cast::<PyByteArray>() {
        return Ok(BytesLike::ByteArray(bytes));
    }
    copied_memoryview(py, value, argument, false).map(BytesLike::Owned)
}

pub(super) fn contiguous_bytes_like<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a, 'py>> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(BytesLike::Bytes(bytes));
    }
    if let Ok(bytes) = value.cast::<PyByteArray>() {
        return Ok(BytesLike::ByteArray(bytes));
    }
    copied_memoryview(py, value, argument, true).map(BytesLike::Owned)
}

pub(super) fn ascii_or_bytes<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a, 'py>> {
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str().map_err(|_| ascii_error(argument))?;
        if !text.is_ascii() {
            return Err(ascii_error(argument));
        }
        return Ok(BytesLike::Owned(text.as_bytes().to_vec()));
    }
    bytes_like(py, value, argument)
}

fn copied_memoryview<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
    argument: &str,
    require_contiguous: bool,
) -> PyResult<Vec<u8>> {
    let memoryview = py
        .import("builtins")?
        .getattr("memoryview")?
        .call1((value,))
        .map_err(|_| type_error(argument))?;
    if require_contiguous && !memoryview.getattr("c_contiguous")?.is_truthy()? {
        return Err(PyBufferError::new_err(
            "memoryview: underlying buffer is not C-contiguous",
        ));
    }
    let bytes = memoryview.call_method0("tobytes")?;
    let bytes = bytes
        .cast::<PyBytes>()
        .map_err(|_| PyTypeError::new_err("memoryview.tobytes() did not return bytes"))?;
    Ok(bytes.as_bytes().to_vec())
}

fn type_error(argument: &str) -> PyErr {
    PyTypeError::new_err(format!("{argument} must be a bytes-like object"))
}

fn ascii_error(argument: &str) -> PyErr {
    PyValueError::new_err(format!("{argument} must contain only ASCII characters"))
}
