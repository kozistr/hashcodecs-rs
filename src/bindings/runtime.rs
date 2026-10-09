use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Once;

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

pub(super) fn return_function_result(
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
    init: &Once,
    register: unsafe fn(*mut ffi::PyMethodDef, (u8, u8)),
) -> PyResult<()> {
    init.call_once(|| {
        let version = module.py().version_info();
        unsafe { register(methods, (version.major, version.minor)) };
    });

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
    fn convert_callback_panics() {
        Python::initialize();
        Python::attach(|py| {
            for (payload, expected) in [
                (0, "callback panic"),
                (1, "owned panic"),
                (2, "panic from Rust code"),
            ] {
                let result = catch_unwind_callback(py, || match payload {
                    0 => panic!("callback panic"),
                    1 => std::panic::panic_any(String::from("owned panic")),
                    _ => std::panic::panic_any(42_u8),
                });
                assert!(result.is_null());
                let mut error_type = ptr::null_mut();
                let mut error_value = ptr::null_mut();
                let mut traceback = ptr::null_mut();
                // PyErr::fetch resumes PanicException; inspect and clear the C error instead.
                #[allow(deprecated)]
                unsafe {
                    ffi::PyErr_Fetch(
                        &raw mut error_type,
                        &raw mut error_value,
                        &raw mut traceback,
                    );
                }
                let error_type = unsafe { Bound::<PyAny>::from_owned_ptr(py, error_type) };
                let error_value = unsafe { Bound::<PyAny>::from_owned_ptr(py, error_value) };
                let _traceback = unsafe { Bound::<PyAny>::from_owned_ptr_or_opt(py, traceback) };
                assert!(error_type.is(py.get_type::<PanicException>()));
                assert_eq!(error_value.str().unwrap().to_str().unwrap(), expected);
                assert!(!PyErr::occurred(py));
            }
        });
    }

    #[test]
    fn detach_retained_views() {
        Python::initialize();
        Python::attach(|py| {
            let view = py
                .eval(c"memoryview(b'x' + b'a' * 65536 + b'y')[1:-1]", None, None)
                .unwrap();
            let result = with_function_bytes(py, view.as_ptr(), 1, |input| input.to_vec()).unwrap();
            assert_eq!(result, vec![b'a'; 65536]);
        });
    }

    #[test]
    fn reject_invalid_methods() {
        use crate::bindings::schema::base64::{BINDING_COUNT, register_all};
        use pyo3::exceptions::PyValueError;

        Python::initialize();
        Python::attach(|py| {
            let mut methods = [const { ffi::PyMethodDef::zeroed() }; BINDING_COUNT + 1];
            let init = Once::new();
            init.call_once(|| unsafe {
                register_all(methods.as_mut_ptr(), (3, 15));
            });
            methods[0].ml_flags |= ffi::METH_CLASS;
            let module = PyModule::new(py, "invalid_methods").unwrap();
            let error = unsafe { add_methods(&module, methods.as_mut_ptr(), &init, register_all) }
                .unwrap_err();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(!PyErr::occurred(py));
            assert!(!module.hasattr("b64encode").unwrap());
        });
    }
}
