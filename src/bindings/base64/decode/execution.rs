use core::{ptr, slice};

use pyo3::exceptions::PyMemoryError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::{STANDARD_ALPHABET, output_too_small, with_output_ptr};
use super::configured::{
    ConfiguredDecoder, decode_configured, decode_configured_into, decode_configured_strict_into,
};
use super::fallback::{
    canonical_padding, decode_with_binascii, decoding_error, warn_legacy_altchars,
};
use super::lenient::{try_decode_lenient, try_decode_lenient_into};
use super::policy::{
    ConfiguredShortcut, DecodeAttempt, DecodeRoute, ErrorWrites, Padding, PreparedDecoder,
};
use super::strict::{decode_strict, decode_strict_into, decode_unpadded, decode_unpadded_into};
use crate::base64::{Base64Error, DecodeAlphabet};
use crate::bindings::buffer::BytesLike;

#[derive(Clone, Copy)]
enum NativeDecoder<'a> {
    Direct(DecodeAlphabet, Padding),
    CustomStrict(&'a ConfiguredDecoder, [u8; 2]),
    Configured(&'a ConfiguredDecoder),
}

// Storage owns allocation and writes; PreparedDecoder owns the order of attempts.
trait DecodeOutput<'py> {
    type Value;
    const REUSABLE: bool;

    fn native(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        decoder: NativeDecoder<'_>,
        prepared: &PreparedDecoder,
        writes: ErrorWrites,
    ) -> PyResult<Result<Self::Value, Base64Error>>;

    fn lenient(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        prepared: &PreparedDecoder,
    ) -> PyResult<Result<Self::Value, Base64Error>>;

    fn store_fallback(&self, bytes: Bound<'py, PyBytes>) -> PyResult<Self::Value>;
}

struct Allocating;

impl<'py> DecodeOutput<'py> for Allocating {
    type Value = Bound<'py, PyBytes>;
    const REUSABLE: bool = false;

    fn native(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        decoder: NativeDecoder<'_>,
        prepared: &PreparedDecoder,
        _writes: ErrorWrites,
    ) -> PyResult<Result<Self::Value, Base64Error>> {
        match decoder {
            NativeDecoder::Direct(alphabet, Padding::Padded) => decode_strict(py, input, alphabet),
            NativeDecoder::Direct(alphabet, Padding::Unpadded) => {
                decode_unpadded(py, input, alphabet)
            }
            NativeDecoder::CustomStrict(decoder, _) | NativeDecoder::Configured(decoder) => {
                decode_configured(py, input, decoder, prepared.semantics)
            }
        }
    }

    fn lenient(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        prepared: &PreparedDecoder,
    ) -> PyResult<Result<Self::Value, Base64Error>> {
        try_decode_lenient(
            py,
            input,
            prepared.policy.altchars,
            prepared.policy.padding,
            prepared.lenient_table(),
            prepared.semantics,
        )
    }

    fn store_fallback(&self, bytes: Bound<'py, PyBytes>) -> PyResult<Self::Value> {
        Ok(bytes)
    }
}

impl<'py> DecodeOutput<'py> for Bound<'py, PyByteArray> {
    type Value = usize;
    const REUSABLE: bool = true;

    fn native(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        decoder: NativeDecoder<'_>,
        prepared: &PreparedDecoder,
        writes: ErrorWrites,
    ) -> PyResult<Result<usize, Base64Error>> {
        match decoder {
            NativeDecoder::Direct(alphabet, Padding::Padded) => {
                decode_strict_into(input, self, alphabet, writes)
            }
            NativeDecoder::Direct(alphabet, Padding::Unpadded) => {
                decode_unpadded_into(input, self, alphabet, writes)
            }
            NativeDecoder::CustomStrict(decoder, altchars) => {
                decode_configured_strict_into(input, self, altchars, decoder, writes)
            }
            NativeDecoder::Configured(decoder) => {
                decode_configured_into(py, input, self, decoder, prepared.semantics)
            }
        }
    }

    fn lenient(
        &self,
        _py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        prepared: &PreparedDecoder,
    ) -> PyResult<Result<usize, Base64Error>> {
        try_decode_lenient_into(
            input,
            self,
            prepared.policy.altchars,
            prepared.policy.padding,
            prepared.lenient_table(),
            prepared.semantics,
        )
    }

    fn store_fallback(&self, bytes: Bound<'py, PyBytes>) -> PyResult<usize> {
        copy_decoded_into(&bytes, self)
    }
}

impl PreparedDecoder {
    pub(super) fn decode_allocating<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        // All attempts and the warning scan must observe one stable input.
        #[cfg(Py_GIL_DISABLED)]
        if let Some(input) = input.snapshot_mutable()? {
            return self.execute(py, &BytesLike::OwnedVec(input), &Allocating);
        }
        self.execute(py, input, &Allocating)
    }

    pub(super) fn decode_into<'py>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        output: &Bound<'py, PyByteArray>,
    ) -> PyResult<usize> {
        #[cfg(Py_GIL_DISABLED)]
        if let Some(input) = input.snapshot_mutable()? {
            return self.execute(py, &BytesLike::OwnedVec(input), output);
        }
        if let Some(input) = input.snapshot_for_output(output)? {
            return self.execute(py, &BytesLike::OwnedVec(input), output);
        }
        self.execute(py, input, output)
    }

    fn execute<'py, O: DecodeOutput<'py>>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        output: &O,
    ) -> PyResult<O::Value> {
        let (urlsafe_315, direct, strict) = match self.route {
            DecodeRoute::Configured(shortcut) => {
                let value = self.configured_output(py, input, output, shortcut)?;
                return self.finish(py, input, value);
            }
            DecodeRoute::Strict { urlsafe_315 } => (urlsafe_315, false, true),
            DecodeRoute::LenientDirect { urlsafe_315 } => (urlsafe_315, true, false),
            DecodeRoute::LenientCustom => (false, false, false),
        };

        if urlsafe_315 {
            if (self.policy.padding.is_padded() || !strict)
                && let Some(value) = self.try_native(
                    py,
                    input,
                    output,
                    NativeDecoder::Direct(DecodeAlphabet::UrlSafe, Padding::Padded),
                    self.attempt,
                )?
            {
                return Ok(value);
            }
            if !self.policy.padding.is_padded()
                && let Some(value) = self.try_native(
                    py,
                    input,
                    output,
                    NativeDecoder::Direct(DecodeAlphabet::UrlSafe, Padding::Unpadded),
                    self.attempt,
                )?
            {
                return Ok(value);
            }
        }

        let value = if strict {
            self.try_native(
                py,
                input,
                output,
                self.strict_decoder(self.policy.padding),
                self.attempt,
            )?
        } else {
            let padded = if direct {
                self.try_native(
                    py,
                    input,
                    output,
                    self.strict_decoder(Padding::Padded),
                    DecodeAttempt::Probe,
                )?
            } else {
                None
            };
            if padded.is_some() {
                padded
            } else {
                let unpadded = if self.policy.padding == Padding::Unpadded {
                    self.try_native(
                        py,
                        input,
                        output,
                        self.strict_decoder(Padding::Unpadded),
                        self.attempt,
                    )?
                } else {
                    None
                };
                if unpadded.is_some() {
                    unpadded
                } else {
                    DecodeAttempt::Strict
                        .accept(output.lenient(py, input, self)?)
                        .map_err(|error| native_error(py, error))?
                }
            }
        };
        let value = match value {
            Some(value) => value,
            None => output.store_fallback(decode_with_binascii(
                py,
                self.semantics,
                input,
                self.policy.altchars,
                self.policy.validation,
                self.policy.padding,
            )?)?,
        };
        self.finish(py, input, value)
    }

    fn strict_decoder(&self, padding: Padding) -> NativeDecoder<'_> {
        match self.policy.altchars {
            None => NativeDecoder::Direct(DecodeAlphabet::Standard, padding),
            Some([b'-', b'_']) => NativeDecoder::Direct(DecodeAlphabet::Mixed, padding),
            Some(altchars) => NativeDecoder::CustomStrict(self.strict_custom(), altchars),
        }
    }

    fn try_native<'py, O: DecodeOutput<'py>>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        output: &O,
        decoder: NativeDecoder<'_>,
        attempt: DecodeAttempt,
    ) -> PyResult<Option<O::Value>> {
        attempt
            .accept(output.native(py, input, decoder, self, attempt.error_writes())?)
            .map_err(|error| native_error(py, error))
    }

    fn configured_output<'py, O: DecodeOutput<'py>>(
        &self,
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        output: &O,
        shortcut: ConfiguredShortcut,
    ) -> PyResult<O::Value> {
        if shortcut == ConfiguredShortcut::StandardStrict {
            // Reusable output must reject noncanonical bits before writing. Allocating
            // output retains its specific padding-bit error after alphabet validation.
            let canonical = !self.policy.canonical
                || unsafe {
                    input.with_bytes(|input| {
                        let padding = usize::from(input.ends_with(b"="))
                            + usize::from(input.ends_with(b"=="));
                        let data = &input[..input.len() - padding];
                        data.last().is_none_or(|last| {
                            !STANDARD_ALPHABET.contains(last) || canonical_padding(data)
                        })
                    })
                };
            if (!O::REUSABLE || canonical)
                && let Some(value) = self.try_native(
                    py,
                    input,
                    output,
                    NativeDecoder::Direct(DecodeAlphabet::Standard, Padding::Padded),
                    DecodeAttempt::Probe,
                )?
            {
                if canonical {
                    return Ok(value);
                }
                return Err(decoding_error(py, "Non-zero padding bits"));
            }
        }
        if shortcut == ConfiguredShortcut::CanonicalUnpadded
            && unsafe {
                input.with_bytes(|input| canonical_unpadded_input(input, self.policy.altchars))
            }
            && let Some(value) = self.try_native(
                py,
                input,
                output,
                self.strict_decoder(Padding::Unpadded),
                DecodeAttempt::Probe,
            )?
        {
            return Ok(value);
        }
        output
            .native(
                py,
                input,
                NativeDecoder::Configured(self.configured()),
                self,
                ErrorWrites::ValidatedPrefix,
            )?
            .map_err(|error| native_error(py, error))
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

fn native_error(py: Python<'_>, error: Base64Error) -> PyErr {
    match error {
        Base64Error::InvalidInput => decoding_error(py, "Incorrect padding"),
        Base64Error::OutputTooSmall { required, provided } => output_too_small(required, provided),
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
