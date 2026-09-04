use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList};

use crate::bindings::arguments::seed_u64;
use crate::bindings::runtime::{
    XXH3_DETACH_THRESHOLD, return_function_result, with_function_bytes,
};
use crate::xxhash::{xxh3_64 as hash_64, xxh3_128 as hash_128};

use super::batch::{
    xxh3_64_batch as hash_64_batch, xxh3_64_batch_into as hash_64_batch_into,
    xxh3_128_batch as hash_128_batch, xxh3_128_batch_into as hash_128_batch_into,
};

pub(in crate::bindings) unsafe extern "C" fn xxh3_64(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        crate::bindings::schema::xxhash::xxh3_64(args, nargs, keywords, |py, input, seed| {
            let Some(seed) = seed_u64(seed.as_ptr()) else {
                return std::ptr::null_mut();
            };
            let result = with_function_bytes(py, input.as_ptr(), XXH3_DETACH_THRESHOLD, |bytes| {
                hash_64(bytes, seed)
            });
            return_function_result(
                py,
                result.map(|value| ffi::PyLong_FromUnsignedLongLong(value as _)),
            )
        })
    }
}

pub(in crate::bindings) unsafe extern "C" fn xxh3_128(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        crate::bindings::schema::xxhash::xxh3_128(args, nargs, keywords, |py, input, seed| {
            let Some(seed) = seed_u64(seed.as_ptr()) else {
                return std::ptr::null_mut();
            };
            let result = with_function_bytes(py, input.as_ptr(), XXH3_DETACH_THRESHOLD, |bytes| {
                hash_128(bytes, seed)
            });
            match result {
                Ok([low, high]) => {
                    PyInt::new(py, (u128::from(high) << 64) | u128::from(low)).into_ptr()
                }
                Err(error) => {
                    error.restore(py);
                    std::ptr::null_mut()
                }
            }
        })
    }
}

macro_rules! batch_callback {
    ($name:ident, $operation:ident) => {
        pub(in crate::bindings) unsafe extern "C" fn $name(
            _self: *mut ffi::PyObject,
            args: *const *mut ffi::PyObject,
            nargs: isize,
            keywords: *mut ffi::PyObject,
        ) -> *mut ffi::PyObject {
            unsafe {
                crate::bindings::schema::xxhash::$name(args, nargs, keywords, |py, items, seed| {
                    let result = (|| {
                        let seed = seed_u64(seed.as_ptr()).ok_or_else(|| PyErr::fetch(py))?;
                        let items = items.raw(py).cast::<PyList>()?;
                        $operation(py, items, seed)
                    })();
                    return_function_result(py, result.map(Bound::into_ptr))
                })
            }
        }
    };
}

batch_callback!(xxh3_64_batch, hash_64_batch);
batch_callback!(xxh3_128_batch, hash_128_batch);

macro_rules! batch_into_callback {
    ($name:ident, $operation:ident) => {
        pub(in crate::bindings) unsafe extern "C" fn $name(
            _self: *mut ffi::PyObject,
            args: *const *mut ffi::PyObject,
            nargs: isize,
            keywords: *mut ffi::PyObject,
        ) -> *mut ffi::PyObject {
            unsafe {
                crate::bindings::schema::xxhash::$name(
                    args,
                    nargs,
                    keywords,
                    |py, items, output, seed| {
                        let result = (|| {
                            let seed = seed_u64(seed.as_ptr()).ok_or_else(|| PyErr::fetch(py))?;
                            let items = items.raw(py).cast::<PyList>()?;
                            let output = output.raw(py).cast::<PyByteArray>()?;
                            $operation(py, items, output, seed)
                        })();
                        return_function_result(
                            py,
                            result.map(|written| PyInt::new(py, written).into_ptr()),
                        )
                    },
                )
            }
        }
    };
}

batch_into_callback!(xxh3_64_batch_into, hash_64_batch_into);
batch_into_callback!(xxh3_128_batch_into, hash_128_batch_into);
