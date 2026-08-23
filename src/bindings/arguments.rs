use std::ffi::c_char;
use std::ptr;

use pyo3::ffi;

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

#[inline]
unsafe fn tuple_size(tuple: *mut ffi::PyObject) -> usize {
    unsafe { ffi::PyTuple_GET_SIZE(tuple) as usize }
}

#[inline]
unsafe fn tuple_item(tuple: *mut ffi::PyObject, index: usize) -> *mut ffi::PyObject {
    unsafe { ffi::PyTuple_GET_ITEM(tuple, index as isize) }
}

fn type_error(name: *const c_char, detail: *const c_char) {
    unsafe {
        ffi::PyErr_Format(ffi::PyExc_TypeError, c"%s() %s".as_ptr(), name, detail);
    }
}
