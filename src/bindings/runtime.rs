use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use pyo3::ffi;
use pyo3::panic::PanicException;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::buffer::bytes_like;
use super::objects::{bytes_data, bytes_size};

pub(super) const BASE64_DETACH_THRESHOLD: usize = 256 * 1024;
pub(super) const MURMUR3_DETACH_THRESHOLD: usize = 64 * 1024;
pub(super) const XXH3_DETACH_THRESHOLD: usize = 256 * 1024;
pub(super) const METHOD_FLAGS: i32 = ffi::METH_FASTCALL | ffi::METH_KEYWORDS;

pub(super) fn with_function_bytes<T: Send>(
    py: Python<'_>,
    object: *mut ffi::PyObject,
    detach_threshold: usize,
    operation: impl FnOnce(&[u8]) -> T + Send,
) -> PyResult<T> {
    if unsafe { ffi::PyBytes_CheckExact(object) } != 0 {
        let length = unsafe { bytes_size(object) };
        let bytes = unsafe { std::slice::from_raw_parts(bytes_data(object), length) };
        return if length >= detach_threshold {
            Ok(py.detach(|| operation(bytes)))
        } else {
            Ok(operation(bytes))
        };
    }

    let object = unsafe { Bound::from_borrowed_ptr(py, object) };
    let input = bytes_like(&object, "s")?;
    let detach = input.detach_safe() && input.len() >= detach_threshold;

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

pub(super) fn catch_unwind_callback(
    py: Python<'_>,
    callback: impl FnOnce() -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    match catch_unwind(AssertUnwindSafe(callback)) {
        Ok(result) => result,
        Err(payload) => {
            let message = if let Some(message) = payload.downcast_ref::<String>() {
                message.clone()
            } else if let Some(message) = payload.downcast_ref::<&str>() {
                (*message).to_owned()
            } else {
                "panic from Rust code".to_owned()
            };
            PanicException::new_err(message).restore(py);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_panics_become_python_exceptions() {
        Python::initialize();
        Python::attach(|py| {
            let result = catch_unwind_callback(py, || panic!("callback panic"));
            assert!(result.is_null());
            assert!(PyErr::occurred(py));
            unsafe { ffi::PyErr_Clear() };
        });
    }
}
