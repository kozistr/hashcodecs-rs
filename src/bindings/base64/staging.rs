//! Output allocation, capacity checks, and configured decoding staging.

use core::ptr;
use std::mem::MaybeUninit;
use std::slice;

use pyo3::exceptions::{PyMemoryError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::configured::Translation;
use crate::base64::{
    DecodeAlphabet, STANDARD_ALPHABET, decode_to_ptr_with_unpadded_layout, decode_unpadded_layout,
    validate_alphabet,
};
use crate::bindings::buffer::with_bytearray;
use crate::bindings::objects::{bytearray_data, bytearray_size, bytes_data_mut};

pub(super) const CONFIGURED_STAGING_CAPACITY: usize = 4096;

#[inline]
unsafe fn decode_staging<const CHECKED: bool>(input: &[u8], output: *mut u8) -> Option<usize> {
    let layout = if CHECKED {
        decode_unpadded_layout(input).ok()?
    } else {
        decode_unpadded_layout(input).expect("validated configured Base64 staging remains valid")
    };
    let decoded = unsafe {
        decode_to_ptr_with_unpadded_layout(input, output, layout, DecodeAlphabet::Standard)
    };
    if CHECKED {
        decoded.ok()?;
    } else {
        decoded.expect("validated configured Base64 staging remains valid");
    }
    Some(layout.output_len())
}

pub(super) struct StagingWriter {
    staging: [MaybeUninit<u8>; CONFIGURED_STAGING_CAPACITY],
    staged: usize,
    output: *mut u8,
    written: usize,
    translation: Option<Translation>,
}

impl StagingWriter {
    pub(super) fn new(output: *mut u8, translation: Option<Translation>) -> Self {
        Self {
            staging: [MaybeUninit::uninit(); CONFIGURED_STAGING_CAPACITY],
            staged: 0,
            output,
            written: 0,
            translation,
        }
    }

    pub(super) fn set_translation(&mut self, translation: Option<Translation>) {
        assert_eq!(
            self.staged, 0,
            "translation changes only before staging starts"
        );
        self.translation = translation;
    }

    pub(super) fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = (input.len() - source).min(CONFIGURED_STAGING_CAPACITY - self.staged);
            // The initialized staging range is exactly `0..staged`. Extend it
            // only after copying every byte in the new suffix.
            unsafe {
                self.staging
                    .as_mut_ptr()
                    .add(self.staged)
                    .cast::<u8>()
                    .copy_from_nonoverlapping(input.as_ptr().add(source), copied)
            };
            self.staged += copied;
            source += copied;
            if self.staged == CONFIGURED_STAGING_CAPACITY {
                self.flush::<CHECKED>()?;
            }
        }
        Some(())
    }

    pub(super) fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()> {
        self.staging[self.staged].write(STANDARD_ALPHABET[usize::from(value)]);
        self.staged += 1;
        if self.staged == CONFIGURED_STAGING_CAPACITY {
            self.flush::<CHECKED>()?;
        }
        Some(())
    }

    fn flush<const CHECKED: bool>(&mut self) -> Option<()> {
        // Push methods initialize every byte in this prefix before increasing
        // `staged`; no code reads the uninitialized suffix.
        let staging = unsafe {
            slice::from_raw_parts_mut(self.staging.as_mut_ptr().cast::<u8>(), self.staged)
        };
        if let Some(translation) = self.translation {
            translation.apply(staging);
        }
        self.written +=
            unsafe { decode_staging::<CHECKED>(staging, self.output.add(self.written))? };
        self.staged = 0;
        Some(())
    }

    pub(super) fn finish<const CHECKED: bool>(&mut self) -> Option<usize> {
        if self.staged != 0 {
            self.flush::<CHECKED>()?;
        }
        Some(self.written)
    }
}

pub(super) struct StagingValidator {
    staging: [MaybeUninit<u8>; CONFIGURED_STAGING_CAPACITY],
    staged: usize,
    translation: Option<Translation>,
}

impl StagingValidator {
    pub(super) fn new(translation: Option<Translation>) -> Self {
        Self {
            staging: [MaybeUninit::uninit(); CONFIGURED_STAGING_CAPACITY],
            staged: 0,
            translation,
        }
    }

    pub(super) fn push(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = (input.len() - source).min(CONFIGURED_STAGING_CAPACITY - self.staged);
            // As with `StagingWriter`, `0..staged` is the sole initialized range.
            unsafe {
                self.staging
                    .as_mut_ptr()
                    .add(self.staged)
                    .cast::<u8>()
                    .copy_from_nonoverlapping(input.as_ptr().add(source), copied)
            };
            self.staged += copied;
            source += copied;
            if self.staged == CONFIGURED_STAGING_CAPACITY {
                self.flush()?;
            }
        }
        Some(())
    }

    fn flush(&mut self) -> Option<()> {
        // Every byte in this prefix was initialized by `push`; the remainder
        // of the array stays uninitialized and is never exposed.
        let staging = unsafe {
            slice::from_raw_parts_mut(self.staging.as_mut_ptr().cast::<u8>(), self.staged)
        };
        if let Some(translation) = self.translation {
            translation.apply(staging);
        }
        decode_unpadded_layout(staging).ok()?;
        validate_alphabet(staging, DecodeAlphabet::Standard).ok()?;
        self.staged = 0;
        Some(())
    }

    pub(super) fn finish(mut self) -> Option<()> {
        if self.staged != 0 {
            self.flush()?;
        }
        Some(())
    }
}

/// Allocate an uninitialized Python `bytes` payload for direct initialization.
///
/// # Safety
/// If the returned Python object can escape, `init` must have initialized all
/// `length` bytes. An initialization error may leave bytes unwritten only when
/// the caller discards the object without reading its payload.
pub(super) unsafe fn pybytes_with_len<'py, T>(
    py: Python<'py>,
    length: usize,
    init: impl FnOnce(*mut u8) -> T,
) -> PyResult<(Bound<'py, PyBytes>, T)> {
    let length = ffi::Py_ssize_t::try_from(length)
        .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
    unsafe {
        let raw = ffi::PyBytes_FromStringAndSize(core::ptr::null(), length);
        let bytes: Bound<'py, PyBytes> =
            Bound::from_owned_ptr_or_err(py, raw)?.cast_into_unchecked();
        let buffer = bytes_data_mut(raw);
        debug_assert!(!buffer.is_null());

        // CPython leaves the payload uninitialized when passed a null source.
        // Keep it behind a raw pointer until the initializer has written every
        // byte instead of creating a Rust `&mut [u8]` with invalid contents.
        let initialized = init(buffer);
        Ok((bytes, initialized))
    }
}

pub(super) fn with_output_ptr<T>(
    output: &Bound<'_, PyByteArray>,
    required: usize,
    callback: impl FnOnce(*mut u8) -> T,
) -> PyResult<T> {
    with_bytearray(output, || {
        let provided = unsafe { bytearray_size(output.as_ptr()) };
        if provided < required {
            return Err(output_too_small(required, provided));
        }
        Ok(callback(unsafe { bytearray_data(output.as_ptr()) }))
    })
}

pub(super) fn output_too_small(required: usize, provided: usize) -> PyErr {
    PyValueError::new_err(format!(
        "Base64 output requires {required} bytes but the destination has {provided}"
    ))
}

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
