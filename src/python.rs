use pyo3::prelude::*;
use pyo3::types::PyModule;

use self::base64::{
    b64decode, b64decode_batch, b64decode_batch_into, b64decode_into, b64encode, b64encode_batch,
    b64encode_batch_into, b64encode_into, standard_b64decode, standard_b64decode_into,
    standard_b64encode, standard_b64encode_into, urlsafe_b64decode_315, urlsafe_b64decode_into_315,
    urlsafe_b64decode_into_pre_315, urlsafe_b64decode_pre_315, urlsafe_b64encode,
    urlsafe_b64encode_into,
};
use self::murmur3::{
    PyMurmur3X64Hasher128, PyMurmur3X86Hasher32, PyMurmur3X86Hasher128, murmur3_32,
    murmur3_x64_128_digest, murmur3_x86_128_digest,
};

mod base64;
mod buffer;
mod murmur3;

pub(super) const DETACH_THRESHOLD: usize = 64 * 1024;

#[pymodule(name = "_hashcodecs")]
fn python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(b64encode_batch, module)?)?;
    module.add_function(wrap_pyfunction!(b64encode_batch_into, module)?)?;
    module.add_function(wrap_pyfunction!(b64encode_into, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode_batch, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode_batch_into, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode_into, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64encode_into, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64decode, module)?)?;
    module.add_function(wrap_pyfunction!(standard_b64decode_into, module)?)?;
    module.add_function(wrap_pyfunction!(urlsafe_b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(urlsafe_b64encode_into, module)?)?;
    if base64::python_at_least(module.py(), (3, 15)) {
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_315, module)?)?;
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_into_315, module)?)?;
    } else {
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_pre_315, module)?)?;
        module.add_function(wrap_pyfunction!(urlsafe_b64decode_into_pre_315, module)?)?;
    }
    module.add_function(wrap_pyfunction!(murmur3_32, module)?)?;
    module.add_function(wrap_pyfunction!(murmur3_x86_128_digest, module)?)?;
    module.add_function(wrap_pyfunction!(murmur3_x64_128_digest, module)?)?;
    module.add_class::<PyMurmur3X86Hasher32>()?;
    module.add_class::<PyMurmur3X86Hasher128>()?;
    module.add_class::<PyMurmur3X64Hasher128>()?;
    Ok(())
}
