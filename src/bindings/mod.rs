use std::ffi::c_char;
use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyModule};

use self::buffer::bytes_like;
use self::murmur3::{PyMurmur3X64Hasher128, PyMurmur3X86Hasher32, PyMurmur3X86Hasher128};

mod base64;
mod buffer;
mod murmur3;
mod xxhash;

pub(super) const DETACH_THRESHOLD: usize = 64 * 1024;
pub(super) const METHOD_FLAGS: i32 = ffi::METH_FASTCALL | ffi::METH_KEYWORDS;

#[inline]
unsafe fn tuple_size(tuple: *mut ffi::PyObject) -> usize {
    unsafe { ffi::PyTuple_GET_SIZE(tuple) as usize }
}

#[inline]
unsafe fn tuple_item(tuple: *mut ffi::PyObject, index: usize) -> *mut ffi::PyObject {
    unsafe { ffi::PyTuple_GET_ITEM(tuple, index as isize) }
}

pub(super) fn list_items<'py>(items: &Bound<'py, PyList>) -> Vec<Bound<'py, PyAny>> {
    #[cfg(Py_GIL_DISABLED)]
    return items.iter().collect();
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        let length = ffi::PyList_GET_SIZE(items.as_ptr()) as usize;
        let mut values = Vec::with_capacity(length);
        for index in 0..length {
            let item = ffi::PyList_GET_ITEM(items.as_ptr(), index as isize);
            values.push(Bound::from_borrowed_ptr(items.py(), item));
        }
        values
    }
}

#[inline]
pub(super) unsafe fn bytearray_data(value: *mut ffi::PyObject) -> *mut u8 {
    #[cfg(Py_GIL_DISABLED)]
    return unsafe { ffi::PyByteArray_AsString(value).cast() };
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        ffi::PyByteArray_AS_STRING(value).cast()
    }
}

#[inline]
pub(super) unsafe fn bytearray_size(value: *mut ffi::PyObject) -> usize {
    #[cfg(Py_GIL_DISABLED)]
    return unsafe { ffi::PyByteArray_Size(value) as usize };
    #[cfg(not(Py_GIL_DISABLED))]
    unsafe {
        ffi::PyByteArray_GET_SIZE(value) as usize
    }
}

#[inline]
pub(super) unsafe fn bytes_data(value: *mut ffi::PyObject) -> *const u8 {
    unsafe { ffi::PyBytes_AS_STRING(value).cast() }
}

#[inline]
pub(super) unsafe fn bytes_data_mut(value: *mut ffi::PyObject) -> *mut u8 {
    unsafe { bytes_data(value).cast_mut() }
}

#[inline]
pub(super) unsafe fn bytes_size(value: *mut ffi::PyObject) -> usize {
    unsafe { ffi::Py_SIZE(value) as usize }
}

#[derive(Clone, Copy)]
pub(super) struct HashArguments {
    pub(super) input: *mut ffi::PyObject,
    pub(super) seed: *mut ffi::PyObject,
}

pub(super) unsafe fn parse_hash_arguments(
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
    name: *const c_char,
) -> Option<HashArguments> {
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
        unsafe { tuple_size(keywords) }
    };
    if nargs + keyword_count > 2 {
        type_error(name, c"got too many arguments".as_ptr());
        return None;
    }

    for index in 0..keyword_count {
        let keyword = unsafe { tuple_item(keywords, index) };
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
    Some(HashArguments { input, seed })
}

pub(super) unsafe fn parse_raw_arguments<const N: usize>(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    function_name: *const c_char,
    parameter_names: [*const c_char; N],
    max_positional: usize,
    required: usize,
) -> Option<[*mut ffi::PyObject; N]> {
    let nargs = nargs as usize;
    if nargs > max_positional {
        unsafe {
            ffi::PyErr_Format(
                ffi::PyExc_TypeError,
                c"%s() takes at most %zu positional arguments (%zu given)".as_ptr(),
                function_name,
                max_positional,
                nargs,
            );
        }
        return None;
    }

    let mut values = [ptr::null_mut(); N];
    for (index, value) in values.iter_mut().take(nargs).enumerate() {
        *value = unsafe { *args.add(index) };
    }

    let keyword_count = if keywords.is_null() {
        0
    } else {
        unsafe { tuple_size(keywords) }
    };
    for keyword_index in 0..keyword_count {
        let keyword = unsafe { tuple_item(keywords, keyword_index) };
        let value = unsafe { *args.add(nargs + keyword_index) };
        let parameter_index = parameter_names.iter().position(|parameter| {
            (unsafe { ffi::PyUnicode_CompareWithASCIIString(keyword, *parameter) }) == 0
        });
        let Some(parameter_index) = parameter_index else {
            unsafe {
                ffi::PyErr_Format(
                    ffi::PyExc_TypeError,
                    c"%s() got an unexpected keyword argument '%U'".as_ptr(),
                    function_name,
                    keyword,
                );
            }
            return None;
        };
        if !values[parameter_index].is_null() {
            unsafe {
                ffi::PyErr_Format(
                    ffi::PyExc_TypeError,
                    c"%s() got multiple values for argument '%s'".as_ptr(),
                    function_name,
                    parameter_names[parameter_index],
                );
            }
            return None;
        }
        values[parameter_index] = value;
    }

    for index in 0..required {
        if values[index].is_null() {
            unsafe {
                ffi::PyErr_Format(
                    ffi::PyExc_TypeError,
                    c"%s() missing required argument '%s'".as_ptr(),
                    function_name,
                    parameter_names[index],
                );
            }
            return None;
        }
    }
    Some(values)
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
        let length = unsafe { bytes_size(object) };
        let bytes = unsafe { std::slice::from_raw_parts(bytes_data(object), length) };
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
    unsafe { base64::add_to_module(module)? };
    unsafe { murmur3::add_to_module(module)? };
    module.add_class::<PyMurmur3X86Hasher32>()?;
    module.add_class::<PyMurmur3X86Hasher128>()?;
    module.add_class::<PyMurmur3X64Hasher128>()?;
    unsafe { xxhash::add_to_module(module)? };
    Ok(())
}
