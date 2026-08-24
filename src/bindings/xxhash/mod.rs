use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList};

use super::arguments::{parse_hash_arguments, parse_raw_arguments, seed_u64};
use super::runtime::{
    XXH3_DETACH_THRESHOLD, catch_unwind_callback, return_function_result, with_function_bytes,
};
use crate::xxhash::{xxh3_64, xxh3_128};

mod batch;
mod methods;
use batch::{xxh3_64_batch, xxh3_64_batch_into, xxh3_128_batch, xxh3_128_batch_into};

unsafe extern "C" fn xxh3_64_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"xxh3_64".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, XXH3_DETACH_THRESHOLD, |bytes| {
            xxh3_64(bytes, seed)
        });
        return_function_result(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLongLong(value as _)),
        )
    })
}

unsafe extern "C" fn xxh3_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"xxh3_128".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, XXH3_DETACH_THRESHOLD, |bytes| {
            xxh3_128(bytes, seed)
        });
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
    })
}

unsafe extern "C" fn xxh3_64_batch_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

unsafe extern "C" fn xxh3_128_batch_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

unsafe extern "C" fn xxh3_64_batch_into_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

unsafe extern "C" fn xxh3_128_batch_into_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) use methods::add_to_module;
