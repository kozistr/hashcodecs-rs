use std::ffi::c_char;
use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use self::base64::{
    b64decode, b64decode_batch, b64decode_batch_into, b64decode_into, b64encode, b64encode_batch,
    b64encode_batch_into, b64encode_into, standard_b64decode, standard_b64decode_into,
    standard_b64encode, standard_b64encode_into, urlsafe_b64decode_315, urlsafe_b64decode_into_315,
    urlsafe_b64decode_into_pre_315, urlsafe_b64decode_pre_315, urlsafe_b64encode,
    urlsafe_b64encode_into,
};
use self::buffer::bytes_like;
use self::murmur3::{PyMurmur3X64Hasher128, PyMurmur3X86Hasher32, PyMurmur3X86Hasher128};
use self::xxhash::{xxh3_64_batch, xxh3_128_batch};

mod base64;
mod buffer;
mod murmur3;
mod xxhash;

pub(super) const DETACH_THRESHOLD: usize = 64 * 1024;
pub(super) const METHOD_FLAGS: i32 = ffi::METH_FASTCALL | ffi::METH_KEYWORDS;

#[derive(Clone, Copy)]
pub(super) struct FunctionArguments {
    pub(super) input: *mut ffi::PyObject,
    pub(super) seed: *mut ffi::PyObject,
}

pub(super) unsafe fn parse_function_arguments(
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
    name: *const c_char,
) -> Option<FunctionArguments> {
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
    Some(FunctionArguments { input, seed })
}

fn type_error(name: *const c_char, detail: *const c_char) {
    unsafe {
        ffi::PyErr_Format(ffi::PyExc_TypeError, c"%s() %s".as_ptr(), name, detail);
    }
}

pub(super) unsafe fn seed_u32(seed: *mut ffi::PyObject) -> Option<u32> {
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

pub(super) unsafe fn seed_u64(seed: *mut ffi::PyObject) -> Option<u64> {
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

pub(super) fn with_function_bytes<T: Send>(
    py: Python<'_>,
    object: *mut ffi::PyObject,
    operation: impl FnOnce(&[u8]) -> T + Send,
) -> PyResult<T> {
    if unsafe { ffi::PyBytes_CheckExact(object) } != 0 {
        let length = unsafe { ffi::PyBytes_Size(object) } as usize;
        let bytes =
            unsafe { std::slice::from_raw_parts(ffi::PyBytes_AsString(object).cast(), length) };
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

#[pymodule(name = "_hashcodecs")]
fn python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(b64encode_batch, module)?)?;
    module.add_function(wrap_pyfunction!(b64encode_batch_into, module)?)?;
    module.add_function(wrap_pyfunction!(b64encode_into, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode_batch, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode_batch_into, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode_into, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64encode_into, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64decode, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64decode_into, module)?)?;
    module.add_function(wrap_pyfunction!(urlsafe_b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(urlsafe_b64encode_into, module)?)?;
    if base64::python_at_least(module.py(), (3, 15)) {
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_315, module)?)?;
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_into_315, module)?)?;
    } else {
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_pre_315, module)?)?;
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_into_pre_315, module)?)?;
    }
    unsafe { murmur3::add_to_module(module)? };
    module.add_class::<PyMurmur3X86Hasher32>()?;
    module.add_class::<PyMurmur3X86Hasher128>()?;
    module.add_class::<PyMurmur3X64Hasher128>()?;
    unsafe { xxhash::add_to_module(module)? };
    module.add_function(wrap_pyfunction!(xxh3_64_batch, module)?)?;
    module.add_function(wrap_pyfunction!(xxh3_128_batch, module)?)?;
    Ok(())
}
