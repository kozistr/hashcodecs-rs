use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::buffer::bytes_like;
use super::objects::{bytes_data, bytes_size};

pub(super) const DETACH_THRESHOLD: usize = 64 * 1024;
pub(super) const METHOD_FLAGS: i32 = ffi::METH_FASTCALL | ffi::METH_KEYWORDS;

pub(super) fn with_function_bytes<T: Send>(
    py: Python<'_>,
    object: *mut ffi::PyObject,
    operation: impl FnOnce(&[u8]) -> T + Send,
) -> PyResult<T> {
    if unsafe { ffi::PyBytes_CheckExact(object) } != 0 {
        let length = unsafe { bytes_size(object) };
        let bytes = unsafe { std::slice::from_raw_parts(bytes_data(object), length) };
        return if length >= DETACH_THRESHOLD {
            Ok(py.detach(|| operation(bytes)))
        } else {
            Ok(operation(bytes))
        };
    }
    let object = unsafe { Bound::from_borrowed_ptr(py, object) };
    let input = bytes_like(py, &object, "s")?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    Ok(unsafe {
        input.with_bytes(|bytes| {
            if detach {
                py.detach(|| operation(bytes))
            } else {
                operation(bytes)
            }
        })
    })
}

pub(super) unsafe fn return_function_result(
    py: Python<'_>,
    result: PyResult<*mut ffi::PyObject>,
) -> *mut ffi::PyObject {
    match result {
        Ok(value) => value,
        Err(error) => {
            error.restore(py);
            ptr::null_mut()
        }
    }
}

pub(super) unsafe fn add_methods(
    module: &Bound<'_, PyModule>,
    methods: *mut ffi::PyMethodDef,
) -> PyResult<()> {
    if unsafe { ffi::PyModule_AddFunctions(module.as_ptr(), methods) } == -1 {
        Err(PyErr::fetch(module.py()))
    } else {
        Ok(())
    }
}
