use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3::types::{PyByteArray, PyList};

use super::*;

unsafe extern "C" fn standard_b64encode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64encode".as_ptr(),
            [c"s".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        return_bound(
            py,
            super::standard_b64encode(py, raw_argument(py, &values[0])),
        )
    }
}

unsafe extern "C" fn standard_b64encode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64encode_into".as_ptr(),
            [c"s".as_ptr(), c"output".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            super::standard_b64encode_into(py, raw_argument(py, &values[0]), output)
        })();
        return_usize(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64encode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64encode".as_ptr(),
            [c"s".as_ptr(), c"padded".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = truthy_argument(py, values[1], true)
            .and_then(|padded| super::urlsafe_b64encode(py, raw_argument(py, &values[0]), padded));
        return_bound(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64encode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64encode_into".as_ptr(),
            [c"s".as_ptr(), c"output".as_ptr(), c"padded".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let padded = truthy_argument(py, values[2], true)?;
            super::urlsafe_b64encode_into(py, raw_argument(py, &values[0]), output, padded)
        })();
        return_usize(py, result)
    }
}

unsafe extern "C" fn b64encode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64encode".as_ptr(),
            [
                c"s".as_ptr(),
                c"altchars".as_ptr(),
                c"padded".as_ptr(),
                c"wrapcol".as_ptr(),
            ],
            2,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let padded = truthy_argument(py, values[2], true)?;
            let wrapcol = if values[3].is_null() {
                0
            } else {
                raw_argument(py, &values[3]).extract::<i128>()?
            };
            super::b64encode(
                py,
                raw_argument(py, &values[0]),
                optional_argument(py, &values[1]),
                padded,
                wrapcol,
            )
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn b64encode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64encode_batch".as_ptr(),
            [c"items".as_ptr(), c"altchars".as_ptr()],
            2,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::b64encode_batch(py, items, optional_argument(py, &values[1]))
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn b64encode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64encode_batch_into".as_ptr(),
            [c"items".as_ptr(), c"outputs".as_ptr(), c"altchars".as_ptr()],
            3,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::b64encode_batch_into(py, items, outputs, optional_argument(py, &values[2]))
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn standard_b64encode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64encode_batch".as_ptr(),
            [c"items".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::standard_b64encode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn standard_b64encode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64encode_batch_into".as_ptr(),
            [c"items".as_ptr(), c"outputs".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::standard_b64encode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64encode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64encode_batch".as_ptr(),
            [c"items".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::urlsafe_b64encode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64encode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64encode_batch_into".as_ptr(),
            [c"items".as_ptr(), c"outputs".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::urlsafe_b64encode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn b64encode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64encode_into".as_ptr(),
            [
                c"s".as_ptr(),
                c"output".as_ptr(),
                c"altchars".as_ptr(),
                c"padded".as_ptr(),
                c"wrapcol".as_ptr(),
            ],
            3,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let padded = truthy_argument(py, values[3], true)?;
            let wrapcol = if values[4].is_null() {
                0
            } else {
                raw_argument(py, &values[4]).extract::<i128>()?
            };
            super::b64encode_into(
                py,
                raw_argument(py, &values[0]),
                output,
                optional_argument(py, &values[2]),
                padded,
                wrapcol,
            )
        })();
        return_usize(py, result)
    }
}

unsafe extern "C" fn b64decode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64decode".as_ptr(),
            [
                c"s".as_ptr(),
                c"altchars".as_ptr(),
                c"validate".as_ptr(),
                c"padded".as_ptr(),
                c"ignorechars".as_ptr(),
                c"canonical".as_ptr(),
            ],
            3,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let validate = if values[2].is_null() {
                None
            } else {
                Some(truthy_argument(py, values[2], false)?)
            };
            let padded = truthy_argument(py, values[3], true)?;
            let canonical = truthy_argument(py, values[5], false)?;
            super::b64decode(
                py,
                raw_argument(py, &values[0]),
                optional_argument(py, &values[1]),
                validate,
                padded,
                provided_argument(py, &values[4]),
                canonical,
            )
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn standard_b64decode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64decode".as_ptr(),
            [c"s".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        return_bound(
            py,
            super::standard_b64decode(py, raw_argument(py, &values[0])),
        )
    }
}

unsafe extern "C" fn standard_b64decode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64decode_into".as_ptr(),
            [c"s".as_ptr(), c"output".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            super::standard_b64decode_into(py, raw_argument(py, &values[0]), output)
        })();
        return_usize(py, result)
    }
}

unsafe extern "C" fn standard_b64decode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64decode_batch".as_ptr(),
            [c"items".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::standard_b64decode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn standard_b64decode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"standard_b64decode_batch_into".as_ptr(),
            [c"items".as_ptr(), c"outputs".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::standard_b64decode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64decode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64decode_batch".as_ptr(),
            [c"items".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::urlsafe_b64decode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64decode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64decode_batch_into".as_ptr(),
            [c"items".as_ptr(), c"outputs".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::urlsafe_b64decode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn b64decode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64decode_batch".as_ptr(),
            [
                c"items".as_ptr(),
                c"altchars".as_ptr(),
                c"validate".as_ptr(),
            ],
            3,
            1,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let validate = truthy_argument(py, values[2], false)?;
            super::b64decode_batch(py, items, optional_argument(py, &values[1]), validate)
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn b64decode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64decode_batch_into".as_ptr(),
            [
                c"items".as_ptr(),
                c"outputs".as_ptr(),
                c"altchars".as_ptr(),
                c"validate".as_ptr(),
            ],
            4,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            let validate = truthy_argument(py, values[3], false)?;
            super::b64decode_batch_into(
                py,
                items,
                outputs,
                optional_argument(py, &values[2]),
                validate,
            )
        })();
        return_bound(py, result)
    }
}

unsafe extern "C" fn b64decode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"b64decode_into".as_ptr(),
            [
                c"s".as_ptr(),
                c"output".as_ptr(),
                c"altchars".as_ptr(),
                c"validate".as_ptr(),
                c"padded".as_ptr(),
                c"ignorechars".as_ptr(),
                c"canonical".as_ptr(),
            ],
            4,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let validate = if values[3].is_null() {
                None
            } else {
                Some(truthy_argument(py, values[3], false)?)
            };
            let padded = truthy_argument(py, values[4], true)?;
            let canonical = truthy_argument(py, values[6], false)?;
            super::b64decode_into(
                py,
                raw_argument(py, &values[0]),
                output,
                optional_argument(py, &values[2]),
                validate,
                padded,
                provided_argument(py, &values[5]),
                canonical,
            )
        })();
        return_usize(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64decode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64decode".as_ptr(),
            [c"s".as_ptr(), c"padded".as_ptr()],
            1,
            1,
        ) else {
            return ptr::null_mut();
        };
        let default = !python_at_least(py, (3, 15));
        let result = truthy_argument(py, values[1], default).and_then(|padded| {
            if python_at_least(py, (3, 15)) {
                super::urlsafe_b64decode_315(py, raw_argument(py, &values[0]), padded)
            } else {
                super::urlsafe_b64decode_pre_315(py, raw_argument(py, &values[0]), padded)
            }
        });
        return_bound(py, result)
    }
}

unsafe extern "C" fn urlsafe_b64decode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(values) = parse_raw_arguments(
            args,
            nargs,
            keywords,
            c"urlsafe_b64decode_into".as_ptr(),
            [c"s".as_ptr(), c"output".as_ptr(), c"padded".as_ptr()],
            2,
            2,
        ) else {
            return ptr::null_mut();
        };
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let default = !python_at_least(py, (3, 15));
            let padded = truthy_argument(py, values[2], default)?;
            if python_at_least(py, (3, 15)) {
                super::urlsafe_b64decode_into_315(py, raw_argument(py, &values[0]), output, padded)
            } else {
                super::urlsafe_b64decode_into_pre_315(
                    py,
                    raw_argument(py, &values[0]),
                    output,
                    padded,
                )
            }
        })();
        return_usize(py, result)
    }
}

static mut METHODS: [ffi::PyMethodDef; 25] = [
    ffi::PyMethodDef {
        ml_name: c"standard_b64encode".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64encode,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64encode($module, /, s)
--

Encode bytes with the padded standard Base64 alphabet.

Args:
    s: Contiguous bytes-like data to encode.

Returns:
    Newly allocated Base64 bytes using + and /.

Raises:
    TypeError: s is not a contiguous bytes-like object.

Examples:
    >>> standard_b64encode(b'hello')
    b'aGVsbG8='"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64encode_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64encode_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64encode_into($module, /, s, output)
--

Encode bytes with the standard alphabet into a reusable bytearray.

Only the written prefix changes; the destination keeps its size.

Args:
    s: Contiguous bytes-like data to encode.
    output: Destination bytearray with room for the padded result.

Returns:
    The number of bytes written to output.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: output is too small.

Examples:
    >>> output = bytearray(8)
    >>> standard_b64encode_into(b'hello', output)
    8"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64encode".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64encode,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64encode($module, /, s, *, padded=True)
--

Encode bytes with the URL-safe Base64 alphabet.

Args:
    s: Contiguous bytes-like data to encode.
    padded: Append trailing = padding when required.

Returns:
    Newly allocated Base64 bytes using - and _.

Raises:
    TypeError: s is not a contiguous bytes-like object.

Examples:
    >>> urlsafe_b64encode(bytes([251, 255]), padded=False)
    b'-_8'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64encode_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64encode_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64encode_into($module, /, s, output, *, padded=True)
--

Encode bytes with the URL-safe alphabet into a reusable bytearray.

Args:
    s: Contiguous bytes-like data to encode.
    output: Destination bytearray with room for the complete result.
    padded: Append trailing = padding when required.

Returns:
    The number of bytes written to output.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: output is too small.

Examples:
    >>> output = bytearray(4)
    >>> urlsafe_b64encode_into(bytes([251, 255]), output)
    4"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64encode".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64encode,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64encode($module, /, s, altchars=None, *, padded=True, wrapcol=0)
--

Encode a bytes-like object as Base64.

The alphabet, padding, and fixed-width line wrapping are configurable.

Args:
    s: Contiguous bytes-like data to encode.
    altchars: A two-byte replacement for + and /, or None.
    padded: Append trailing = padding when required.
    wrapcol: Maximum characters per line; zero disables wrapping.

Returns:
    Newly allocated Base64-encoded bytes.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: altchars is not length two or wrapcol is negative.

Examples:
    >>> b64encode(b'hello')
    b'aGVsbG8='"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64encode_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64encode_batch,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64encode_batch($module, /, items, altchars=None)
--

Encode a list of bytes-like objects as padded Base64.

Args:
    items: A list of contiguous bytes-like objects.
    altchars: A two-byte replacement for + and /, or None.

Returns:
    Encoded byte strings in input order.

Raises:
    TypeError: items is not a list or an item is not bytes-like.
    ValueError: altchars is not exactly two bytes.

Examples:
    >>> b64encode_batch([b'one', b'two'])
    [b'b25l', b'dHdv']"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64encode_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64encode_batch_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64encode_batch_into($module, /, items, outputs, altchars=None)
--

Encode each item into a matching reusable bytearray.

Processing is fail-fast and non-transactional.

Args:
    items: A list of contiguous bytes-like objects.
    outputs: An equal-length list of distinct destination bytearrays.
    altchars: A two-byte replacement for + and /, or None.

Returns:
    The number of bytes written to each destination.

Raises:
    TypeError: A container, item, or destination has an invalid type.
    ValueError: A list length, destination, or altchars is invalid.

Examples:
    >>> outputs = [bytearray(4), bytearray(4)]
    >>> b64encode_batch_into([b'one', b'two'], outputs)
    [4, 4]"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64encode_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64encode_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64encode_into($module, /, s, output, altchars=None, *, padded=True, wrapcol=0)
--

Encode Base64 into a reusable bytearray.

Only the written prefix changes; the destination keeps its size.

Args:
    s: Contiguous bytes-like data to encode.
    output: Destination bytearray with room for the result.
    altchars: A two-byte replacement for + and /, or None.
    padded: Append trailing = padding when required.
    wrapcol: Maximum characters per line; zero disables wrapping.

Returns:
    The number of bytes written to output.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: An option is invalid or output is too small.

Examples:
    >>> output = bytearray(8)
    >>> b64encode_into(b'hello', output)
    8"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64decode".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64decode,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64decode($module, /, s, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)
--

Decode an ASCII string or bytes-like Base64 value.

Strict validation, unpadded input, ignored bytes, and canonical tail bits are configurable.

Args:
    s: ASCII text or contiguous bytes-like Base64 data.
    altchars: Two characters replacing + and /, or None.
    validate: Reject non-alphabet bytes when true.
    padded: Require padding when true; accept an unpadded tail when false.
    ignorechars: Bytes allowed outside the alphabet in lenient mode.
    canonical: Reject non-zero unused tail bits.

Returns:
    Newly allocated decoded bytes.

Raises:
    binascii.Error: The Base64 data, padding, or tail bits are invalid.
    TypeError: An argument has an unsupported type.
    ValueError: Text is not ASCII or altchars is not length two.

Examples:
    >>> b64decode(b'aGVsbG8=', validate=True)
    b'hello'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64decode".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64decode,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64decode($module, /, s)
--

Decode padded Base64 using the standard alphabet.

Args:
    s: ASCII text or contiguous bytes-like Base64 data.

Returns:
    Newly allocated decoded bytes.

Raises:
    binascii.Error: The remaining Base64 data has invalid padding.
    TypeError: s has an unsupported type.
    ValueError: Text input is not ASCII.

Examples:
    >>> standard_b64decode(b'aGVsbG8=')
    b'hello'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64decode_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64decode_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64decode_into($module, /, s, output)
--

Decode standard Base64 into a reusable bytearray.

Args:
    s: ASCII text or contiguous bytes-like Base64 data.
    output: Destination bytearray with room for the decoded result.

Returns:
    The number of decoded bytes written to output.

Raises:
    binascii.Error: The input has invalid Base64 padding.
    TypeError: An argument has an unsupported type.
    ValueError: output is too small or text is not ASCII.

Examples:
    >>> output = bytearray(5)
    >>> standard_b64decode_into(b'aGVsbG8=', output)
    5"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64decode_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64decode_batch,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64decode_batch($module, /, items, altchars=None, validate=False)
--

Decode a list of padded Base64 values.

Args:
    items: A list of ASCII strings or bytes-like Base64 values.
    altchars: Two characters replacing + and /, or None.
    validate: Reject bytes outside the selected alphabet when true.

Returns:
    Decoded byte strings in input order.

Raises:
    binascii.Error: An item contains invalid Base64 data or padding.
    TypeError: items is not a list or an item has an invalid type.
    ValueError: Text is not ASCII or altchars is not length two.

Examples:
    >>> b64decode_batch([b'b25l', b'dHdv'], validate=True)
    [b'one', b'two']"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64decode_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64decode_batch_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64decode_batch_into($module, /, items, outputs, altchars=None, validate=False)
--

Decode each item into a matching reusable bytearray.

Processing is fail-fast and non-transactional.

Args:
    items: A list of ASCII strings or bytes-like Base64 values.
    outputs: An equal-length list of distinct destination bytearrays.
    altchars: Two characters replacing + and /, or None.
    validate: Reject bytes outside the selected alphabet when true.

Returns:
    The number of decoded bytes written to each destination.

Raises:
    binascii.Error: An item contains invalid Base64 data or padding.
    TypeError: A container, item, or destination has an invalid type.
    ValueError: A list length, destination, text value, or altchars is invalid.

Examples:
    >>> outputs = [bytearray(3), bytearray(3)]
    >>> b64decode_batch_into([b'b25l', b'dHdv'], outputs, validate=True)
    [3, 3]"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"b64decode_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: b64decode_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"b64decode_into($module, /, s, output, altchars=None, validate=None, *, padded=True, ignorechars=None, canonical=False)
--

Decode Base64 data into a reusable bytearray.

The options match b64decode. Invalid input may modify part of the output prefix.

Args:
    s: ASCII text or contiguous bytes-like Base64 data.
    output: Destination bytearray with room for the result.
    altchars: Two characters replacing + and /, or None.
    validate: Reject non-alphabet bytes when true.
    padded: Require padding when true; accept an unpadded tail when false.
    ignorechars: Bytes allowed outside the alphabet in lenient mode.
    canonical: Reject non-zero unused tail bits.

Returns:
    The number of decoded bytes written to output.

Raises:
    binascii.Error: The Base64 data, padding, or tail bits are invalid.
    TypeError: An argument has an unsupported type.
    ValueError: An option is invalid or output is too small.

Examples:
    >>> output = bytearray(5)
    >>> b64decode_into(b'aGVsbG8=', output, validate=True)
    5"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64decode".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64decode,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64decode($module, /, s, *, padded=True)
--

Decode Base64 using the URL-safe alphabet.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    padded: Require padding when true; accept an unpadded tail when false.

Returns:
    Newly allocated decoded bytes.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: s has an unsupported type.
    ValueError: Text input is not ASCII.

Examples:
    >>> urlsafe_b64decode(b'-_8=', padded=True)
    b'\xfb\xff'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64decode_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64decode_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64decode_into($module, /, s, output, *, padded=True)
--

Decode URL-safe Base64 into a reusable bytearray.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    output: Destination bytearray with room for the result.
    padded: Require padding when true; accept an unpadded tail when false.

Returns:
    The number of decoded bytes written to output.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: An argument has an unsupported type.
    ValueError: output is too small or text is not ASCII.

Examples:
    >>> output = bytearray(2)
    >>> urlsafe_b64decode_into(b'-_8=', output, padded=True)
    2"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64encode_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64encode_batch,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64encode_batch($module, /, items)
--

Encode a list of bytes-like objects with the standard Base64 alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64encode_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64encode_batch_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64encode_batch_into($module, /, items, outputs)
--

Encode each item into a matching reusable bytearray with the standard alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64encode_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64encode_batch,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64encode_batch($module, /, items)
--

Encode a list of bytes-like objects with the URL-safe Base64 alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64encode_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64encode_batch_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64encode_batch_into($module, /, items, outputs)
--

Encode each item into a matching reusable bytearray with the URL-safe alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64decode_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64decode_batch,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64decode_batch($module, /, items)
--

Decode a list of padded Base64 values with the standard alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"standard_b64decode_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: standard_b64decode_batch_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"standard_b64decode_batch_into($module, /, items, outputs)
--

Decode each item into a matching reusable bytearray with the standard alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64decode_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64decode_batch,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64decode_batch($module, /, items)
--

Decode a list of padded Base64 values with the URL-safe alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"urlsafe_b64decode_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: urlsafe_b64decode_batch_into,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"urlsafe_b64decode_batch_into($module, /, items, outputs)
--

Decode each item into a matching reusable bytearray with the URL-safe alphabet."
            .as_ptr(),
    },
    ffi::PyMethodDef::zeroed(),
];

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    if python_at_least(module.py(), (3, 15)) {
        unsafe {
            (*methods.add(14)).ml_doc = cr"urlsafe_b64decode($module, /, s, *, padded=False)
--

Decode Base64 using the URL-safe alphabet.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    padded: Require padding when true; accept an unpadded tail when false.

Returns:
    Newly allocated decoded bytes.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: s has an unsupported type.
    ValueError: Text input is not ASCII.

Examples:
    >>> urlsafe_b64decode(b'-_8')
    b'\xfb\xff'"
                .as_ptr();
            (*methods.add(15)).ml_doc =
                cr"urlsafe_b64decode_into($module, /, s, output, *, padded=False)
--

Decode URL-safe Base64 into a reusable bytearray.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    output: Destination bytearray with room for the result.
    padded: Require padding when true; accept an unpadded tail when false.

Returns:
    The number of decoded bytes written to output.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: An argument has an unsupported type.
    ValueError: output is too small or text is not ASCII.

Examples:
    >>> output = bytearray(2)
    >>> urlsafe_b64decode_into(b'-_8', output)
    2"
                .as_ptr();
        }
    }
    let result = unsafe { ffi::PyModule_AddFunctions(module.as_ptr(), methods) };
    if result == -1 {
        Err(PyErr::fetch(module.py()))
    } else {
        Ok(())
    }
}
