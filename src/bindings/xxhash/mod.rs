use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyInt;

use super::buffer::{BytesLike, bytes_like};
use super::{
    DETACH_THRESHOLD, METHOD_FLAGS, parse_function_arguments, return_function_result, seed_u64,
    with_function_bytes,
};
use crate::{xxh3_64, xxh3_128};

mod batch;
pub use batch::{xxh3_64_batch, xxh3_128_batch};

unsafe extern "C" fn xxh3_64_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_function_arguments(args, nargsf, keywords, c"xxh3_64".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| xxh3_64(bytes, seed));
        return_function_result(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLongLong(value as _)),
        )
    }
}

unsafe extern "C" fn xxh3_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_function_arguments(args, nargsf, keywords, c"xxh3_128".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| xxh3_128(bytes, seed));
        match result {
            Ok([low, high]) => {
                let value = (u128::from(high) << 64) | u128::from(low);
                PyInt::new(py, value).into_ptr()
            }
            Err(error) => {
                error.restore(py);
                ptr::null_mut()
            }
        }
    }
}

static mut METHODS: [ffi::PyMethodDef; 3] = [
    ffi::PyMethodDef {
        ml_name: c"xxh3_64".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_64($module, /, s, seed=0)\n--\n\n".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_128($module, /, s, seed=0)\n--\n\n".as_ptr(),
    },
    ffi::PyMethodDef::zeroed(),
];

pub(super) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    let result = unsafe { ffi::PyModule_AddFunctions(module.as_ptr(), methods) };
    if result == -1 {
        Err(PyErr::fetch(module.py()))
    } else {
        Ok(())
    }
}

fn parse_batch<'a, 'py>(
    py: Python<'py>,
    items: &'a [Bound<'py, PyAny>],
) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    items
        .iter()
        .map(|item| bytes_like(py, item, "items element"))
        .collect()
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= DETACH_THRESHOLD
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> Vec<&'a [u8]> {
    inputs
        .iter()
        .map(|input| unsafe { input.as_bytes() })
        .collect()
}
