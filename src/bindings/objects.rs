use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyList;

pub(super) fn list_items<'py>(items: &Bound<'py, PyList>) -> Vec<Bound<'py, PyAny>> {
    #[cfg(Py_GIL_DISABLED)]
    return items.iter().collect();
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr()) as usize;
        let mut values = Vec::with_capacity(length);
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index as isize);
            values.push(Bound::from_borrowed_ptr(items.py(), item));
        }
        values
    }
}

#[inline]
pub(super) unsafe fn bytearray_data(value: *mut ffi::PyObject) -> *mut u8 {
    #[cfg(Py_GIL_DISABLED)]
    return unsafe { ffi::PyByteArray_AsString(value).cast() };
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        ffi::PyByteArray_AS_STRING(value).cast()
    }
}

#[inline]
pub(super) unsafe fn bytearray_size(value: *mut ffi::PyObject) -> usize {
    #[cfg(Py_GIL_DISABLED)]
    return unsafe { ffi::PyByteArray_Size(value) as usize };
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        ffi::PyByteArray_GET_SIZE(value) as usize
    }
}

#[inline]
pub(super) unsafe fn bytes_data(value: *mut ffi::PyObject) -> *const u8 {
    unsafe { ffi::PyBytes_AS_STRING(value).cast() }
}

#[inline]
pub(super) unsafe fn bytes_data_mut(value: *mut ffi::PyObject) -> *mut u8 {
    unsafe { bytes_data(value).cast_mut() }
}

#[inline]
pub(super) unsafe fn bytes_size(value: *mut ffi::PyObject) -> usize {
    unsafe { ffi::Py_SIZE(value) as usize }
}
