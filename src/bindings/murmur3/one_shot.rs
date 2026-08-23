use std::ptr;

use pyo3::ffi;
use pyo3::prelude::Python;

use super::digest::{x64_128_digest, x86_128_digest};
use crate::bindings::arguments::{parse_hash_arguments, seed_u32};
use crate::bindings::runtime::{return_function_result, with_function_bytes};
use crate::murmur3::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128};

fn bytes_result(digest: &[u8]) -> *mut ffi::PyObject {
    unsafe { ffi::PyBytes_FromStringAndSize(digest.as_ptr().cast(), digest.len() as isize) }
}

pub(super) unsafe extern "C" fn murmur3_32(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"murmur3_32".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| murmur3_x86_32(bytes, seed));
        return_function_result(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLong(value as _)),
        )
    }
}

pub(super) unsafe extern "C" fn murmur3_x86_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_hash_arguments(args, nargsf, keywords, c"murmur3_x86_128_digest".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| {
            x86_128_digest(murmur3_x86_128(bytes, seed))
        });
        return_function_result(py, result.map(|digest| bytes_result(&digest)))
    }
}

pub(super) unsafe extern "C" fn murmur3_x64_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_hash_arguments(args, nargsf, keywords, c"murmur3_x64_128_digest".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| {
            x64_128_digest(murmur3_x64_128(bytes, seed))
        });
        return_function_result(py, result.map(|digest| bytes_result(&digest)))
    }
}
