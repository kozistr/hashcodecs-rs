use core::{ptr, slice};

use pyo3::exceptions::PyMemoryError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::with_output_ptr;

pub(super) struct BytesWriter(*mut ffi::compat::PyBytesWriter);

impl BytesWriter {
    pub(super) fn new(py: Python<'_>, input_len: usize) -> PyResult<Self> {
        let capacity = input_len
            .div_ceil(4)
            .checked_mul(3)
            .and_then(|length| ffi::Py_ssize_t::try_from(length).ok())
            .ok_or_else(|| PyMemoryError::new_err("Base64 output is too large"))?;
        let writer = unsafe { ffi::compat::PyBytesWriter_Create(capacity) };
        if writer.is_null() {
            Err(PyErr::fetch(py))
        } else {
            Ok(Self(writer))
        }
    }

    pub(super) unsafe fn data(&self) -> *mut u8 {
        unsafe { ffi::compat::PyBytesWriter_GetData(self.0).cast() }
    }

    pub(super) unsafe fn finish<'py>(
        mut self,
        py: Python<'py>,
        length: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let length = ffi::Py_ssize_t::try_from(length)
            .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
        let writer = self.0;
        self.0 = ptr::null_mut();
        let output = unsafe { ffi::compat::PyBytesWriter_FinishWithSize(writer, length) };
        Ok(unsafe { Bound::from_owned_ptr_or_err(py, output)?.cast_into_unchecked() })
    }
}

impl Drop for BytesWriter {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { ffi::compat::PyBytesWriter_Discard(self.0) };
        }
    }
}

pub(super) fn copy_decoded_into(
    decoded: &Bound<'_, PyBytes>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let decoded = decoded.as_bytes();
    with_output_ptr(output, decoded.len(), |output| {
        let output = unsafe { slice::from_raw_parts_mut(output, decoded.len()) };
        output.copy_from_slice(decoded);
    })?;
    Ok(decoded.len())
}
