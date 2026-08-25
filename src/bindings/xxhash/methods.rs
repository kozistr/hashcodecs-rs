use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::{
    xxh3_64_batch_digest, xxh3_64_batch_into_digest, xxh3_64_digest, xxh3_128_batch_digest,
    xxh3_128_batch_into_digest, xxh3_128_digest,
};
use crate::bindings::runtime::{METHOD_FLAGS, add_methods};

static mut METHODS: [ffi::PyMethodDef; 7] = [
    ffi::PyMethodDef {
        ml_name: c"xxh3_64".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr###"xxh3_64($module, /, s, seed=0)
--

Compute the canonical XXH3 64-bit hash.

XXH3 is a non-cryptographic hash designed for speed.

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
    '0x2d06800538d394c2'"###
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr###"xxh3_128($module, /, s, seed=0)
--

Compute the canonical XXH3 128-bit hash.

The low 64-bit word occupies the least significant half of the returned
integer. XXH3 is non-cryptographic.

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
    '0x99aa06d3014798d86001c324468d497f'"###
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_batch_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr###"xxh3_64_batch($module, /, items, seed=0)
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
    True"###
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128_batch".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_batch_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr###"xxh3_128_batch($module, /, items, seed=0)
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
    True"###
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_64_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_64_batch_into_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr###"xxh3_64_batch_into($module, /, items, output, seed=0)
--

Write XXH3 64-bit hashes as packed little-endian bytes.

Inputs and capacity are validated before output is mutated. Bytes after the
written prefix remain unchanged.

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
    8
    >>> int.from_bytes(output, 'little') == xxh3_64(b'hello')
    True"###
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"xxh3_128_batch_into".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: xxh3_128_batch_into_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr###"xxh3_128_batch_into($module, /, items, output, seed=0)
--

Write XXH3 128-bit hashes as packed little-endian bytes.

Inputs and capacity are validated before output is mutated. Bytes after the
written prefix remain unchanged.

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
    16
    >>> int.from_bytes(output, 'little') == xxh3_128(b'hello')
    True"###
            .as_ptr(),
    },
    ffi::PyMethodDef::zeroed(),
];

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    unsafe { add_methods(module, methods) }
}
