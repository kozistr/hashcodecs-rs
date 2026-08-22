use std::ptr;

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList};

use super::buffer::{BytesLike, bytes_like};
use super::{
    DETACH_THRESHOLD, METHOD_FLAGS, parse_hash_arguments, parse_raw_arguments,
    return_function_result, seed_u64, with_function_bytes,
};
use crate::{xxh3_64, xxh3_128};

mod batch;
use batch::{xxh3_64_batch, xxh3_64_batch_into, xxh3_128_batch, xxh3_128_batch_into};

unsafe extern "C" fn xxh3_64_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"xxh3_64".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| xxh3_64(bytes, seed));
        return_function_result(
            py,
            result.map(|value| ffi::PyLong_FromUnsignedLongLong(value as _)),
        )
    }
}

unsafe extern "C" fn xxh3_128_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargsf: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
        let Some(arguments) = parse_hash_arguments(args, nargsf, keywords, c"xxh3_128".as_ptr())
        else {
            return ptr::null_mut();
        };
        let Some(seed) = seed_u64(arguments.seed) else {
            return ptr::null_mut();
        };
        let result = with_function_bytes(py, arguments.input, |bytes| xxh3_128(bytes, seed));
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
    }
}

unsafe extern "C" fn xxh3_64_batch_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
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
    }
}

unsafe extern "C" fn xxh3_128_batch_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
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
    }
}

unsafe extern "C" fn xxh3_64_batch_into_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
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
    }
}

unsafe extern "C" fn xxh3_128_batch_into_digest(
    _self: *mut ffi::PyObject,
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    let py = unsafe { Python::assume_attached() };
    unsafe {
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
    }
}

static mut METHODS: [ffi::PyMethodDef; 7] = [
    ffi::PyMethodDef {
        ml_name: c"xxh3_64".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"xxh3_64($module, /, s, seed=0)
--

Compute the canonical XXH3 64-bit hash.

XXH3 is fast but non-cryptographic.

Args:
    s: Contiguous bytes-like data to hash.
    seed: Initial unsigned 64-bit seed.

Returns:
    The unsigned 64-bit hash as a Python integer.

Raises:
    TypeError: s is not bytes-like or seed is not an integer.
    OverflowError: seed is outside 0 <= seed < 2**64.

Examples:
    >>> hex(xxh3_64(b''))
    '0x2d06800538d394c2'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"xxh3_128($module, /, s, seed=0)
--

Compute the canonical XXH3 128-bit hash.

The low word occupies the least significant half. XXH3 is non-cryptographic.

Args:
    s: Contiguous bytes-like data to hash.
    seed: Initial unsigned 64-bit seed.

Returns:
    The unsigned 128-bit hash as a Python integer.

Raises:
    TypeError: s is not bytes-like or seed is not an integer.
    OverflowError: seed is outside 0 <= seed < 2**64.

Examples:
    >>> hex(xxh3_128(b''))
    '0x99aa06d3014798d86001c324468d497f'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_batch_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"xxh3_64_batch($module, /, items, seed=0)
--

Compute canonical XXH3 64-bit hashes for a list of inputs.

Args:
    items: A list of contiguous bytes-like objects to hash.
    seed: Initial unsigned 64-bit seed shared by every item.

Returns:
    One unsigned 64-bit integer per item, in input order.

Raises:
    TypeError: The container, an item, or seed has an invalid type.
    OverflowError: seed is outside 0 <= seed < 2**64.

Examples:
    >>> xxh3_64_batch([b'', b'hello']) == [xxh3_64(b''), xxh3_64(b'hello')]
    True"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_batch_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"xxh3_128_batch($module, /, items, seed=0)
--

Compute canonical XXH3 128-bit hashes for a list of inputs.

Args:
    items: A list of contiguous bytes-like objects to hash.
    seed: Initial unsigned 64-bit seed shared by every item.

Returns:
    One unsigned 128-bit integer per item, in input order.

Raises:
    TypeError: The container, an item, or seed has an invalid type.
    OverflowError: seed is outside 0 <= seed < 2**64.

Examples:
    >>> xxh3_128_batch([b'', b'hello']) == [xxh3_128(b''), xxh3_128(b'hello')]
    True"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_batch_into_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"xxh3_64_batch_into($module, /, items, output, seed=0)
--

Write XXH3 64-bit hashes as packed little-endian bytes.

Inputs and capacity are validated before output is mutated.

Args:
    items: A list of contiguous bytes-like objects to hash.
    output: Destination with at least 8 * len(items) bytes.
    seed: Initial unsigned 64-bit seed shared by every item.

Returns:
    The total number of bytes written.

Raises:
    TypeError: A container, item, destination, or seed has an invalid type.
    ValueError: output is too small.
    OverflowError: seed is outside 0 <= seed < 2**64.

Examples:
    >>> output = bytearray(8)
    >>> xxh3_64_batch_into([b'hello'], output)
    8"
        .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_batch_into_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"xxh3_128_batch_into($module, /, items, output, seed=0)
--

Write XXH3 128-bit hashes as packed little-endian bytes.

Inputs and capacity are validated before output is mutated.

Args:
    items: A list of contiguous bytes-like objects to hash.
    output: Destination with at least 16 * len(items) bytes.
    seed: Initial unsigned 64-bit seed shared by every item.

Returns:
    The total number of bytes written.

Raises:
    TypeError: A container, item, destination, or seed has an invalid type.
    ValueError: output is too small.
    OverflowError: seed is outside 0 <= seed < 2**64.

Examples:
    >>> output = bytearray(16)
    >>> xxh3_128_batch_into([b'hello'], output)
    16"
        .as_ptr(),
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

fn parse_batch<'a, 'py>(
    py: Python<'py>,
    items: &'a [Bound<'py, PyAny>],
) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    items
        .iter()
        .map(|item| bytes_like(py, item, "items element"))
        .collect()
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= DETACH_THRESHOLD
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> Vec<&'a [u8]> {
    inputs
        .iter()
        .map(|input| unsafe { input.as_bytes() })
        .collect()
}
