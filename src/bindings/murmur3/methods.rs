use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::one_shot::{murmur3_32, murmur3_x64_128_digest, murmur3_x86_128_digest};
use crate::bindings::runtime::{METHOD_FLAGS, add_methods};

static mut METHODS: [ffi::PyMethodDef; 4] = [
    ffi::PyMethodDef {
        ml_name: c"murmur3_32".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_32,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"murmur3_32($module, /, s, seed=0)
--

Compute the canonical MurmurHash3 x86 32-bit hash.

MurmurHash3 is fast but non-cryptographic.

Args:
    s: Contiguous bytes-like data to hash.
    seed: Initial unsigned 32-bit seed.

Returns:
    The unsigned 32-bit hash as a Python integer.

Raises:
    TypeError: s is not bytes-like or seed is not an integer.
    OverflowError: seed is outside 0 <= seed < 2**32.

Examples:
    >>> hex(murmur3_32(b'hello'))
    '0x248bfa47'"
            .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"murmur3_x86_128_digest".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_x86_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"murmur3_x86_128_digest($module, /, s, seed=0)
--

Compute the canonical MurmurHash3 x86 128-bit digest.

The four result words are serialized as little-endian 32-bit integers.

Args:
    s: Contiguous bytes-like data to hash.
    seed: Initial unsigned 32-bit seed.

Returns:
    A 16-byte digest.

Raises:
    TypeError: s is not bytes-like or seed is not an integer.
    OverflowError: seed is outside 0 <= seed < 2**32.

Examples:
    >>> len(murmur3_x86_128_digest(b'hello'))
    16"
        .as_ptr(),
    },
    ffi::PyMethodDef {
        ml_name: c"murmur3_x64_128_digest".as_ptr(),
        ml_meth: ffi::PyMethodDefPointer {
            PyCFunctionFastWithKeywords: murmur3_x64_128_digest,
        },
        ml_flags: METHOD_FLAGS,
        ml_doc: cr"murmur3_x64_128_digest($module, /, s, seed=0)
--

Compute the canonical MurmurHash3 x64 128-bit digest.

The two result words are serialized as little-endian 64-bit integers.

Args:
    s: Contiguous bytes-like data to hash.
    seed: Initial unsigned 32-bit seed.

Returns:
    A 16-byte digest.

Raises:
    TypeError: s is not bytes-like or seed is not an integer.
    OverflowError: seed is outside 0 <= seed < 2**32.

Examples:
    >>> len(murmur3_x64_128_digest(b'hello'))
    16"
        .as_ptr(),
    },
    ffi::PyMethodDef::zeroed(),
];

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    unsafe { add_methods(module, methods) }
}
