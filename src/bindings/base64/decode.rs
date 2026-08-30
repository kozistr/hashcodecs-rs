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
    decode_unpadded_into_with_altchars, decode_unpadded_with_altchars,
    lenient_continues_after_padding, normalize_mime_whitespace, try_decode_lenient,
    try_decode_lenient_into, try_decode_strict,
};
use self::output::copy_decoded_into;
use self::plan::{DecodeExecution, DecodeOptions, DecodeOutput, DecodePlan};

mod batch;
mod fallback;
mod native;
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

fn empty_ignorechars(ignorechars: Option<&Bound<'_, PyAny>>) -> bool {
    ignorechars.is_some_and(|value| {
        value
            .cast::<PyBytes>()
            .is_ok_and(|bytes| bytes.as_bytes().is_empty())
    })
}

fn decode_plan_allocating_inner<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
    options: DecodeOptions<'_, 'py>,
) -> PyResult<Bound<'py, PyBytes>> {
    let DecodeOptions {
        altchars,
        padded,
        ignorechars,
        canonical,
        ..
    } = options;
    let strict_mode = options.strict_mode();
    let empty_ignorechars = empty_ignorechars(ignorechars);
    if altchars.is_none()
        && padded
        && ignorechars.is_none_or(|_| empty_ignorechars)
        && (canonical || empty_ignorechars)
    {
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
    }

    if ignorechars.is_none() && !canonical && strict_mode {
        if !padded {
            return match decode_unpadded_with_altchars(py, input, altchars) {
                Ok(output) => Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
                Err(_) => decode_with_binascii(py, input, altchars, true, false, None, false),
            };
        }
        return match decode_strict_with_altchars(py, input, altchars) {
            Ok(output) => Ok(output),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => Err(error),
            Err(_) => decode_with_binascii(py, input, altchars, true, true, None, false),
        };
    }

    if ignorechars.is_none() && !canonical && !strict_mode {
        let direct = match altchars {
            None => Some(DecodeAlphabet::Standard),
            Some([b'-', b'_']) => Some(DecodeAlphabet::Mixed),
            Some(_) => None,
        };
        if let Some(alphabet) = direct
            && let Some(output) = try_decode_strict(py, input, alphabet)?
        {
            return Ok(output);
        }
        if padded
            && let Some(alphabet) = direct
            && let Some(normalized) = normalize_mime_whitespace(input)?
            && let Some(output) = try_decode_strict(py, &BytesLike::Owned(normalized), alphabet)?
        {
            return Ok(output);
        }
        if !padded {
            match decode_unpadded_with_altchars(py, input, altchars) {
                Ok(output) => return Ok(output),
                Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
                Err(_) => {}
            }
        }
        if let Some(output) = try_decode_lenient(py, input, altchars, padded)? {
            return Ok(output);
        }
    }
    if ignorechars.is_none()
        && canonical
        && !padded
        && unsafe { input.with_bytes(|input| canonical_unpadded_input(input, altchars)) }
    {
        match decode_unpadded_with_altchars(py, input, altchars) {
            Ok(output) => return Ok(output),
            Err(error) if error.is_instance_of::<PyMemoryError>(py) => return Err(error),
            Err(_) => {}
        }
    }
    if ignorechars.is_some() || canonical {
        return decode_advanced(py, input, options);
    }
    decode_with_binascii(
        py,
        input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )
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
    DecodePlan::new(&input, options)
        .execute(py, DecodeExecution::Allocate)
        .map(DecodeOutput::into_bytes)
}

/// Decode with the standard Base64 alphabet.
pub(super) fn standard_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::standard())
        .execute(py, DecodeExecution::Allocate)
        .map(DecodeOutput::into_bytes)
}

/// Decode standard Base64 into a reusable output.
pub(super) fn standard_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::standard())
        .execute(py, DecodeExecution::Into(output))
        .map(DecodeOutput::into_written)
}

pub(super) fn urlsafe_b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::urlsafe(padded))
        .execute(py, DecodeExecution::Allocate)
        .map(DecodeOutput::into_bytes)
}

pub(super) fn urlsafe_b64decode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = ascii_or_bytes(py, s, "s")?;
    DecodePlan::new(&input, DecodeOptions::urlsafe(padded))
        .execute(py, DecodeExecution::Into(output))
        .map(DecodeOutput::into_written)
}

fn decode_plan_into_inner<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, 'py>,
    output: &Bound<'py, PyByteArray>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<usize> {
    let DecodeOptions {
        altchars,
        padded,
        ignorechars,
        canonical,
        ..
    } = options;
    let strict_mode = options.strict_mode();
    let transactional_errors = !strict_mode;
    let alphabet = if altchars == Some(*b"-_") {
        DecodeAlphabet::Mixed
    } else {
        DecodeAlphabet::Standard
    };

    let empty_ignorechars = empty_ignorechars(ignorechars);
    if altchars.is_none()
        && padded
        && ignorechars.is_none_or(|_| empty_ignorechars)
        && (canonical || empty_ignorechars)
    {
        let canonical_input = !canonical
            || unsafe {
                input.with_bytes(|input| {
                    let padding =
                        usize::from(input.ends_with(b"=")) + usize::from(input.ends_with(b"=="));
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
    }

    let direct = if ignorechars.is_none() && !canonical && (padded || !strict_mode) {
        decode_strict_into_with_altchars(py, input, output, altchars, transactional_errors)?
    } else {
        Err(Base64Error::InvalidInput)
    };
    match direct {
        Ok(written) => return Ok(written),
        Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
            return Err(output_too_small(required, provided));
        }
        Err(Base64Error::OutputTooSmall { .. }) => {}
        Err(Base64Error::InvalidInput) => {}
    }

    if padded
        && !strict_mode
        && ignorechars.is_none()
        && !canonical
        && matches!(altchars, None | Some([b'-', b'_']))
        && let Some(normalized) = normalize_mime_whitespace(input)?
    {
        let normalized = BytesLike::Owned(normalized);
        match decode_strict_into(&normalized, output, alphabet, true)? {
            Ok(written) => return Ok(written),
            Err(Base64Error::OutputTooSmall { .. }) | Err(Base64Error::InvalidInput) => {}
        }
    }

    if !padded && ignorechars.is_none() && !canonical {
        match decode_unpadded_into_with_altchars(py, input, output, altchars, transactional_errors)?
        {
            Ok(written) => return Ok(written),
            Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                return Err(output_too_small(required, provided));
            }
            Err(Base64Error::OutputTooSmall { .. }) => {}
            Err(Base64Error::InvalidInput) => {}
        }
    }

    if !strict_mode
        && ignorechars.is_none()
        && !canonical
        && let Some(written) = try_decode_lenient_into(
            input,
            output,
            altchars,
            padded,
            lenient_continues_after_padding(py),
        )?
    {
        return Ok(written);
    }

    if ignorechars.is_none()
        && canonical
        && !padded
        && unsafe { input.with_bytes(|input| canonical_unpadded_input(input, altchars)) }
        && let Ok(written) = decode_unpadded_into_with_altchars(py, input, output, altchars, true)?
    {
        return Ok(written);
    }

    if ignorechars.is_some() || canonical {
        return decode_advanced_into(py, input, output, options);
    }

    let decoded = decode_with_binascii(
        py,
        input,
        altchars,
        strict_mode,
        padded,
        ignorechars,
        canonical,
    )?;
    copy_decoded_into(&decoded, output)
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
    DecodePlan::new(&input, options)
        .execute(py, DecodeExecution::Into(output))
        .map(DecodeOutput::into_written)
}
