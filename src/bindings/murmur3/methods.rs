use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::sync::Once;

use crate::bindings::runtime::add_methods;
use crate::bindings::schema::murmur3::{BINDING_COUNT, register_all};

static mut METHODS: [ffi::PyMethodDef; BINDING_COUNT + 1] =
    [const { ffi::PyMethodDef::zeroed() }; BINDING_COUNT + 1];

static METHODS_INIT: Once = Once::new();

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    let version_info = module.py().version_info();
    let version = (version_info.major, version_info.minor);
    METHODS_INIT.call_once(|| unsafe { register_all(methods, version) });
    unsafe { add_methods(module, methods) }
}
