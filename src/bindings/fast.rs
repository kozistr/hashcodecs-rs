use std::ffi::c_char;
use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;

use super::buffer::bytes_like;
use crate::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128, xxh3_64, xxh3_128};

const FLAGS: i32 = ffi::METH_FASTCALL | ffi::METH_KEYWORDS;

#[derive(Clone, Copy)]
struct Arguments {
    input: *mut ffi::PyObject,
    seed: *mut ffi::PyObject,
}

unsafe fn parse_arguments(
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
    name: *const c_char,
) -> Option<Arguments> {
    let nargs = nargsf as usize;
    if nargs > 2 {
        type_error(name, c"takes at most 2 arguments".as_ptr());
        return None;
    }

    let mut input = if nargs == 0 {
        ptr::null_mut()
    } else {
        unsafe { *args }
    };
    let mut seed = if nargs < 2 {
        ptr::null_mut()
    } else {
        unsafe { *args.add(1) }
    };

    let keyword_count = if keywords.is_null() {
        0
    } else {
        unsafe { ffi::PyTuple_Size(keywords) as usize }
    };
    if nargs + keyword_count > 2 {
        type_error(name, c"got too many arguments".as_ptr());
        return None;
    }

    for index in 0..keyword_count {
        let keyword = unsafe { ffi::PyTuple_GetItem(keywords, index as isize) };
        let value = unsafe { *args.add(nargs + index) };
        if unsafe { ffi::PyUnicode_CompareWithASCIIString(keyword, c"s".as_ptr()) } == 0 {
            if !input.is_null() {
                type_error(name, c"got multiple values for argument 's'".as_ptr());
                return None;
            }
            input = value;
        } else if unsafe { ffi::PyUnicode_CompareWithASCIIString(keyword, c"seed".as_ptr()) } == 0 {
            if !seed.is_null() {
                type_error(name, c"got multiple values for argument 'seed'".as_ptr());
                return None;
            }
            seed = value;
        } else {
            type_error(name, c"got an unexpected keyword argument".as_ptr());
            return None;
        }
    }

    if input.is_null() {
        type_error(name, c"missing required argument 's'".as_ptr());
        return None;
    }
    Some(Arguments { input, seed })
}

fn type_error(name: *const c_char, detail: *const c_char) {
    unsafe {
        ffi::PyErr_Format(ffi::PyExc_TypeError, c"%s() %s".as_ptr(), name, detail);
    }
}

unsafe fn seed_u32(seed: *mut ffi::PyObject) -> Option<u32> {
    if seed.is_null() {
        return Some(0);
    }
    let value = unsafe { ffi::PyLong_AsUnsignedLongLong(seed) };
    if unsafe { ffi::PyErr_Occurred() }.is_null() && value <= u64::from(u32::MAX) {
        Some(value as u32)
    } else {
        if unsafe { ffi::PyErr_Occurred() }.is_null() {
            unsafe {
                ffi::PyErr_SetString(
                    ffi::PyExc_OverflowError,
                    c"seed does not fit in uint32".as_ptr(),
                )
            };
        }
        None
    }
}

unsafe fn seed_u64(seed: *mut ffi::PyObject) -> Option<u64> {
    if seed.is_null() {
        return Some(0);
    }
    let value = unsafe { ffi::PyLong_AsUnsignedLongLong(seed) };
    if unsafe { ffi::PyErr_Occurred() }.is_null() {
        Some(value as u64)
    } else {
        None
    }
}

fn bytes_result(digest: &[u8]) -> *mut ffi::PyObject {
    unsafe { ffi::PyBytes_FromStringAndSize(digest.as_ptr().cast(), digest.len() as isize) }
}

fn with_bytes<T: Send>(
    py: Python<'_>,
    object: *mut ffi::PyObject,
    operation: impl FnOnce(&[u8]) -> T + Send,
) -> PyResult<T> {
    if unsafe { ffi::PyBytes_CheckExact(object) } != 0 {
        let length = unsafe { ffi::PyBytes_Size(object) } as usize;
        let bytes =
            unsafe { std::slice::from_raw_parts(ffi::PyBytes_AsString(object).cast(), length) };
        return if length >= super::DETACH_THRESHOLD {
            Ok(py.detach(|| operation(bytes)))
        } else {
            Ok(operation(bytes))
        };
    }
    let object = unsafe { Bound::from_borrowed_ptr(py, object) };
    let input = bytes_like(py, &object, "s")?;
    let detach = input.detach_safe() && input.len() >= super::DETACH_THRESHOLD;
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

unsafe fn return_pyresult(
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

unsafe extern "C" fn murmur3_32_fast(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_arguments(args, nargsf, keywords, c"murmur3_32".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_bytes(py, arguments.input, |bytes| murmur3_x86_32(bytes, seed));
        return_pyresult(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLong(value as _)),
        )
    }
}

unsafe extern "C" fn murmur3_x86_128_fast(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_arguments(args, nargsf, keywords, c"murmur3_x86_128_digest".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_bytes(py, arguments.input, |bytes| {
            let words = murmur3_x86_128(bytes, seed);
            let mut digest = [0_u8; 16];
            for (index, word) in words.iter().enumerate() {
                digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
            }
            digest
        });
        return_pyresult(py, result.map(|digest| bytes_result(&digest)))
    }
}

unsafe extern "C" fn murmur3_x64_128_fast(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) =
            parse_arguments(args, nargsf, keywords, c"murmur3_x64_128_digest".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u32(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_bytes(py, arguments.input, |bytes| {
            let words = murmur3_x64_128(bytes, seed);
            let mut digest = [0_u8; 16];
            for (index, word) in words.iter().enumerate() {
                digest[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
            }
            digest
        });
        return_pyresult(py, result.map(|digest| bytes_result(&digest)))
    }
}

unsafe extern "C" fn xxh3_64_fast(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_arguments(args, nargsf, keywords, c"xxh3_64".as_ptr()) else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_bytes(py, arguments.input, |bytes| xxh3_64(bytes, seed));
        return_pyresult(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLongLong(value as _)),
        )
    }
}

unsafe extern "C" fn xxh3_128_fast(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_arguments(args, nargsf, keywords, c"xxh3_128".as_ptr()) else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_bytes(py, arguments.input, |bytes| xxh3_128(bytes, seed));
        match result {
            Ok([low, high]) => {
                let value = (u128::from(high) << 64) | u128::from(low);
                pyo3::types::PyInt::new(py, value).into_ptr()
            }
            Err(error) => {
                error.restore(py);
                ptr::null_mut()
            }
        }
    }
}

static mut METHODS: [ffi::PyMethodDef; 6] = [
    ffi::PyMethodDef {
        ml_name: c"murmur3_32".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_32_fast,
        },
        ml_flags: FLAGS,
        ml_doc: c"murmur3_32($module, /, s, seed=0)\n--\n\n".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"murmur3_x86_128_digest".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_x86_128_fast,
        },
        ml_flags: FLAGS,
        ml_doc: c"murmur3_x86_128_digest($module, /, s, seed=0)\n--\n\n".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"murmur3_x64_128_digest".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_x64_128_fast,
        },
        ml_flags: FLAGS,
        ml_doc: c"murmur3_x64_128_digest($module, /, s, seed=0)\n--\n\n".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_fast,
        },
        ml_flags: FLAGS,
        ml_doc: c"xxh3_64($module, /, s, seed=0)\n--\n\n".as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_fast,
        },
        ml_flags: FLAGS,
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
