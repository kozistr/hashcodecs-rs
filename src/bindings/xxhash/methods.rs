use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::sync::Once;

use crate::bindings::runtime::add_methods;
use crate::bindings::schema::xxhash::{BINDING_COUNT, register_all};

static mut METHODS: [ffi::PyMethodDef; BINDING_COUNT + 1] =
    [const { ffi::PyMethodDef::zeroed() }; BINDING_COUNT + 1];

static METHODS_INIT: Once = Once::new();

pub(crate) fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    unsafe { add_methods(module, methods, &METHODS_INIT, register_all) }
}
