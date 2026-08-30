use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyBufferError, PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::critical_section::{with_critical_section, with_critical_section2};
use pyo3::types::{PyByteArray, PyBytes, PyMemoryView, PyString};

use super::objects::{bytearray_data, bytearray_size, bytes_data, bytes_size};

#[cfg(not(Py_GIL_DISABLED))]
// Below the first detach threshold, retaining the acquired buffer avoids a
// Python attribute lookup without preventing any operation from detaching.
const MEMORYVIEW_OWNER_THRESHOLD: usize = 64 * 1024;
#[cfg(all(Py_GIL_DISABLED, any(Py_LIMITED_API, PyPy, GraalPy)))]
const MEMORYVIEW_OWNER_THRESHOLD: usize = 4 * 1024;

struct MemoryViewInfo<'py> {
    nbytes: usize,
    c_contiguous: bool,
    data: *mut u8,
    owner: Option<Bound<'py, PyAny>>,
    buffer: BorrowedBuffer<'py>,
}

/// Holds an acquired buffer and releases its export when the value drops.
///
/// The binding processes this buffer without a copy if it is C-contiguous and the build uses the GIL.
/// Another handle can mutate storage behind a read-only view. The GIL synchronizes access for unknown exporters.
/// Critical sections synchronize exact mutable builtins.
pub(super) struct BorrowedBuffer<'py> {
    view: ffi::Py_buffer,
    memoryview_source: *mut ffi::PyObject,
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

#[derive(Clone, Copy)]
pub(super) struct BufferRange {
    start: usize,
    end: usize,
}

impl BufferRange {
    fn new(data: *const u8, length: usize) -> Option<Self> {
        if length == 0 {
            return None;
        }
        let start = data as usize;
        Some(Self {
            start,
            end: start.saturating_add(length),
        })
    }

    pub(super) fn for_bytearray(value: &Bound<'_, PyByteArray>) -> Option<Self> {
        with_bytearray(value, || unsafe {
            Self::new(
                bytearray_data(value.as_ptr()),
                bytearray_size(value.as_ptr()),
            )
        })
    }

    pub(super) fn start(self) -> usize {
        self.start
    }

    pub(super) fn end(self) -> usize {
        self.end
    }

    pub(super) fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
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

    pub(super) fn python_bytes(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyBytes>>> {
        match self {
            Self::Bytes(bytes) => Ok(Some((*bytes).clone())),
            Self::OwnedBytes(bytes) => Ok(Some(bytes.clone())),
            Self::Buffer(buffer)
                if !buffer.memoryview_source.is_null()
                    && buffer.view.obj == buffer.memoryview_source =>
            {
                // The active export owns `view.obj`.
                // Equality proves that the exact memoryview source remains alive during access.
                let memoryview = unsafe { Bound::from_borrowed_ptr(py, buffer.memoryview_source) };
                let owner = memoryview.getattr(intern!(py, "obj"))?;
                if !PyBytes::is_exact_type_of(&owner) {
                    return Ok(None);
                }
                let owner = owner.cast_into::<PyBytes>()?;
                if unsafe {
                    bytes_size(owner.as_ptr()) == buffer.len()
                        && bytes_data(owner.as_ptr()) == buffer.view.buf.cast_const().cast()
                } {
                    Ok(Some(owner))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    pub(super) fn bytearray_identity(&self) -> Option<*mut ffi::PyObject> {
        match self {
            Self::ByteArray(value) => Some(value.as_ptr()),
            Self::OwnedByteArray(value) => Some(value.as_ptr()),
            _ => None,
        }
    }

    pub(super) fn buffer_range(&self) -> Option<BufferRange> {
        match self {
            Self::Buffer(buffer) => BufferRange::new(buffer.view.buf.cast(), buffer.len()),
            _ => None,
        }
    }

    pub(super) fn into_snapshot(self) -> PyResult<Self> {
        self.into_snapshot_if(true)
    }

    #[cfg(Py_GIL_DISABLED)]
    pub(super) fn snapshot_mutable(&self) -> PyResult<Option<Vec<u8>>> {
        self.snapshot_if(self.is_mutable_bytearray())
    }

    #[cfg(Py_GIL_DISABLED)]
    pub(super) fn into_stable(self) -> PyResult<Self> {
        let needs_snapshot = self.is_mutable_bytearray();
        self.into_snapshot_if(needs_snapshot)
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
        self.snapshot_if(self.overlaps(output))
    }

    #[cfg(Py_GIL_DISABLED)]
    fn is_mutable_bytearray(&self) -> bool {
        matches!(self, Self::ByteArray(_) | Self::OwnedByteArray(_))
    }

    fn snapshot_if(&self, needed: bool) -> PyResult<Option<Vec<u8>>> {
        needed.then(|| self.try_snapshot()).transpose()
    }

    fn into_snapshot_if(self, needed: bool) -> PyResult<Self> {
        match self.snapshot_if(needed)? {
            Some(snapshot) => Ok(Self::Owned(snapshot)),
            None => Ok(self),
        }
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
            #[cfg(not(Py_GIL_DISABLED))]
            Self::ByteArray(value) => unsafe { bytearray_bytes(value) },
            #[cfg(not(Py_GIL_DISABLED))]
            Self::OwnedByteArray(value) => unsafe { bytearray_bytes(value) },
            #[cfg(Py_GIL_DISABLED)]
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
    if value.is_instance_of::<PyString>() {
        let py = value.py();
        let encoded = value.call_method1(intern!(py, "encode"), ("ascii",))?;
        return buffer_bytes_like(&encoded, argument, false);
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
    with_critical_section(value, || {
        let buffer = acquire_buffer(value, std::ptr::null_mut())?;
        let c_contiguous = unsafe {
            ffi::PyBuffer_IsContiguous(&raw const buffer.view, b'C' as std::ffi::c_char) != 0
        };
        if require_contiguous && !c_contiguous {
            return Err(PyBufferError::new_err(
                "memoryview: underlying buffer is not C-contiguous",
            ));
        }

        #[cfg(not(Py_GIL_DISABLED))]
        if c_contiguous {
            return Ok(BytesLike::Buffer(buffer));
        }

        copy_buffer(value.py(), &buffer).map(BytesLike::OwnedBytes)
    })
}

fn exact_memoryview_bytes_like<'py>(
    memoryview: &Bound<'py, PyMemoryView>,
    require_contiguous: bool,
) -> PyResult<BytesLike<'py, 'py>> {
    let MemoryViewInfo {
        nbytes,
        c_contiguous,
        data,
        owner,
        buffer,
    } = memoryview_info(memoryview)?;

    if require_contiguous && !c_contiguous {
        return Err(PyBufferError::new_err(
            "memoryview: underlying buffer is not C-contiguous",
        ));
    }

    if let Some(owner) = owner {
        if PyBytes::is_exact_type_of(&owner) {
            let owner = owner.cast_into::<PyBytes>()?;
            if unsafe {
                bytes_size(owner.as_ptr()) == nbytes
                    && bytes_data(owner.as_ptr()) == data.cast_const()
            } {
                return Ok(BytesLike::OwnedBytes(owner));
            }
        } else if PyByteArray::is_exact_type_of(&owner) {
            let owner = owner.cast_into::<PyByteArray>()?;
            if with_bytearray(&owner, || unsafe {
                bytearray_size(owner.as_ptr()) == nbytes && bytearray_data(owner.as_ptr()) == data
            }) {
                return Ok(BytesLike::OwnedByteArray(owner));
            }
        }
    }

    #[cfg(not(Py_GIL_DISABLED))]
    if c_contiguous {
        return Ok(BytesLike::Buffer(buffer));
    }

    drop(buffer);
    copy_memoryview(memoryview).map(BytesLike::OwnedBytes)
}

fn memoryview_info<'py>(memoryview: &Bound<'py, PyMemoryView>) -> PyResult<MemoryViewInfo<'py>> {
    let py = memoryview.py();
    let buffer = acquire_buffer(memoryview.as_any(), memoryview.as_ptr())?;
    let nbytes = buffer.len();
    let data = buffer.view.buf.cast();
    let c_contiguous = unsafe {
        ffi::PyBuffer_IsContiguous(&raw const buffer.view, b'C' as std::ffi::c_char) != 0
    };
    #[cfg(not(Py_GIL_DISABLED))]
    let try_owner = c_contiguous && nbytes >= MEMORYVIEW_OWNER_THRESHOLD;
    #[cfg(all(Py_GIL_DISABLED, not(any(Py_LIMITED_API, PyPy, GraalPy))))]
    let try_owner = c_contiguous;
    #[cfg(all(Py_GIL_DISABLED, any(Py_LIMITED_API, PyPy, GraalPy)))]
    let try_owner = c_contiguous && nbytes >= MEMORYVIEW_OWNER_THRESHOLD;
    let owner = if try_owner {
        Some(memoryview.getattr(intern!(py, "obj"))?)
    } else {
        None
    };

    Ok(MemoryViewInfo {
        nbytes,
        c_contiguous,
        data,
        owner,
        buffer,
    })
}

fn acquire_buffer<'py>(
    value: &Bound<'py, PyAny>,
    memoryview_source: *mut ffi::PyObject,
) -> PyResult<BorrowedBuffer<'py>> {
    let py = value.py();
    let mut view = unsafe { std::mem::zeroed::<ffi::Py_buffer>() };
    if unsafe { ffi::PyObject_GetBuffer(value.as_ptr(), &raw mut view, ffi::PyBUF_FULL_RO) } != 0 {
        return Err(PyErr::fetch(py));
    }

    Ok(BorrowedBuffer {
        view,
        memoryview_source,
        _python: std::marker::PhantomData,
    })
}

fn copy_buffer<'py>(py: Python<'py>, buffer: &BorrowedBuffer<'_>) -> PyResult<Bound<'py, PyBytes>> {
    PyBytes::new_with(py, buffer.len(), |bytes| {
        let result = unsafe {
            ffi::PyBuffer_ToContiguous(
                bytes.as_mut_ptr().cast(),
                #[cfg(Py_3_11)]
                &raw const buffer.view,
                #[cfg(not(Py_3_11))]
                (&raw const buffer.view).cast_mut(),
                buffer.view.len,
                b'C' as std::ffi::c_char,
            )
        };
        if result != 0 {
            return Err(PyErr::fetch(py));
        }
        Ok(())
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memoryview_info_reports_public_metadata() {
        Python::initialize();
        Python::attach(|py| {
            let owner_data = vec![b'a'; 64 * 1024];
            let owner = PyBytes::new(py, &owner_data);
            let memoryview = PyMemoryView::from(owner.as_any()).unwrap();
            let info = memoryview_info(&memoryview).unwrap();
            assert_eq!(info.nbytes, owner_data.len());
            assert!(info.c_contiguous);
            assert!(info.owner.as_ref().unwrap().is(&owner));
            drop(info);

            let noncontiguous = py
                .eval(c"memoryview(b'abcdef')[::2]", None, None)
                .unwrap()
                .cast_into::<PyMemoryView>()
                .unwrap();
            let info = memoryview_info(&noncontiguous).unwrap();
            assert_eq!(info.nbytes, 3);
            assert!(!info.c_contiguous);
            assert!(info.owner.is_none());
            drop(info);

            memoryview.call_method0(intern!(py, "release")).unwrap();
            let error = memoryview_info(&memoryview).err().unwrap();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert_eq!(
                error.to_string(),
                "ValueError: operation forbidden on released memoryview object"
            );
        });
    }
}
