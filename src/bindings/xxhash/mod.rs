use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList};

use super::buffer::{BytesLike, bytes_like};
use super::{
    DETACH_THRESHOLD, METHOD_FLAGS, parse_hash_arguments, parse_raw_arguments,
    return_function_result, seed_u64, with_function_bytes,
};
use crate::{xxh3_64, xxh3_128};

mod batch;
use batch::{xxh3_64_batch, xxh3_64_batch_into, xxh3_128_batch, xxh3_128_batch_into};

unsafe extern "C" fn xxh3_64_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"xxh3_64".as_ptr())
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
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"xxh3_128".as_ptr())
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

unsafe extern "C" fn xxh3_64_batch_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some([items, seed]) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"xxh3_64_batch".as_ptr(),
            [c"items".as_ptr(), c"seed".as_ptr()],
            2,
            1,
        ) else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(seed) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = Bound::from_borrowed_ptr(py, items).cast_into::<PyList>()?;
            xxh3_64_batch(py, &items, seed)
        })();
        return_function_result(py, result.map(Bound::into_ptr))
    }
}

unsafe extern "C" fn xxh3_128_batch_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some([items, seed]) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"xxh3_128_batch".as_ptr(),
            [c"items".as_ptr(), c"seed".as_ptr()],
            2,
            1,
        ) else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(seed) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = Bound::from_borrowed_ptr(py, items).cast_into::<PyList>()?;
            xxh3_128_batch(py, &items, seed)
        })();
        return_function_result(py, result.map(Bound::into_ptr))
    }
}

unsafe extern "C" fn xxh3_64_batch_into_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some([items, output, seed]) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"xxh3_64_batch_into".as_ptr(),
            [c"items".as_ptr(), c"output".as_ptr(), c"seed".as_ptr()],
            3,
            2,
        ) else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(seed) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = Bound::from_borrowed_ptr(py, items).cast_into::<PyList>()?;
            let output = Bound::from_borrowed_ptr(py, output).cast_into::<PyByteArray>()?;
            xxh3_64_batch_into(py, &items, &output, seed)
        })();
        return_function_result(py, result.map(|written| PyInt::new(py, written).into_ptr()))
    }
}

unsafe extern "C" fn xxh3_128_batch_into_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some([items, output, seed]) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"xxh3_128_batch_into".as_ptr(),
            [c"items".as_ptr(), c"output".as_ptr(), c"seed".as_ptr()],
            3,
            2,
        ) else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(seed) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = Bound::from_borrowed_ptr(py, items).cast_into::<PyList>()?;
            let output = Bound::from_borrowed_ptr(py, output).cast_into::<PyByteArray>()?;
            xxh3_128_batch_into(py, &items, &output, seed)
        })();
        return_function_result(py, result.map(|written| PyInt::new(py, written).into_ptr()))
    }
}

static mut METHODS: [ffi::PyMethodDef; 7] = [
    ffi::PyMethodDef {
        ml_name: c"xxh3_64".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_64($module, /, s, seed=0)\n--\n\nReturn the canonical unsigned 64-bit XXH3 hash of a bytes-like object.\n\nseed must be an unsigned 64-bit integer. XXH3 is a non-cryptographic hash.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_128($module, /, s, seed=0)\n--\n\nReturn the canonical unsigned 128-bit XXH3 hash of a bytes-like object.\n\nseed must be an unsigned 64-bit integer. XXH3 is a non-cryptographic hash.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_batch_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_64_batch($module, /, items, seed=0)\n--\n\nReturn canonical unsigned 64-bit XXH3 hashes for a list of bytes-like objects.\n\nResults preserve input order. seed must be an unsigned 64-bit integer.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_batch_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_128_batch($module, /, items, seed=0)\n--\n\nReturn canonical unsigned 128-bit XXH3 hashes for a list of bytes-like objects.\n\nResults preserve input order. seed must be an unsigned 64-bit integer.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_batch_into_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_64_batch_into($module, /, items, output, seed=0)\n--\n\nWrite 64-bit XXH3 hashes into a reusable bytearray.\n\nEach result occupies 8 little-endian bytes. output must have space for every result. The operation validates all inputs before mutating output and returns the total bytes written.".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_batch_into_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: c"xxh3_128_batch_into($module, /, items, output, seed=0)\n--\n\nWrite 128-bit XXH3 hashes into a reusable bytearray.\n\nEach result occupies 16 little-endian bytes. output must have space for every result. The operation validates all inputs before mutating output and returns the total bytes written.".as_ptr(),
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
