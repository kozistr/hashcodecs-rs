use pyo3::PyTypeInfo;
use pyo3::exceptions::PyMemoryError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyList;

pub(super) fn list_from_fn<'py, T>(
    py: Python<'py>,
    length: usize,
    mut item: impl FnMut(usize) -> PyResult<Bound<'py, T>>,
) -> PyResult<Bound<'py, PyList>>
where
    T: PyTypeInfo,
{
    let length = ffi::Py_ssize_t::try_from(length)
        .map_err(|_| PyMemoryError::new_err("Python list is too large"))?;
    unsafe {
        let list: Bound<'py, PyList> =
            Bound::from_owned_ptr_or_err(py, ffi::PyList_New(length))?.cast_into_unchecked();
        for index in 0..length {
            // PyList_SET_ITEM steals the new reference. The list is not
            // published until every slot has been initialized.
            ffi::PyList_SET_ITEM(list.as_ptr(), index, item(index as usize)?.into_ptr());
        }
        Ok(list)
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn exact_bytes_up_to(items: &Bound<'_, PyList>, max_length: usize) -> bool {
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr());
        (0..length).all(|index| {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index);
            ffi::PyBytes_CheckExact(item) != 0 && ffi::Py_SIZE(item) as usize <= max_length
        })
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn exact_small_bytes(items: &Bound<'_, PyList>, threshold: usize) -> bool {
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr());
        let mut total = 0_usize;
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index);
            if ffi::PyBytes_CheckExact(item) == 0 {
                return false;
            }
            total = total.saturating_add(ffi::Py_SIZE(item) as usize);
            if total >= threshold {
                return false;
            }
        }
        true
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) unsafe fn exact_bytes_at<'a>(items: &'a Bound<'_, PyList>, index: usize) -> &'a [u8] {
    unsafe {
        let item = ffi::PyList_GET_ITEM(items.as_ptr(), index as ffi::Py_ssize_t);
        debug_assert_ne!(ffi::PyBytes_CheckExact(item), 0);
        std::slice::from_raw_parts(bytes_data(item), ffi::Py_SIZE(item) as usize)
    }
}

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
