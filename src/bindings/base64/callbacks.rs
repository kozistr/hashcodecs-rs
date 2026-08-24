use pyo3::ffi;
use pyo3::types::{PyByteArray, PyList};

use crate::bindings::runtime::catch_unwind_callback;

use super::*;

pub(super) unsafe extern "C" fn standard_b64encode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64encode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64encode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64encode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64encode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64encode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64encode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64encode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64encode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64encode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64encode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64encode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64decode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64decode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64decode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64decode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn standard_b64decode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64decode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64decode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64decode_batch(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64decode_batch_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn b64decode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64decode(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
        let result = truthy_argument(py, values[1], default)
            .and_then(|padded| super::urlsafe_b64decode(py, raw_argument(py, &values[0]), padded));
        return_bound(py, result)
    })
}

pub(super) unsafe extern "C" fn urlsafe_b64decode_into(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    catch_unwind_callback(py, || unsafe {
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
            super::urlsafe_b64decode_into(py, raw_argument(py, &values[0]), output, padded)
        })();
        return_usize(py, result)
    })
}
