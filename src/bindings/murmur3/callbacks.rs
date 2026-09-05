use pyo3::ffi;

use super::digest::{x64_128_digest, x86_128_digest};
use crate::bindings::arguments::seed_u32;
use crate::bindings::runtime::{
    MURMUR3_DETACH_THRESHOLD, return_function_result, with_function_bytes,
};
use crate::murmur3::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128};

fn bytes_result(digest: &[u8]) -> *mut ffi::PyObject {
    unsafe { ffi::PyBytes_FromStringAndSize(digest.as_ptr().cast(), digest.len() as isize) }
}

pub(in crate::bindings) unsafe extern "C" fn murmur3_32(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        crate::bindings::schema::murmur3::murmur3_32(args, nargs, keywords, |py, input, seed| {
            let Some(seed) = seed_u32(seed.as_ptr()) else {
                return std::ptr::null_mut();
            };
            let result =
                with_function_bytes(py, input.as_ptr(), MURMUR3_DETACH_THRESHOLD, |bytes| {
                    murmur3_x86_32(bytes, seed)
                });
            return_function_result(
                py,
                result.map(|value| ffi::PyLong_FromUnsignedLong(value as _)),
            )
        })
    }
}

pub(in crate::bindings) unsafe extern "C" fn murmur3_x86_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        crate::bindings::schema::murmur3::murmur3_x86_128_digest(
            args,
            nargs,
            keywords,
            |py, input, seed| {
                let Some(seed) = seed_u32(seed.as_ptr()) else {
                    return std::ptr::null_mut();
                };
                let result =
                    with_function_bytes(py, input.as_ptr(), MURMUR3_DETACH_THRESHOLD, |bytes| {
                        x86_128_digest(murmur3_x86_128(bytes, seed))
                    });
                return_function_result(py, result.map(|digest| bytes_result(&digest)))
            },
        )
    }
}

pub(in crate::bindings) unsafe extern "C" fn murmur3_x64_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        crate::bindings::schema::murmur3::murmur3_x64_128_digest(
            args,
            nargs,
            keywords,
            |py, input, seed| {
                let Some(seed) = seed_u32(seed.as_ptr()) else {
                    return std::ptr::null_mut();
                };
                let result =
                    with_function_bytes(py, input.as_ptr(), MURMUR3_DETACH_THRESHOLD, |bytes| {
                        x64_128_digest(murmur3_x64_128(bytes, seed))
                    });
                return_function_result(py, result.map(|digest| bytes_result(&digest)))
            },
        )
    }
}
