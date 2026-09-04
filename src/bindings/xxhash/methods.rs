use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::{
    xxh3_64_batch_digest, xxh3_64_batch_into_digest, xxh3_64_digest, xxh3_128_batch_digest,
    xxh3_128_batch_into_digest, xxh3_128_digest,
};
use crate::bindings::runtime::{METHOD_FLAGS, add_methods};

include!("../../../generated/rust/xxhash_methods.rs");

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    unsafe { add_methods(module, methods) }
}
