//! Output allocation, capacity checks, and configured decoding staging.

use core::ptr;
use std::mem::MaybeUninit;
use std::slice;

use pyo3::exceptions::{PyMemoryError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::configured::Translation;
use super::lenient::decoded_len_upper_bound;
use crate::base64::{
    DecodeAlphabet, STANDARD_ALPHABET, decode_standard_validated_to_ptr,
    decode_to_ptr_with_unpadded_layout, decode_unpadded_layout, validate_alphabet,
};
use crate::bindings::buffer::with_bytearray;
use crate::bindings::objects::{bytearray_data, bytearray_size, bytes_data_mut};

pub(super) const CONFIGURED_STAGING_CAPACITY: usize = 4096;

#[inline]
unsafe fn decode_staging<const CHECKED: bool>(input: &[u8], output: *mut u8) -> Option<usize> {
    if CHECKED {
        let layout = decode_unpadded_layout(input).ok()?;
        unsafe {
            decode_to_ptr_with_unpadded_layout(input, output, layout, DecodeAlphabet::Standard)
        }
        .ok()?;
        Some(layout.output_len())
    } else {
        Some(unsafe { decode_standard_validated_to_ptr(input, output) })
    }
}

#[repr(align(32))]
struct AlignedStaging([MaybeUninit<u8>; CONFIGURED_STAGING_CAPACITY]);

#[repr(C)]
struct StagingBuffer {
    initialized: usize,
    bytes: AlignedStaging,
}

impl StagingBuffer {
    fn new() -> Self {
        Self {
            initialized: 0,
            bytes: AlignedStaging([MaybeUninit::uninit(); CONFIGURED_STAGING_CAPACITY]),
        }
    }

    fn is_empty(&self) -> bool {
        self.initialized == 0
    }

    fn is_full(&self) -> bool {
        self.initialized == CONFIGURED_STAGING_CAPACITY
    }

    fn remaining_capacity(&self) -> usize {
        CONFIGURED_STAGING_CAPACITY - self.initialized
    }

    fn extend_from_slice(&mut self, input: &[u8]) -> usize {
        let copied = input
            .len()
            .min(CONFIGURED_STAGING_CAPACITY - self.initialized);

        // `0..initialized` is the sole initialized range. Extend it only
        // after copying every byte in the new suffix.
        unsafe {
            self.bytes
                .0
                .as_mut_ptr()
                .add(self.initialized)
                .cast::<u8>()
                .copy_from_nonoverlapping(input.as_ptr(), copied)
        };
        self.initialized += copied;
        copied
    }

    fn push(&mut self, value: u8) {
        self.bytes.0[self.initialized].write(value);
        self.initialized += 1;
    }

    fn initialized_mut(&mut self) -> &mut [u8] {
        // The append methods initialize each byte before increasing the
        // length, and no method exposes the uninitialized suffix.
        unsafe {
            slice::from_raw_parts_mut(self.bytes.0.as_mut_ptr().cast::<u8>(), self.initialized)
        }
    }

    fn clear(&mut self) {
        self.initialized = 0;
    }
}

// Keep hot metadata beside the start of the SIMD-aligned scratch buffer.
#[repr(C)]
pub(super) struct StagingWriter {
    output: *mut u8,
    written: usize,
    translation: Option<Translation>,
    staging: StagingBuffer,
}

impl StagingWriter {
    pub(super) fn new(output: *mut u8, translation: Option<Translation>) -> Self {
        Self {
            output,
            written: 0,
            translation,
            staging: StagingBuffer::new(),
        }
    }

    pub(super) fn set_translation(&mut self, translation: Option<Translation>) {
        assert!(
            self.staging.is_empty(),
            "translation changes only before staging starts"
        );
        self.translation = translation;
    }

    pub(super) fn push_symbols<const CHECKED: bool>(&mut self, input: &[u8]) -> Option<()> {
        if self.translation.is_some() {
            return self.push_staged_symbols::<CHECKED>(input);
        }

        let mut source = 0;
        while source < input.len() {
            // Complete untranslated quartets need no staging copy. Preserve
            // fragments until a full staging buffer or the final flush.
            if self.staging.is_empty() {
                let direct = (input.len() - source) / 4 * 4;
                if direct != 0 {
                    self.written += unsafe {
                        decode_staging::<CHECKED>(
                            &input[source..source + direct],
                            self.output.add(self.written),
                        )?
                    };
                    source += direct;
                    continue;
                }
            }

            let copied = (input.len() - source).min(self.staging.remaining_capacity());
            self.push_staged_symbols::<CHECKED>(&input[source..source + copied])?;
            source += copied;
        }

        Some(())
    }

    fn push_staged_symbols<const CHECKED: bool>(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = self.staging.extend_from_slice(&input[source..]);
            source += copied;

            if self.staging.is_full() {
                self.flush::<CHECKED>()?;
            }
        }

        Some(())
    }

    pub(super) fn push_value<const CHECKED: bool>(&mut self, value: u8) -> Option<()> {
        self.staging.push(STANDARD_ALPHABET[usize::from(value)]);

        if self.staging.is_full() {
            self.flush::<CHECKED>()?;
        }

        Some(())
    }

    fn flush<const CHECKED: bool>(&mut self) -> Option<()> {
        let staging = self.staging.initialized_mut();

        if let Some(translation) = self.translation {
            translation.apply(staging);
        }

        self.written +=
            unsafe { decode_staging::<CHECKED>(staging, self.output.add(self.written))? };
        self.staging.clear();

        Some(())
    }

    pub(super) fn finish<const CHECKED: bool>(&mut self) -> Option<usize> {
        if !self.staging.is_empty() {
            self.flush::<CHECKED>()?;
        }

        Some(self.written)
    }
}

#[repr(C)]
pub(super) struct StagingValidator {
    translation: Option<Translation>,
    staging: StagingBuffer,
}

impl StagingValidator {
    pub(super) fn new(translation: Option<Translation>) -> Self {
        Self {
            translation,
            staging: StagingBuffer::new(),
        }
    }

    pub(super) fn push(&mut self, input: &[u8]) -> Option<()> {
        let mut source = 0;
        while source < input.len() {
            let copied = self.staging.extend_from_slice(&input[source..]);
            source += copied;

            if self.staging.is_full() {
                self.flush()?;
            }
        }

        Some(())
    }

    fn flush(&mut self) -> Option<()> {
        let staging = self.staging.initialized_mut();

        if let Some(translation) = self.translation {
            translation.apply(staging);
        }

        decode_unpadded_layout(staging).ok()?;
        validate_alphabet(staging, DecodeAlphabet::Standard).ok()?;
        self.staging.clear();

        Some(())
    }

    pub(super) fn finish(mut self) -> Option<()> {
        if !self.staging.is_empty() {
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
    #[inline]
    pub(super) fn capacity_for_input(input: &[u8], table: &[u8; 256]) -> usize {
        // Small outputs fit the CPython/PyO3 writer's inline storage. Tightening
        // their bound cannot save a resize, so avoid inspecting padding there.
        if input.len() <= 256 {
            input.len().div_ceil(4) * 3
        } else {
            decoded_len_upper_bound(input, table)
        }
    }

    pub(super) fn new(py: Python<'_>, capacity: usize) -> PyResult<Self> {
        let capacity = ffi::Py_ssize_t::try_from(capacity)
            .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scratch_buffers_are_aligned_and_follow_hot_metadata() {
        assert_eq!(std::mem::align_of::<StagingBuffer>(), 32);
        assert_eq!(std::mem::offset_of!(StagingBuffer, bytes) % 32, 0);
        for offset in [
            std::mem::offset_of!(StagingWriter, staging),
            std::mem::offset_of!(StagingValidator, staging),
        ] {
            assert!(offset <= 64);
            assert_eq!(offset % 32, 0);
        }
    }
}
