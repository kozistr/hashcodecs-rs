use pyo3::PyTypeInfo;
use pyo3::exceptions::PyMemoryError;
use pyo3::ffi;
use pyo3::prelude::*;
#[cfg(not(Py_GIL_DISABLED))]
use pyo3::types::PyBytes;
use pyo3::types::PyList;

pub(super) fn batch_results<T>(length: usize, error: &'static str) -> PyResult<Vec<T>> {
    let mut results = Vec::new();
    results
        .try_reserve_exact(length)
        .map_err(|_| PyMemoryError::new_err(error))?;
    Ok(results)
}

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
            // PyList_SET_ITEM steals the new reference.
            // The function publishes the list after PyList_SET_ITEM initializes every slot.
            ffi::PyList_SET_ITEM(list.as_ptr(), index, item(index as usize)?.into_ptr());
        }
        Ok(list)
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn exact_bytes_up_to<'py>(
    items: &Bound<'py, PyList>,
    max_length: usize,
) -> PyResult<Option<Vec<Bound<'py, PyBytes>>>> {
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr());
        let mut values = Vec::new();
        values
            .try_reserve_exact(length as usize)
            .map_err(|_| PyMemoryError::new_err("Python list is too large"))?;
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index);
            if ffi::PyBytes_CheckExact(item) == 0 || ffi::Py_SIZE(item) as usize > max_length {
                return Ok(None);
            }
            values.push(Bound::from_borrowed_ptr(items.py(), item).cast_into_unchecked());
        }
        Ok(Some(values))
    }
}

#[cfg(not(Py_GIL_DISABLED))]
pub(super) fn exact_bytes_total(items: &Bound<'_, PyList>) -> Option<usize> {
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr());
        let mut total = 0_usize;
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index);
            if ffi::PyBytes_CheckExact(item) == 0 {
                return None;
            }
            total = total.saturating_add(ffi::Py_SIZE(item) as usize);
        }
        Some(total)
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

pub(super) fn list_items<'py>(items: &Bound<'py, PyList>) -> PyResult<Vec<Bound<'py, PyAny>>> {
    let length = items.len();
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| PyMemoryError::new_err("Python list is too large"))?;
    #[cfg(Py_GIL_DISABLED)]
    {
        values.extend(items.iter());
        Ok(values)
    }
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index as isize);
            values.push(Bound::from_borrowed_ptr(items.py(), item));
        }
        Ok(values)
    }
}

#[inline]
pub(super) unsafe fn bytearray_data(value: *mut ffi::PyObject) -> *mut u8 {
    #[cfg(Py_GIL_DISABLED)]
    let data: *mut u8 = unsafe { ffi::PyByteArray_AsString(value).cast() };
    #[cfg(not(Py_GIL_DISABLED))]
    let data: *mut u8 = unsafe { ffi::PyByteArray_AS_STRING(value).cast() };
    if data.is_null() {
        std::ptr::NonNull::<u8>::dangling().as_ptr()
    } else {
        data
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

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::PyByteArray;

    #[test]
    fn oversized_batch_capacity_is_an_error() {
        assert!(batch_results::<u8>(usize::MAX, "batch is too large").is_err());
    }

    #[test]
    fn empty_bytearray_has_a_valid_zero_length_rust_pointer() {
        Python::initialize();
        Python::attach(|py| {
            let value = PyByteArray::new(py, b"");
            let data = unsafe { bytearray_data(value.as_ptr()) };
            assert!(!data.is_null());
            assert_eq!(unsafe { bytearray_size(value.as_ptr()) }, 0);
            assert!(unsafe { std::slice::from_raw_parts_mut(data, 0) }.is_empty());
        });
    }
}
