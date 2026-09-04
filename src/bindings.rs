use pyo3::prelude::*;
use pyo3::types::PyModule;

use self::murmur3::{PyMurmur3X64Hasher128, PyMurmur3X86Hasher32, PyMurmur3X86Hasher128};

mod arguments;
mod base64;
mod buffer;
mod murmur3;
mod objects;
mod runtime;
mod schema;
mod xxhash;

#[pymodule(name = "_hashcodecs")]
fn python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    unsafe { base64::add_to_module(module)? };
    unsafe { murmur3::add_to_module(module)? };
    module.add_class::<PyMurmur3X86Hasher32>()?;
    module.add_class::<PyMurmur3X86Hasher128>()?;
    module.add_class::<PyMurmur3X64Hasher128>()?;
    unsafe { xxhash::add_to_module(module)? };
    Ok(())
}
