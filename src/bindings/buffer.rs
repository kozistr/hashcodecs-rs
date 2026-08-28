use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyBufferError, PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::critical_section::{with_critical_section, with_critical_section2};
use pyo3::types::{PyByteArray, PyBytes, PyMemoryView, PyString};

use super::objects::{bytearray_data, bytearray_size, bytes_data, bytes_size};

#[cfg(all(
    not(hashcodecs_memoryview_shim),
    not(Py_LIMITED_API),
    not(any(PyPy, GraalPy))
))]
const MEMORYVIEW_OWNER_THRESHOLD: usize = 0;
#[cfg(all(not(hashcodecs_memoryview_shim), any(Py_LIMITED_API, PyPy, GraalPy)))]
const MEMORYVIEW_OWNER_THRESHOLD: usize = 4 * 1024;

#[cfg(hashcodecs_memoryview_shim)]
unsafe extern "C" {
    fn hashcodecs_memoryview_owner(memoryview: *mut ffi::PyObject) -> *mut ffi::PyObject;
}

struct MemoryViewInfo<'py> {
    nbytes: usize,
    c_contiguous: bool,
    data: *mut u8,
    owner: Option<Bound<'py, PyAny>>,
}

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
    Bytes(&'a Bound<'py, PyBytes>),
    ByteArray(&'a Bound<'py, PyByteArray>),
    OwnedBytes(Bound<'py, PyBytes>),
    OwnedByteArray(Bound<'py, PyByteArray>),
    #[cfg_attr(Py_GIL_DISABLED, allow(dead_code))]
    Buffer(BorrowedBuffer<'py>),
    Text(&'a str),
    Owned(Vec<u8>),
}

impl<'py> BytesLike<'_, 'py> {
    pub(super) fn len(&self) -> usize {
        match self {
            Self::Bytes(bytes) => unsafe { bytes_size(bytes.as_ptr()) },
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

    pub(super) fn python_bytes(&self) -> Option<&Bound<'py, PyBytes>> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::OwnedBytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    /// Make an input independent of batch destinations which overlap its storage.
    pub(super) fn into_stable_for_batch_outputs(
        self,
        outputs: &[Bound<'_, PyByteArray>],
    ) -> PyResult<Self> {
        if outputs.iter().any(|output| self.overlaps(output)) {
            return self.try_snapshot().map(Self::Owned);
        }
        Ok(self)
    }

    #[cfg(Py_GIL_DISABLED)]
    pub(super) fn snapshot_mutable(&self) -> PyResult<Option<Vec<u8>>> {
        if matches!(self, Self::ByteArray(_) | Self::OwnedByteArray(_)) {
            return self.try_snapshot().map(Some);
        }
        Ok(None)
    }

    #[cfg(Py_GIL_DISABLED)]
    pub(super) fn into_stable(self) -> PyResult<Self> {
        if matches!(self, Self::ByteArray(_) | Self::OwnedByteArray(_)) {
            return self.try_snapshot().map(Self::Owned);
        }
        Ok(self)
    }

    pub(super) fn overlaps(&self, output: &Bound<'_, PyByteArray>) -> bool {
        match self {
            Self::ByteArray(value) => value.as_ptr() == output.as_ptr(),
            Self::OwnedByteArray(value) => value.as_ptr() == output.as_ptr(),
            Self::Buffer(buffer) => with_bytearray(output, || unsafe {
                ranges_overlap(
                    buffer.view.buf.cast(),
                    buffer.len(),
                    bytearray_data(output.as_ptr()),
                    bytearray_size(output.as_ptr()),
                )
            }),
            Self::Bytes(_) | Self::OwnedBytes(_) | Self::Text(_) | Self::Owned(_) => false,
        }
    }

    pub(super) fn snapshot_for_output(
        &self,
        output: &Bound<'_, PyByteArray>,
    ) -> PyResult<Option<Vec<u8>>> {
        if self.overlaps(output) {
            return self.try_snapshot().map(Some);
        }
        Ok(None)
    }

    fn try_snapshot(&self) -> PyResult<Vec<u8>> {
        unsafe { self.with_bytes(try_copy_bytes) }
    }

    /// The callback must not run arbitrary Python or release the GIL when the input is mutable.
    pub(super) unsafe fn with_bytes<T>(&self, callback: impl FnOnce(&[u8]) -> T) -> T {
        match self {
            Self::Bytes(bytes) => callback(unsafe { bytes_slice(bytes) }),
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
            Self::Bytes(bytes) => unsafe { bytes_slice(bytes) },
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

fn try_copy_bytes(bytes: &[u8]) -> PyResult<Vec<u8>> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(|_| PyMemoryError::new_err("bytes-like input is too large to snapshot"))?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

fn ranges_overlap(
    first: *const u8,
    first_len: usize,
    second: *const u8,
    second_len: usize,
) -> bool {
    if first_len == 0 || second_len == 0 {
        return false;
    }
    let first_start = first as usize;
    let second_start = second as usize;
    let first_end = first_start.saturating_add(first_len);
    let second_end = second_start.saturating_add(second_len);
    first_start < second_end && second_start < first_end
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

pub(super) fn contiguous_bytes_like_owned<'py>(
    value: &Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'py, 'py>> {
    if PyByteArray::is_exact_type_of(value) {
        return value
            .clone()
            .cast_into::<PyByteArray>()
            .map(BytesLike::OwnedByteArray)
            .map_err(Into::into);
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

pub(super) fn ascii_or_bytes_owned<'py>(
    value: &Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'py, 'py>> {
    if PyByteArray::is_exact_type_of(value) {
        return value
            .clone()
            .cast_into::<PyByteArray>()
            .map(BytesLike::OwnedByteArray)
            .map_err(Into::into);
    }
    buffer_bytes_like(value, argument, false)
}

#[inline]
fn exact_bytes_like<'a, 'py>(value: &'a Bound<'py, PyAny>) -> Option<BytesLike<'a, 'py>> {
    if PyBytes::is_exact_type_of(value) {
        // Exact builtins cannot override their storage behavior.
        let bytes = unsafe { value.cast_unchecked::<PyBytes>() };
        return Some(BytesLike::Bytes(bytes));
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

fn buffer_bytes_like<'py>(
    value: &Bound<'py, PyAny>,
    argument: &str,
    require_contiguous: bool,
) -> PyResult<BytesLike<'py, 'py>> {
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

fn exact_memoryview_bytes_like<'py>(
    memoryview: &Bound<'py, PyMemoryView>,
    require_contiguous: bool,
) -> PyResult<BytesLike<'py, 'py>> {
    let info = memoryview_info(memoryview, require_contiguous)?;

    if require_contiguous && !info.c_contiguous {
        return Err(PyBufferError::new_err(
            "memoryview: underlying buffer is not C-contiguous",
        ));
    }

    if let Some(owner) = info.owner {
        if PyBytes::is_exact_type_of(&owner) {
            let owner = owner.cast_into::<PyBytes>()?;
            if unsafe {
                bytes_size(owner.as_ptr()) == info.nbytes
                    && bytes_data(owner.as_ptr()) == info.data.cast_const()
            } {
                return Ok(BytesLike::OwnedBytes(owner));
            }
        } else if PyByteArray::is_exact_type_of(&owner) {
            let owner = owner.cast_into::<PyByteArray>()?;
            if with_bytearray(&owner, || unsafe {
                bytearray_size(owner.as_ptr()) == info.nbytes
                    && bytearray_data(owner.as_ptr()) == info.data
            }) {
                return Ok(BytesLike::OwnedByteArray(owner));
            }
        }
    }

    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(buffer) = borrowed_contiguous_buffer(memoryview.as_any()) {
        return Ok(BytesLike::Buffer(buffer));
    }

    copy_memoryview(memoryview).map(BytesLike::OwnedBytes)
}

#[cfg(hashcodecs_memoryview_shim)]
fn memoryview_info<'py>(
    memoryview: &Bound<'py, PyMemoryView>,
    _require_contiguous: bool,
) -> PyResult<MemoryViewInfo<'py>> {
    with_critical_section(memoryview.as_any(), || unsafe {
        let mut view = std::mem::zeroed::<ffi::Py_buffer>();
        if ffi::PyObject_GetBuffer(memoryview.as_ptr(), &raw mut view, ffi::PyBUF_FULL_RO) != 0 {
            return Err(PyErr::fetch(memoryview.py()));
        }
        let nbytes = view.len as usize;
        let data = view.buf.cast();
        let c_contiguous =
            ffi::PyBuffer_IsContiguous(&raw const view, b'C' as std::ffi::c_char) != 0;
        let owner = hashcodecs_memoryview_owner(memoryview.as_ptr());
        let owner = if c_contiguous
            && !owner.is_null()
            && (ffi::PyBytes_CheckExact(owner) != 0 || ffi::PyByteArray_CheckExact(owner) != 0)
        {
            Some(Bound::from_borrowed_ptr(memoryview.py(), owner))
        } else {
            None
        };
        ffi::PyBuffer_Release(&raw mut view);
        Ok(MemoryViewInfo {
            nbytes,
            c_contiguous,
            data,
            owner,
        })
    })
}

#[cfg(not(hashcodecs_memoryview_shim))]
fn memoryview_info<'py>(
    memoryview: &Bound<'py, PyMemoryView>,
    require_contiguous: bool,
) -> PyResult<MemoryViewInfo<'py>> {
    let py = memoryview.py();
    let nbytes = memoryview
        .getattr(intern!(py, "nbytes"))?
        .extract::<usize>()?;
    let try_owner = nbytes >= MEMORYVIEW_OWNER_THRESHOLD;
    let c_contiguous = if require_contiguous || try_owner || cfg!(not(Py_GIL_DISABLED)) {
        memoryview
            .getattr(intern!(py, "c_contiguous"))?
            .is_truthy()?
    } else {
        false
    };
    let owner = if c_contiguous && try_owner {
        Some(memoryview.getattr(intern!(py, "obj"))?)
    } else {
        None
    };
    let data = if owner.is_some() {
        let mut view = unsafe { std::mem::zeroed::<ffi::Py_buffer>() };
        if unsafe { ffi::PyObject_GetBuffer(memoryview.as_ptr(), &mut view, ffi::PyBUF_CONTIG_RO) }
            != 0
        {
            return Err(PyErr::fetch(py));
        }
        let data = view.buf.cast();
        unsafe { ffi::PyBuffer_Release(&mut view) };
        data
    } else {
        std::ptr::null_mut()
    };
    Ok(MemoryViewInfo {
        nbytes,
        c_contiguous,
        data,
        owner,
    })
}

#[cfg(not(Py_GIL_DISABLED))]
fn borrowed_contiguous_buffer<'py>(value: &Bound<'py, PyAny>) -> Option<BorrowedBuffer<'py>> {
    let mut view = unsafe { std::mem::zeroed::<ffi::Py_buffer>() };

    if unsafe { ffi::PyObject_GetBuffer(value.as_ptr(), &mut view, ffi::PyBUF_CONTIG_RO) } == 0 {
        return Some(BorrowedBuffer {
            view,
            _python: std::marker::PhantomData,
        });
    }

    unsafe { ffi::PyErr_Clear() };
    None
}

fn copy_memoryview<'py>(memoryview: &Bound<'py, PyMemoryView>) -> PyResult<Bound<'py, PyBytes>> {
    let py = memoryview.py();
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

#[cfg(all(test, hashcodecs_memoryview_shim))]
mod tests {
    use super::*;

    #[test]
    fn cpython_memoryview_shim_reports_public_metadata() {
        Python::initialize();
        Python::attach(|py| {
            let owner = PyBytes::new(py, b"abcdef");
            let memoryview = PyMemoryView::from(owner.as_any()).unwrap();
            let info = memoryview_info(&memoryview, false).unwrap();
            assert_eq!(info.nbytes, 6);
            assert!(info.c_contiguous);
            assert!(info.owner.unwrap().is(&owner));

            let noncontiguous = py
                .eval(c"memoryview(b'abcdef')[::2]", None, None)
                .unwrap()
                .cast_into::<PyMemoryView>()
                .unwrap();
            let info = memoryview_info(&noncontiguous, false).unwrap();
            assert_eq!(info.nbytes, 3);
            assert!(!info.c_contiguous);
            assert!(info.owner.is_none());

            memoryview.call_method0(intern!(py, "release")).unwrap();
            let error = memoryview_info(&memoryview, false).err().unwrap();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert_eq!(
                error.to_string(),
                "ValueError: operation forbidden on released memoryview object"
            );
        });
    }
}
