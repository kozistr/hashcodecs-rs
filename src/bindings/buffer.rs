use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyBufferError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::critical_section::{with_critical_section, with_critical_section2};
use pyo3::types::{PyByteArray, PyBytes, PyMemoryView, PyString};

use super::objects::{bytearray_data, bytearray_size, bytes_data, bytes_size};

const MEMORYVIEW_OWNER_THRESHOLD: usize = 4 * 1024;

/// A contiguous buffer borrowed directly from an arbitrary exporter.
///
/// This is only used by GIL-enabled builds. An exporter can expose a
/// read-only view of storage which remains mutable through another handle, so
/// the GIL is the synchronization guarantee for unknown exporters. Exact
/// mutable builtins use critical sections instead.
pub(super) struct BorrowedBuffer<'py> {
    view: ffi::Py_buffer,
    _python: std::marker::PhantomData<Python<'py>>,
}

impl BorrowedBuffer<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.view.len as usize
    }

    #[inline]
    unsafe fn bytes(&self) -> &[u8] {
        let data = if self.view.len == 0 {
            std::ptr::NonNull::<u8>::dangling().as_ptr()
        } else {
            self.view.buf.cast()
        };
        unsafe { std::slice::from_raw_parts(data, self.len()) }
    }
}

impl Drop for BorrowedBuffer<'_> {
    fn drop(&mut self) {
        unsafe { ffi::PyBuffer_Release(&mut self.view) };
    }
}

pub(super) fn with_bytearray<T>(value: &Bound<'_, PyByteArray>, callback: impl FnOnce() -> T) -> T {
    with_critical_section(value.as_any(), callback)
}

pub(super) enum BytesLike<'a, 'py> {
    Bytes(&'a [u8]),
    ByteArray(&'a Bound<'py, PyByteArray>),
    OwnedBytes(Bound<'py, PyBytes>),
    OwnedByteArray(Bound<'py, PyByteArray>),
    #[cfg_attr(Py_GIL_DISABLED, allow(dead_code))]
    Buffer(BorrowedBuffer<'py>),
    Text(&'a str),
    Owned(Vec<u8>),
}

impl BytesLike<'_, '_> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => bytes.len(),
            Self::ByteArray(value) => {
                with_critical_section(value.as_any(), || unsafe { bytearray_size(value.as_ptr()) })
            }
            Self::OwnedBytes(bytes) => unsafe { bytes_size(bytes.as_ptr()) },
            Self::OwnedByteArray(value) => {
                with_critical_section(value.as_any(), || unsafe { bytearray_size(value.as_ptr()) })
            }
            Self::Buffer(buffer) => buffer.len(),
            Self::Text(text) => text.len(),
            Self::Owned(bytes) => bytes.len(),
        }
    }

    pub(super) fn detach_safe(&self) -> bool {
        !matches!(
            self,
            Self::ByteArray(_) | Self::OwnedByteArray(_) | Self::Buffer(_)
        )
    }

    pub(super) fn requires_snapshot_for_output(&self) -> bool {
        matches!(self, Self::Buffer(_))
    }

    #[cfg(Py_GIL_DISABLED)]
    pub(super) fn snapshot_mutable(&self) -> Option<Vec<u8>> {
        match self {
            Self::ByteArray(value) => Some(with_critical_section(value.as_any(), || unsafe {
                bytearray_bytes(value).to_vec()
            })),
            Self::OwnedByteArray(value) => Some(with_critical_section(value.as_any(), || unsafe {
                bytearray_bytes(value).to_vec()
            })),
            _ => None,
        }
    }

    #[cfg(Py_GIL_DISABLED)]
    pub(super) fn into_stable(self) -> Self {
        match self {
            Self::ByteArray(value) => {
                Self::Owned(with_critical_section(value.as_any(), || unsafe {
                    bytearray_bytes(value).to_vec()
                }))
            }
            Self::OwnedByteArray(value) => {
                Self::Owned(with_critical_section(value.as_any(), || unsafe {
                    bytearray_bytes(&value).to_vec()
                }))
            }
            stable => stable,
        }
    }

    pub(super) fn aliases(&self, output: &Bound<'_, PyByteArray>) -> bool {
        matches!(
            self,
            Self::ByteArray(value) if value.as_ptr() == output.as_ptr()
        ) || matches!(
            self,
            Self::OwnedByteArray(value) if value.as_ptr() == output.as_ptr()
        )
    }

    /// The callback must not run arbitrary Python or release the GIL when the input is mutable.
    pub(super) unsafe fn with_bytes<T>(&self, callback: impl FnOnce(&[u8]) -> T) -> T {
        match self {
            Self::Bytes(bytes) => callback(bytes),
            Self::ByteArray(value) => with_critical_section(value.as_any(), || {
                callback(unsafe { bytearray_bytes(value) })
            }),
            Self::OwnedBytes(bytes) => callback(unsafe { bytes_slice(bytes) }),
            Self::OwnedByteArray(value) => with_critical_section(value.as_any(), || {
                callback(unsafe { bytearray_bytes(value) })
            }),
            Self::Buffer(buffer) => callback(unsafe { buffer.bytes() }),
            Self::Text(text) => callback(text.as_bytes()),
            Self::Owned(bytes) => callback(bytes),
        }
    }

    /// The callback must not run arbitrary Python or detach from the interpreter.
    /// The caller must ensure that self and output do not alias.
    pub(super) unsafe fn with_bytes_and_output<T>(
        &self,
        output: &Bound<'_, PyByteArray>,
        callback: impl FnOnce(&[u8], *mut u8, usize) -> T,
    ) -> T {
        match self {
            Self::ByteArray(value) => {
                with_critical_section2(value.as_any(), output.as_any(), || unsafe {
                    callback(
                        bytearray_bytes(value),
                        bytearray_data(output.as_ptr()),
                        bytearray_size(output.as_ptr()),
                    )
                })
            }
            Self::OwnedByteArray(value) => {
                with_critical_section2(value.as_any(), output.as_any(), || unsafe {
                    callback(
                        bytearray_bytes(value),
                        bytearray_data(output.as_ptr()),
                        bytearray_size(output.as_ptr()),
                    )
                })
            }
            _ => with_critical_section(output.as_any(), || unsafe {
                self.with_bytes(|input| {
                    callback(
                        input,
                        bytearray_data(output.as_ptr()),
                        bytearray_size(output.as_ptr()),
                    )
                })
            }),
        }
    }

    pub(super) fn stable_bytes(&self) -> &[u8] {
        match self {
            Self::Bytes(bytes) => bytes,
            Self::ByteArray(_) | Self::OwnedByteArray(_) => {
                unreachable!("mutable bytearrays must be snapshotted before borrowing")
            }
            Self::OwnedBytes(bytes) => bytes.as_bytes(),
            Self::Buffer(buffer) => unsafe { buffer.bytes() },
            Self::Text(text) => text.as_bytes(),
            Self::Owned(bytes) => bytes,
        }
    }
}

pub(super) fn bytes_like<'a, 'py>(
    _py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a, 'py>> {
    if let Some(bytes) = exact_bytes_like(value) {
        return Ok(bytes);
    }
    buffer_bytes_like(value, argument, false)
}

pub(super) fn contiguous_bytes_like<'a, 'py>(
    _py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a, 'py>> {
    if let Some(bytes) = exact_bytes_like(value) {
        return Ok(bytes);
    }
    buffer_bytes_like(value, argument, true)
}

pub(super) fn ascii_or_bytes<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a, 'py>> {
    if let Some(bytes) = exact_bytes_like(value) {
        return Ok(bytes);
    }
    if PyString::is_exact_type_of(value) {
        // The exact-type check above establishes the unchecked cast's invariant.
        let text = unsafe { value.cast_unchecked::<PyString>() };
        let text = text.to_str().map_err(|_| ascii_error(argument))?;
        if !text.is_ascii() {
            return Err(ascii_error(argument));
        }
        return Ok(BytesLike::Text(text));
    }
    if value.is_instance_of::<PyString>() {
        let encoded = value.call_method1(intern!(py, "encode"), ("ascii",))?;
        return buffer_bytes_like(&encoded, argument, false);
    }
    bytes_like(py, value, argument)
}

#[inline]
fn exact_bytes_like<'a, 'py>(value: &'a Bound<'py, PyAny>) -> Option<BytesLike<'a, 'py>> {
    if PyBytes::is_exact_type_of(value) {
        // Exact builtins cannot override their storage behavior.
        let bytes = unsafe { value.cast_unchecked::<PyBytes>() };
        return Some(BytesLike::Bytes(unsafe { bytes_slice(bytes) }));
    }
    if PyByteArray::is_exact_type_of(value) {
        let value = unsafe { value.cast_unchecked::<PyByteArray>() };
        return Some(BytesLike::ByteArray(value));
    }
    None
}

unsafe fn bytes_slice<'a>(value: &'a Bound<'_, PyBytes>) -> &'a [u8] {
    unsafe { std::slice::from_raw_parts(bytes_data(value.as_ptr()), bytes_size(value.as_ptr())) }
}

unsafe fn bytearray_bytes<'a>(value: &'a Bound<'_, PyByteArray>) -> &'a [u8] {
    unsafe {
        std::slice::from_raw_parts(
            bytearray_data(value.as_ptr()),
            bytearray_size(value.as_ptr()),
        )
    }
}

fn buffer_bytes_like<'a, 'py>(
    value: &Bound<'py, PyAny>,
    argument: &str,
    require_contiguous: bool,
) -> PyResult<BytesLike<'a, 'py>> {
    if PyMemoryView::is_exact_type_of(value) {
        let memoryview = unsafe { value.cast_unchecked::<PyMemoryView>() };
        return exact_memoryview_bytes_like(memoryview, require_contiguous);
    }
    if unsafe { ffi::PyObject_CheckBuffer(value.as_ptr()) } == 0 {
        return Err(type_error(argument));
    }
    // A memoryview owns the export acquired here. Subsequent contiguity checks
    // and borrowing operate on that view instead of invoking the exporter a
    // second time, and an exporter-defined exception remains intact.
    let memoryview = PyMemoryView::from(value)?;
    exact_memoryview_bytes_like(&memoryview, require_contiguous)
}

fn exact_memoryview_bytes_like<'a, 'py>(
    memoryview: &Bound<'py, PyMemoryView>,
    require_contiguous: bool,
) -> PyResult<BytesLike<'a, 'py>> {
    let py = memoryview.py();
    let nbytes = memoryview
        .getattr(intern!(py, "nbytes"))?
        .extract::<usize>()?;
    let try_owner = nbytes >= MEMORYVIEW_OWNER_THRESHOLD;
    let contiguous = if require_contiguous || try_owner || cfg!(not(Py_GIL_DISABLED)) {
        memoryview
            .getattr(intern!(py, "c_contiguous"))?
            .is_truthy()?
    } else {
        false
    };
    if require_contiguous && !contiguous {
        return Err(PyBufferError::new_err(
            "memoryview: underlying buffer is not C-contiguous",
        ));
    }
    if contiguous && try_owner {
        let owner = memoryview.getattr(intern!(py, "obj"))?;
        if PyBytes::is_exact_type_of(&owner) {
            let owner = owner.cast_into::<PyBytes>()?;
            if unsafe { bytes_size(owner.as_ptr()) } == nbytes {
                return Ok(BytesLike::OwnedBytes(owner));
            }
        } else if PyByteArray::is_exact_type_of(&owner) {
            let owner = owner.cast_into::<PyByteArray>()?;
            if with_bytearray(&owner, || unsafe { bytearray_size(owner.as_ptr()) }) == nbytes {
                return Ok(BytesLike::OwnedByteArray(owner));
            }
        }
    }
    #[cfg(not(Py_GIL_DISABLED))]
    if contiguous {
        return borrowed_contiguous_buffer(memoryview.as_any()).map(BytesLike::Buffer);
    }
    copy_memoryview(memoryview, false).map(BytesLike::OwnedBytes)
}

#[cfg(not(Py_GIL_DISABLED))]
fn borrowed_contiguous_buffer<'py>(value: &Bound<'py, PyAny>) -> PyResult<BorrowedBuffer<'py>> {
    let mut view = unsafe { std::mem::zeroed::<ffi::Py_buffer>() };
    if unsafe { ffi::PyObject_GetBuffer(value.as_ptr(), &mut view, ffi::PyBUF_CONTIG_RO) } == 0 {
        return Ok(BorrowedBuffer {
            view,
            _python: std::marker::PhantomData,
        });
    }
    Err(PyErr::fetch(value.py()))
}

fn copy_memoryview<'py>(
    memoryview: &Bound<'py, PyMemoryView>,
    require_contiguous: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let py = memoryview.py();
    if require_contiguous
        && !memoryview
            .getattr(intern!(py, "c_contiguous"))?
            .is_truthy()?
    {
        return Err(PyBufferError::new_err(
            "memoryview: underlying buffer is not C-contiguous",
        ));
    }
    memoryview
        .call_method0(intern!(py, "tobytes"))?
        .cast_into::<PyBytes>()
        .map_err(|_| PyTypeError::new_err("memoryview.tobytes() did not return bytes"))
}

fn type_error(argument: &str) -> PyErr {
    PyTypeError::new_err(format!("{argument} must be a bytes-like object"))
}

fn ascii_error(argument: &str) -> PyErr {
    PyValueError::new_err(format!("{argument} must contain only ASCII characters"))
}
