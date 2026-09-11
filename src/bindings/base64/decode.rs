//! Python decoding entry points, ordered native attempts, and CPython fallback.

use core::slice;

use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyAssertionError, PyDeprecationWarning, PyFutureWarning, PyValueError};
use pyo3::ffi;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyType};

use super::configured::{
    ConfiguredDecoder, decode_configured, decode_configured_into, decode_configured_strict_into,
};
use super::lenient::{try_decode_lenient, try_decode_lenient_into};
use super::policy::{
    ConfiguredShortcut, DecodeAttempt, DecodePolicy, DecodeRoute, ErrorWrites, Padding,
    PreparedDecoder, Validation,
};
use super::scan::is_lenient_symbol;
use super::staging::{output_too_small, with_output_ptr};
use super::strict::{
    decode_strict, decode_strict_into, decode_unpadded, decode_unpadded_into, translate_altchars,
};
use crate::base64::{Base64Error, DecodeAlphabet, STANDARD_ALPHABET};
use crate::bindings::buffer::{
    BytesLike, ascii_or_bytes, contiguous_bytes_like, contiguous_bytes_like_exported,
    decode_data_object,
};
use crate::bindings::compatibility::{PythonSemantics, parse_altchars, python_at_least};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;
use crate::bindings::schema::Argument;

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
        prepared.with_lenient_table(|table| {
            try_decode_lenient(
                py,
                input,
                prepared.policy.altchars,
                prepared.policy.padding,
                table,
                prepared.semantics,
            )
        })
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
        prepared.with_lenient_table(|table| {
            try_decode_lenient_into(
                input,
                self,
                prepared.policy.altchars,
                prepared.policy.padding,
                table,
                prepared.semantics,
            )
        })
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
            let padded = if direct && {
                let length = input.len();
                length.is_multiple_of(4)
                    && (length < BASE64_DETACH_THRESHOLD || !self.has_edge_noise(input))
            } {
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

    fn has_edge_noise(&self, input: &BytesLike<'_, '_>) -> bool {
        // A noisy edge rules out a successful strict probe without walking the
        // whole input. Clean edges still go through full native validation.
        unsafe {
            input.with_bytes(|input| {
                input[..64]
                    .iter()
                    .chain(&input[input.len() - 64..])
                    .any(|&byte| byte != b'=' && !is_lenient_symbol(byte, self.policy.altchars))
            })
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

fn invalid_altchars(py: Python<'_>, altchars: &Bound<'_, PyAny>) -> PyErr {
    let representation = match altchars.repr() {
        Ok(value) => value.to_string(),
        Err(error) => return error,
    };
    if python_at_least(py, (3, 15)) {
        PyValueError::new_err(format!("invalid altchars: {representation}"))
    } else {
        PyAssertionError::new_err(representation)
    }
}

fn normalized_altchars<'py>(
    py: Python<'py>,
    altchars: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let altchars = decode_data_object(py, altchars, "altchars")?;
    if altchars.len()? != 2 {
        return Err(invalid_altchars(py, &altchars));
    }
    Ok(altchars)
}

fn construct_decode_alphabet<'py>(
    py: Python<'py>,
    altchars: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let prefix = PyBytes::new(py, &STANDARD_ALPHABET[..62]);
    unsafe {
        Bound::from_owned_ptr_or_err(py, ffi::PyNumber_Add(prefix.as_ptr(), altchars.as_ptr()))
    }
}

struct ParsedDecodeAlphabet {
    altchars: Option<[u8; 2]>,
    full: Option<[u8; 64]>,
}

fn parse_decode_alphabet(alphabet: &Bound<'_, PyAny>) -> PyResult<ParsedDecodeAlphabet> {
    let alphabet = alphabet.cast::<PyBytes>()?;
    if alphabet.as_bytes().len() != STANDARD_ALPHABET.len() {
        return Err(PyValueError::new_err("alphabet must have length 64"));
    }

    let mut table = [0; 64];
    table.copy_from_slice(alphabet.as_bytes());
    if table[..62] == STANDARD_ALPHABET[..62] {
        let altchars = [table[62], table[63]];
        Ok(ParsedDecodeAlphabet {
            altchars: (altchars != *b"+/").then_some(altchars),
            full: None,
        })
    } else {
        Ok(ParsedDecodeAlphabet {
            altchars: None,
            full: Some(table),
        })
    }
}

fn translation_table<'py>(py: Python<'py>, [plus, slash]: [u8; 2]) -> Bound<'py, PyBytes> {
    let mut table: [u8; 256] = std::array::from_fn(|byte| byte as u8);
    table[usize::from(plus)] = b'+';
    table[usize::from(slash)] = b'/';
    PyBytes::new(py, &table)
}

fn prepare_translated_input<'py>(
    py: Python<'py>,
    input: Bound<'py, PyAny>,
    altchars: [u8; 2],
) -> PyResult<(Bound<'py, PyAny>, Option<[u8; 2]>)> {
    if PyBytes::is_exact_type_of(&input) {
        return Ok((input, Some(altchars)));
    }
    if PyByteArray::is_exact_type_of(&input) {
        let bytes = contiguous_bytes_like(&input, "s")?;
        let input = unsafe { bytes.with_bytes(|bytes| PyBytes::new(py, bytes).into_any()) };
        return Ok((input, Some(altchars)));
    }

    let table = translation_table(py, altchars);
    let input = input.call_method1(intern!(py, "translate"), (table,))?;
    Ok((input, None))
}

struct DecodeArguments<'a, 'py> {
    altchars: Option<&'a Bound<'py, PyAny>>,
    validate: Argument,
    padded: Argument,
    ignorechars: Option<&'a Bound<'py, PyAny>>,
    canonical: Argument,
}

fn b64decode_with<'py, T>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    arguments: DecodeArguments<'_, 'py>,
    decode: impl FnOnce(&PreparedDecoder, &BytesLike<'_, 'py>) -> PyResult<T>,
) -> PyResult<T> {
    let mut input = decode_data_object(py, s, "s")?;
    let mut parsed_altchars = None;
    let mut constructed_alphabet = None;

    if let Some(altchars) = arguments.altchars {
        let altchars = normalized_altchars(py, altchars)?;
        if python_at_least(py, (3, 15)) && arguments.ignorechars.is_some() {
            constructed_alphabet = Some(construct_decode_alphabet(py, &altchars)?);
        } else {
            parsed_altchars = parse_altchars(py, Some(&altchars), true)?;
            if arguments.ignorechars.is_none() {
                (input, parsed_altchars) =
                    prepare_translated_input(py, input, parsed_altchars.unwrap_or(*b"+/"))?;
            }
        }
    }

    let input = contiguous_bytes_like_exported(&input, "s")?;
    let validate = arguments.validate.optional_truthy(py)?;
    let alphabet = constructed_alphabet
        .as_ref()
        .map(parse_decode_alphabet)
        .transpose()?;
    if let Some(alphabet) = &alphabet {
        parsed_altchars = alphabet.altchars;
    }
    let padded = arguments.padded.truthy(py)?;
    let ignorechars = arguments
        .ignorechars
        .map(|ignorechars| contiguous_bytes_like_exported(ignorechars, "ignorechars"))
        .transpose()?;
    let copied_ignorechars = ignorechars.as_ref().map(|ignorechars| unsafe {
        ignorechars.with_bytes(|ignorechars| PyBytes::new(py, ignorechars))
    });
    let canonical = arguments.canonical.truthy(py)?;
    let policy = DecodePolicy::new(
        parsed_altchars,
        validate,
        padded,
        copied_ignorechars.as_ref().map(Bound::as_any),
        canonical,
    )
    .with_alphabet(alphabet.and_then(|alphabet| alphabet.full));
    let decoder = PreparedDecoder::new(py, policy)?;
    decode(&decoder, &input)
}

pub(super) fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: Argument,
    padded: Argument,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: Argument,
) -> PyResult<Bound<'py, PyBytes>> {
    b64decode_with(
        py,
        s,
        DecodeArguments {
            altchars,
            validate,
            padded,
            ignorechars,
            canonical,
        },
        |decoder, input| decoder.decode_allocating(py, input),
    )
}

/// Decode with the standard Base64 alphabet.
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    PreparedDecoder::new(py, DecodePolicy::standard())?.decode_allocating(py, &input)
}

/// Decode standard Base64 into a reusable output.
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    PreparedDecoder::new(py, DecodePolicy::standard())?.decode_into(py, &input, output)
}

fn urlsafe_b64decode_with<'py, T>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: Argument,
    decode: impl FnOnce(&PreparedDecoder, &BytesLike<'_, 'py>) -> PyResult<T>,
) -> PyResult<T> {
    let input = decode_data_object(py, s, "s")?;
    let (input, altchars) = prepare_translated_input(py, input, *b"-_")?;
    let input = contiguous_bytes_like_exported(&input, "s")?;
    let padded = padded.truthy(py)?;
    let decoder = PreparedDecoder::new(
        py,
        DecodePolicy::new(altchars, Some(false), padded, None, false),
    )?;
    decode(&decoder, &input)
}

pub(super) fn urlsafe_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: Argument,
) -> PyResult<Bound<'py, PyBytes>> {
    urlsafe_b64decode_with(py, s, padded, |decoder, input| {
        decoder.decode_allocating(py, input)
    })
}

pub(super) fn urlsafe_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: Argument,
) -> PyResult<usize> {
    urlsafe_b64decode_with(py, s, padded, |decoder, input| {
        decoder.decode_into(py, input, output)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    validate: Argument,
    padded: Argument,
    ignorechars: Option<&Bound<'_, PyAny>>,
    canonical: Argument,
) -> PyResult<usize> {
    b64decode_with(
        py,
        s,
        DecodeArguments {
            altchars,
            validate,
            padded,
            ignorechars,
            canonical,
        },
        |decoder, input| decoder.decode_into(py, input, output),
    )
}

fn decoding_error(py: Python<'_>, message: &'static str) -> PyErr {
    match py
        .import("binascii")
        .and_then(|module| module.getattr("Error"))
        .and_then(|value| value.cast_into::<PyType>().map_err(Into::into))
    {
        Ok(error_type) => PyErr::from_type(error_type, (message,)),
        Err(error) => error,
    }
}

#[inline]
fn warn_legacy_altchars(
    py: Python<'_>,
    semantics: PythonSemantics,
    input: &BytesLike<'_, '_>,
    altchars: Option<[u8; 2]>,
    ignorechars_specified: bool,
    validation: Validation,
) -> PyResult<()> {
    if ignorechars_specified {
        return Ok(());
    }
    let Some(altchars) = altchars else {
        return Ok(());
    };
    if !semantics.warns_legacy_altchars {
        return Ok(());
    }
    let badchar = unsafe {
        input.with_bytes(|input| {
            b"+/"
                .iter()
                .copied()
                .find(|byte| !altchars.contains(byte) && input.contains(byte))
        })
    };
    let Some(badchar) = badchar else {
        return Ok(());
    };
    let strict_mode = validation.is_strict();
    let mode = if strict_mode { "True" } else { "False" };
    let outcome = if strict_mode {
        "will be an error"
    } else {
        "will be discarded"
    };
    let altchars = PyBytes::new(py, &altchars).repr()?.to_string();
    let message = format!(
        "invalid character '{}' in Base64 data with altchars={altchars} and validate={mode} {outcome} in future Python versions",
        char::from(badchar),
    );
    let category = if strict_mode {
        py.get_type::<PyDeprecationWarning>()
    } else {
        py.get_type::<PyFutureWarning>()
    };
    py.import("warnings")?
        .call_method1("warn", (message, category, 1))?;
    Ok(())
}

fn decode_with_binascii<'py>(
    py: Python<'py>,
    semantics: PythonSemantics,
    input: &BytesLike<'_, 'py>,
    altchars: Option<[u8; 2]>,
    validation: Validation,
    padding: Padding,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_with_binascii(
            py,
            semantics,
            &BytesLike::OwnedVec(input),
            altchars,
            validation,
            padding,
        );
    }
    let translated = if let Some(altchars) = altchars {
        unsafe { input.with_bytes(|input| translate_altchars(input, altchars)) }?
    } else {
        None
    };
    let data = if let Some(translated) = translated.as_deref() {
        PyBytes::new(py, translated)
    } else if let Some(bytes) = input.python_bytes(py)? {
        bytes
    } else {
        unsafe { input.with_bytes(|input| PyBytes::new(py, input)) }
    };
    let input = data.as_bytes();
    let decode = py
        .import(intern!(py, "binascii"))?
        .getattr(intern!(py, "a2b_base64"))?;
    let strict_mode = validation.is_strict();
    let output = if semantics.binascii_accepts_padding() {
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict_mode", strict_mode)?;
        kwargs.set_item("padded", padding.is_padded())?;
        decode.call((data,), Some(&kwargs))?
    } else if !semantics.binascii_accepts_strict_mode() {
        if strict_mode && !strict_base64_310(input) {
            return Err(decoding_error(py, "Non-base64 digit found"));
        }
        decode.call1((data,))?
    } else {
        let kwargs = PyDict::new(py);
        kwargs.set_item("strict_mode", strict_mode)?;
        decode.call((data,), Some(&kwargs))?
    };
    output.cast_into::<PyBytes>().map_err(Into::into)
}

fn canonical_padding(input: &[u8]) -> bool {
    let Some(&last) = input.last() else {
        return true;
    };
    let value = STANDARD_ALPHABET
        .iter()
        .position(|&byte| byte == last)
        .expect("normalized Base64 input uses the standard alphabet");
    match input.len() % 4 {
        2 => value & 0x0f == 0,
        3 => value & 0x03 == 0,
        _ => true,
    }
}

fn strict_base64_310(input: &[u8]) -> bool {
    let padding = input
        .iter()
        .position(|&byte| byte == b'=')
        .unwrap_or(input.len());
    input[..padding]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
        && input[padding..].len() <= 2
        && input[padding..].iter().all(|&byte| byte == b'=')
}
