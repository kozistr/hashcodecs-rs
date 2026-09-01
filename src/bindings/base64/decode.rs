use pyo3::exceptions::PyMemoryError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes};

use super::{STANDARD_ALPHABET, output_too_small, parse_altchars};
use crate::base64::{Base64Error, DecodeAlphabet};
use crate::bindings::buffer::{BytesLike, ascii_or_bytes};

use self::fallback::{canonical_padding, decode_with_binascii, decoding_error};
use self::native::{
    decode_advanced, decode_advanced_into, decode_strict, decode_strict_into,
    decode_strict_into_with_altchars, decode_strict_with_altchars,
    decode_unpadded_into_with_altchars, decode_unpadded_with_altchars, try_decode_lenient,
    try_decode_lenient_into, try_decode_strict, try_decode_urlsafe_315,
    try_decode_urlsafe_315_into,
};
use self::output::copy_decoded_into;
use self::plan::{
    DecodeOptions, DecodePlan, DecodeStrategy, execute_decode_strategy,
    execute_strict_decode_strategy,
};

mod batch;
mod fallback;
pub(super) mod native;
mod output;
mod plan;

pub(super) use self::batch::{
    b64decode_batch, b64decode_batch_into, standard_b64decode_batch, standard_b64decode_batch_into,
    urlsafe_b64decode_batch, urlsafe_b64decode_batch_into,
};

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

fn decode_plan_allocating_inner<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
    options: DecodeOptions<'_, 'py>,
    skips_legacy_warning: &mut bool,
) -> PyResult<Bound<'py, PyBytes>> {
    macro_rules! execute_strict_attempt {
        (Urlsafe315) => {
            if let Some(output) = try_decode_urlsafe_315(py, input, true, options.padded)? {
                *skips_legacy_warning = true;
                return Ok(output);
            }
        };
        (Strict) => {
            match decode_strict_with_altchars(py, input, options.altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        };
        (Unpadded) => {
            match decode_unpadded_with_altchars(py, input, options.altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        };
        (Binascii) => {
            return decode_with_binascii(py, input, options.altchars, true, options.padded);
        };
    }
    execute_strict_decode_strategy!(py, options, execute_strict_attempt);
    let strategy = DecodeStrategy::select(py, options);
    decode_plan_allocating_routed(py, input, options, strategy, skips_legacy_warning)
}

fn decode_plan_allocating_routed<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
    options: DecodeOptions<'_, 'py>,
    strategy: DecodeStrategy,
    skips_legacy_warning: &mut bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let DecodeOptions {
        altchars,
        padded,
        canonical,
        ..
    } = options;
    let strict_mode = options.strict_mode();

    macro_rules! execute_attempt {
        (Urlsafe315) => {
            if let Some(output) = try_decode_urlsafe_315(py, input, strict_mode, padded)? {
                *skips_legacy_warning = true;
                return Ok(output);
            }
        };
        (StandardStrict) => {
            match decode_strict(py, input, DecodeAlphabet::Standard) {
                Ok(output) => {
                    let canonical_input = !canonical
                        || unsafe {
                            input.with_bytes(|input| {
                                // A successful strict decode guarantees that padding is
                                // confined to the final two bytes.
                                let padding = usize::from(input.ends_with(b"="))
                                    + usize::from(input.ends_with(b"=="));
                                let data_len = input.len() - padding;
                                canonical_padding(&input[..data_len])
                            })
                        };
                    if canonical_input {
                        return Ok(output);
                    }
                    return Err(decoding_error(py, "Non-zero padding bits"));
                }
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        };
        (Strict) => {
            match decode_strict_with_altchars(py, input, altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                    return Err(error);
                }
                Err(_) => {}
            }
        };
        (StrictProbe) => {
            let alphabet = match altchars {
                None => DecodeAlphabet::Standard,
                Some([b'-', b'_']) => DecodeAlphabet::Mixed,
                Some(_) => unreachable!("the strategy limits direct lenient decoding"),
            };
            if let Some(output) = try_decode_strict(py, input, alphabet)? {
                return Ok(output);
            }
        };
        (MimeWhitespace) => {
            if unsafe {
                input.with_bytes(|input| is_mime_whitespace_input(input, altchars == Some(*b"-_")))
            } {
                return decode_advanced(py, input, options);
            }
        };
        (Unpadded) => {
            match decode_unpadded_with_altchars(py, input, altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        };
        (Lenient) => {
            if let Some(output) = try_decode_lenient(py, input, altchars, padded)? {
                return Ok(output);
            }
        };
        (CanonicalUnpadded) => {
            if unsafe { input.with_bytes(|input| canonical_unpadded_input(input, altchars)) } {
                match decode_unpadded_with_altchars(py, input, altchars) {
                    Ok(output) => return Ok(output),
                    Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                        return Err(error);
                    }
                    Err(_) => {}
                }
            }
        };
        (Advanced) => {
            return decode_advanced(py, input, options);
        };
        (Binascii) => {
            return decode_with_binascii(py, input, altchars, strict_mode, padded);
        };
    }

    execute_decode_strategy!(strategy, execute_attempt)
}

pub(super) fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: Option<bool>,
    padded: bool,
    ignorechars: Option<&Bound<'py, PyAny>>,
    canonical: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let options = DecodeOptions::new(altchars, validate, padded, ignorechars, canonical);
    DecodePlan::new(&input, options).execute_allocating(py)
}

/// Decode with the standard Base64 alphabet.
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::standard()).execute_allocating(py)
}

/// Decode standard Base64 into a reusable output.
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::standard()).execute_into(py, output)
}

pub(super) fn urlsafe_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::urlsafe(padded)).execute_allocating(py)
}

pub(super) fn urlsafe_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::urlsafe(padded)).execute_into(py, output)
}

fn decode_plan_into_inner<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
    output: &Bound<'py, PyByteArray>,
    options: DecodeOptions<'_, '_>,
    skips_legacy_warning: &mut bool,
) -> PyResult<usize> {
    macro_rules! execute_strict_attempt {
        (Urlsafe315) => {
            if let Some(written) = try_decode_urlsafe_315_into(input, output, true, options.padded)?
            {
                *skips_legacy_warning = true;
                return Ok(written);
            }
        };
        (Strict) => {
            match decode_strict_into_with_altchars(py, input, output, options.altchars, false)? {
                Ok(written) => return Ok(written),
                Err(Base64Error::OutputTooSmall { required, provided }) => {
                    return Err(output_too_small(required, provided));
                }
                Err(Base64Error::InvalidInput) => {}
            }
        };
        (Unpadded) => {
            match decode_unpadded_into_with_altchars(py, input, output, options.altchars, false)? {
                Ok(written) => return Ok(written),
                Err(Base64Error::OutputTooSmall { required, provided }) => {
                    return Err(output_too_small(required, provided));
                }
                Err(Base64Error::InvalidInput) => {}
            }
        };
        (Binascii) => {
            let decoded = decode_with_binascii(py, input, options.altchars, true, options.padded)?;
            return copy_decoded_into(&decoded, output);
        };
    }
    execute_strict_decode_strategy!(py, options, execute_strict_attempt);
    let strategy = DecodeStrategy::select(py, options);
    decode_plan_into_routed(py, input, output, options, strategy, skips_legacy_warning)
}

fn decode_plan_into_routed<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
    output: &Bound<'py, PyByteArray>,
    options: DecodeOptions<'_, '_>,
    strategy: DecodeStrategy,
    skips_legacy_warning: &mut bool,
) -> PyResult<usize> {
    let DecodeOptions {
        altchars,
        padded,
        canonical,
        ..
    } = options;
    let strict_mode = options.strict_mode();
    let transactional_errors = !strict_mode;

    macro_rules! execute_attempt {
        (Urlsafe315) => {
            if let Some(written) = try_decode_urlsafe_315_into(input, output, strict_mode, padded)?
            {
                *skips_legacy_warning = true;
                return Ok(written);
            }
        };
        (StandardStrict) => {
            let canonical_input = !canonical
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
                && let Ok(written) = decode_strict_into(
                    input,
                    output,
                    DecodeAlphabet::Standard,
                    // Any prefix written before a rejected attempt is fully
                    // validated and safe for the advanced fallback to overwrite.
                    true,
                )?
            {
                return Ok(written);
            }
        };
        (Strict) => {
            match decode_strict_into_with_altchars(py, input, output, altchars, false)? {
                Ok(written) => return Ok(written),
                Err(Base64Error::OutputTooSmall { required, provided }) => {
                    return Err(output_too_small(required, provided));
                }
                Err(Base64Error::InvalidInput) => {}
            }
        };
        (StrictProbe) => {
            match decode_strict_into_with_altchars(py, input, output, altchars, true)? {
                Ok(written) => return Ok(written),
                Err(Base64Error::OutputTooSmall { .. } | Base64Error::InvalidInput) => {}
            }
        };
        (MimeWhitespace) => {
            if unsafe {
                input.with_bytes(|input| is_mime_whitespace_input(input, altchars == Some(*b"-_")))
            } {
                return decode_advanced_into(py, input, output, options);
            }
        };
        (Unpadded) => {
            match decode_unpadded_into_with_altchars(
                py,
                input,
                output,
                altchars,
                transactional_errors,
            )? {
                Ok(written) => return Ok(written),
                Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                    return Err(output_too_small(required, provided));
                }
                Err(Base64Error::OutputTooSmall { .. } | Base64Error::InvalidInput) => {}
            }
        };
        (Lenient) => {
            if let Some(written) = try_decode_lenient_into(py, input, output, altchars, padded)? {
                return Ok(written);
            }
        };
        (CanonicalUnpadded) => {
            if unsafe { input.with_bytes(|input| canonical_unpadded_input(input, altchars)) }
                && let Ok(written) =
                    decode_unpadded_into_with_altchars(py, input, output, altchars, true)?
            {
                return Ok(written);
            }
        };
        (Advanced) => {
            return decode_advanced_into(py, input, output, options);
        };
        (Binascii) => {
            let decoded = decode_with_binascii(py, input, altchars, strict_mode, padded)?;
            return copy_decoded_into(&decoded, output);
        };
    }

    execute_decode_strategy!(strategy, execute_attempt)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    validate: Option<bool>,
    padded: bool,
    ignorechars: Option<&Bound<'_, PyAny>>,
    canonical: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    let options = DecodeOptions::new(altchars, validate, padded, ignorechars, canonical);
    DecodePlan::new(&input, options).execute_into(py, output)
}
