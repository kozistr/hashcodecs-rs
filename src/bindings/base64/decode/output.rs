use core::{ptr, slice};

use pyo3::exceptions::PyMemoryError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::{STANDARD_ALPHABET, output_too_small, with_output_ptr};
use super::fallback::{
    canonical_padding, decode_with_binascii, decoding_error, warn_legacy_altchars,
};
use super::policy::{
    AdvancedShortcut, DecodeRoute, ErrorWrites, Padding, PreparedDecoder, StoreBounds,
};
use super::{
    decode_advanced, decode_advanced_into, decode_strict, decode_strict_into,
    decode_strict_into_with_altchars, decode_strict_with_altchars,
    decode_unpadded_into_with_altchars, decode_unpadded_with_altchars, try_decode_lenient,
    try_decode_lenient_into, try_decode_strict, try_decode_urlsafe_315,
    try_decode_urlsafe_315_into,
};
use crate::base64::{Base64Error, DecodeAlphabet};
use crate::bindings::buffer::BytesLike;

impl PreparedDecoder {
    pub(super) fn decode_allocating<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let value = match self.route {
            DecodeRoute::Advanced(shortcut) => {
                self.decode_advanced_allocating(py, input, shortcut)?
            }
            DecodeRoute::Strict { urlsafe_315 } => {
                if urlsafe_315
                    && let Some(value) = try_decode_urlsafe_315(
                        py,
                        input,
                        self.policy.padding,
                        self.policy.error_writes(),
                    )?
                {
                    return Ok(value);
                }
                self.decode_strict_allocating(py, input)?
            }
            DecodeRoute::LenientDirect { urlsafe_315 } => {
                if urlsafe_315
                    && let Some(value) = try_decode_urlsafe_315(
                        py,
                        input,
                        self.policy.padding,
                        self.policy.error_writes(),
                    )?
                {
                    return Ok(value);
                }
                if let Some(value) = try_decode_strict(py, input, self.direct_alphabet())? {
                    value
                } else if self.policy.padding == Padding::Padded
                    && unsafe {
                        input.with_bytes(|input| {
                            is_mime_whitespace_input(input, self.policy.altchars == Some(*b"-_"))
                        })
                    }
                {
                    decode_advanced(py, input, self.advanced(), self.semantics)?
                } else if self.policy.padding == Padding::Unpadded
                    && let Some(value) = self.try_unpadded_allocating(py, input)?
                {
                    value
                } else if let Some(value) = try_decode_lenient(
                    py,
                    input,
                    self.policy.padding,
                    self.lenient_table(),
                    self.semantics,
                )? {
                    value
                } else {
                    self.decode_binascii(py, input)?
                }
            }
            DecodeRoute::LenientCustom => {
                if self.policy.padding == Padding::Unpadded
                    && let Some(value) = self.try_unpadded_allocating(py, input)?
                {
                    value
                } else if let Some(value) = try_decode_lenient(
                    py,
                    input,
                    self.policy.padding,
                    self.lenient_table(),
                    self.semantics,
                )? {
                    value
                } else {
                    self.decode_binascii(py, input)?
                }
            }
        };
        self.finish(py, input, value)
    }

    fn decode_advanced_allocating<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        shortcut: AdvancedShortcut,
    ) -> PyResult<Bound<'py, PyBytes>> {
        if shortcut == AdvancedShortcut::StandardStrict {
            match decode_strict(py, input, DecodeAlphabet::Standard) {
                Ok(value) => {
                    let canonical_input = !self.policy.canonical
                        || unsafe {
                            input.with_bytes(|input| {
                                let padding = usize::from(input.ends_with(b"="))
                                    + usize::from(input.ends_with(b"=="));
                                canonical_padding(&input[..input.len() - padding])
                            })
                        };
                    if canonical_input {
                        return Ok(value);
                    }
                    return Err(decoding_error(py, "Non-zero padding bits"));
                }
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        }
        if shortcut == AdvancedShortcut::CanonicalUnpadded
            && unsafe {
                input.with_bytes(|input| canonical_unpadded_input(input, self.policy.altchars))
            }
        {
            match decode_unpadded_with_altchars(
                py,
                input,
                self.policy.altchars,
                || self.strict_custom(),
                self.semantics,
            ) {
                Ok(value) => return Ok(value),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        }
        decode_advanced(py, input, self.advanced(), self.semantics)
    }

    fn decode_strict_allocating<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let result = if self.policy.padding == Padding::Padded {
            decode_strict_with_altchars(
                py,
                input,
                self.policy.altchars,
                || self.strict_custom(),
                self.semantics,
            )
        } else {
            decode_unpadded_with_altchars(
                py,
                input,
                self.policy.altchars,
                || self.strict_custom(),
                self.semantics,
            )
        };
        match result {
            Ok(value) => Ok(value),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
            Err(_) => self.decode_binascii(py, input),
        }
    }

    fn try_unpadded_allocating<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        match decode_unpadded_with_altchars(
            py,
            input,
            self.policy.altchars,
            || self.strict_custom(),
            self.semantics,
        ) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn decode_into(
        &self,
        py: Python<'_>,
        input: &BytesLike<'_, '_>,
        output: &Bound<'_, PyByteArray>,
    ) -> PyResult<usize> {
        // Every route and the compatibility-warning scan must observe the same bytes.
        if let Some(input) = input.snapshot_for_output(output)? {
            return self.decode_into(py, &BytesLike::OwnedVec(input), output);
        }
        let value = match self.route {
            DecodeRoute::Advanced(shortcut) => {
                self.decode_advanced_into(py, input, output, shortcut)?
            }
            DecodeRoute::Strict { urlsafe_315 } => {
                if urlsafe_315
                    && let Some(value) = try_decode_urlsafe_315_into(
                        input,
                        output,
                        self.policy.padding,
                        self.policy.error_writes(),
                        self.policy.store_bounds(),
                    )?
                {
                    return Ok(value);
                }
                self.decode_strict_into(py, input, output)?
            }
            DecodeRoute::LenientDirect { urlsafe_315 } => {
                if urlsafe_315
                    && let Some(value) = try_decode_urlsafe_315_into(
                        input,
                        output,
                        self.policy.padding,
                        self.policy.error_writes(),
                        self.policy.store_bounds(),
                    )?
                {
                    return Ok(value);
                }
                if let Some(value) = accept_into(
                    decode_strict_into(
                        input,
                        output,
                        self.direct_alphabet(),
                        ErrorWrites::PreserveOutput,
                    )?,
                    StoreBounds::DeferToFallback,
                )? {
                    value
                } else if self.policy.padding == Padding::Padded
                    && unsafe {
                        input.with_bytes(|input| {
                            is_mime_whitespace_input(input, self.policy.altchars == Some(*b"-_"))
                        })
                    }
                {
                    decode_advanced_into(py, input, output, self.advanced(), self.semantics)?
                } else if self.policy.padding == Padding::Unpadded
                    && let Some(value) = self.try_unpadded_into(input, output)?
                {
                    value
                } else if let Some(value) = try_decode_lenient_into(
                    input,
                    output,
                    self.policy.altchars,
                    self.policy.padding,
                    self.lenient_table(),
                    self.semantics,
                )? {
                    value
                } else {
                    self.decode_binascii_into(py, input, output)?
                }
            }
            DecodeRoute::LenientCustom => {
                if self.policy.padding == Padding::Unpadded
                    && let Some(value) = self.try_unpadded_into(input, output)?
                {
                    value
                } else if let Some(value) = try_decode_lenient_into(
                    input,
                    output,
                    self.policy.altchars,
                    self.policy.padding,
                    self.lenient_table(),
                    self.semantics,
                )? {
                    value
                } else {
                    self.decode_binascii_into(py, input, output)?
                }
            }
        };
        self.finish(py, input, value)
    }

    fn decode_advanced_into(
        &self,
        py: Python<'_>,
        input: &BytesLike<'_, '_>,
        output: &Bound<'_, PyByteArray>,
        shortcut: AdvancedShortcut,
    ) -> PyResult<usize> {
        if shortcut == AdvancedShortcut::StandardStrict {
            let canonical_input = !self.policy.canonical
                || unsafe {
                    input.with_bytes(|input| {
                        let padding = usize::from(input.ends_with(b"="))
                            + usize::from(input.ends_with(b"=="));
                        let data = &input[..input.len().saturating_sub(padding)];
                        data.last().is_none_or(|last| {
                            !STANDARD_ALPHABET.contains(last) || canonical_padding(data)
                        })
                    })
                };
            if canonical_input
                && let Ok(value) = decode_strict_into(
                    input,
                    output,
                    DecodeAlphabet::Standard,
                    ErrorWrites::PreserveOutput,
                )?
            {
                return Ok(value);
            }
        }
        if shortcut == AdvancedShortcut::CanonicalUnpadded
            && unsafe {
                input.with_bytes(|input| canonical_unpadded_input(input, self.policy.altchars))
            }
            && let Ok(value) = decode_unpadded_into_with_altchars(
                input,
                output,
                self.policy.altchars,
                || self.strict_custom(),
                ErrorWrites::PreserveOutput,
            )?
        {
            return Ok(value);
        }
        decode_advanced_into(py, input, output, self.advanced(), self.semantics)
    }

    fn decode_strict_into(
        &self,
        py: Python<'_>,
        input: &BytesLike<'_, '_>,
        output: &Bound<'_, PyByteArray>,
    ) -> PyResult<usize> {
        let result = if self.policy.padding == Padding::Padded {
            decode_strict_into_with_altchars(
                input,
                output,
                self.policy.altchars,
                || self.strict_custom(),
                self.policy.error_writes(),
            )?
        } else {
            decode_unpadded_into_with_altchars(
                input,
                output,
                self.policy.altchars,
                || self.strict_custom(),
                self.policy.error_writes(),
            )?
        };
        if let Some(value) = accept_into(result, self.policy.store_bounds())? {
            Ok(value)
        } else {
            self.decode_binascii_into(py, input, output)
        }
    }

    fn try_unpadded_into(
        &self,
        input: &BytesLike<'_, '_>,
        output: &Bound<'_, PyByteArray>,
    ) -> PyResult<Option<usize>> {
        accept_into(
            decode_unpadded_into_with_altchars(
                input,
                output,
                self.policy.altchars,
                || self.strict_custom(),
                self.policy.error_writes(),
            )?,
            self.policy.store_bounds(),
        )
    }

    fn direct_alphabet(&self) -> DecodeAlphabet {
        match self.policy.altchars {
            None => DecodeAlphabet::Standard,
            Some([b'-', b'_']) => DecodeAlphabet::Mixed,
            Some(_) => unreachable!("the prepared route limits direct lenient decoding"),
        }
    }

    fn decode_binascii<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        decode_with_binascii(
            py,
            self.semantics,
            input,
            self.policy.altchars,
            self.policy.validation,
            self.policy.padding,
        )
    }

    fn decode_binascii_into(
        &self,
        py: Python<'_>,
        input: &BytesLike<'_, '_>,
        output: &Bound<'_, PyByteArray>,
    ) -> PyResult<usize> {
        let decoded = self.decode_binascii(py, input)?;
        copy_decoded_into(&decoded, output)
    }

    fn finish<T>(&self, py: Python<'_>, input: &BytesLike<'_, '_>, value: T) -> PyResult<T> {
        warn_legacy_altchars(
            py,
            self.semantics,
            input,
            self.policy.altchars,
            self.policy.ignorechars_specified,
            self.policy.validation,
        )?;
        Ok(value)
    }
}

fn accept_into(result: Result<usize, Base64Error>, bounds: StoreBounds) -> PyResult<Option<usize>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Base64Error::OutputTooSmall { required, provided })
            if bounds == StoreBounds::ReportImmediately =>
        {
            Err(output_too_small(required, provided))
        }
        Err(Base64Error::OutputTooSmall { .. } | Base64Error::InvalidInput) => Ok(None),
    }
}

fn canonical_unpadded_input(input: &[u8], altchars: Option<[u8; 2]>) -> bool {
    let remainder = input.len() % 4;
    if !matches!(remainder, 2 | 3) {
        return remainder == 0;
    }
    let last = input[input.len() - 1];
    let value = match altchars {
        Some([_, slash]) if last == slash => Some(63),
        Some([plus, _]) if last == plus => Some(62),
        _ => STANDARD_ALPHABET.iter().position(|&byte| byte == last),
    };
    value.is_some_and(|value| {
        if remainder == 2 {
            value & 0x0f == 0
        } else {
            value & 0x03 == 0
        }
    })
}

fn is_mime_whitespace_input(input: &[u8], mixed_alphabet: bool) -> bool {
    let mut saw_whitespace = false;
    for &byte in input {
        if matches!(byte, b'\r' | b'\n' | b' ') {
            saw_whitespace = true;
        } else if !(byte.is_ascii_alphanumeric()
            || matches!(byte, b'+' | b'/' | b'=')
            || (mixed_alphabet && matches!(byte, b'-' | b'_')))
        {
            return false;
        }
    }
    saw_whitespace
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

fn copy_decoded_into(
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
