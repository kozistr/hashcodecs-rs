use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;

use super::one_shot::{murmur3_32, murmur3_x64_128_digest, murmur3_x86_128_digest};
use crate::bindings::runtime::{METHOD_FLAGS, add_methods};

include!("../../../generated/rust/murmur3_methods.rs");

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    unsafe { add_methods(module, methods) }
}
