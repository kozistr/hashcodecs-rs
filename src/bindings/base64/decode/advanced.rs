use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::lenient::symbols::{
    AlphanumericPrefix, TranslateBytes, select_alphanumeric_prefix, select_translate_bytes,
};
use crate::base64::{Base64Error, decode_layout, decode_unpadded_layout};
use crate::bindings::base64::STANDARD_ALPHABET;
use crate::bindings::base64::decode::fallback::decoding_error;
use crate::bindings::base64::decode::output::BytesWriter;
use crate::bindings::base64::decode::plan::DecodeOptions;
use crate::bindings::base64::output_too_small;
use crate::bindings::buffer::{BytesLike, contiguous_bytes_like, with_bytearray};
use crate::bindings::objects::{bytearray_data, bytearray_size};
use crate::bindings::runtime::BASE64_DETACH_THRESHOLD;

mod scanner;
mod staging;

#[cfg(test)]
use staging::{ADVANCED_STAGING_CAPACITY, StagingValidator, StagingWriter};

#[derive(Clone, Copy)]
pub(super) struct Translation {
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
    translate: TranslateBytes,
}

impl Translation {
    pub(super) fn new(table: &[u8; 256]) -> Option<Self> {
        let mut sources = [0_u8; 2];
        let mut targets = [0_u8; 2];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            if value < 64 && STANDARD_ALPHABET[usize::from(value)] != byte {
                assert!(count < 2, "a Base64 alphabet translates at most two bytes");
                sources[count] = byte;
                targets[count] = STANDARD_ALPHABET[usize::from(value)];
                count += 1;
            }
        }
        if count == 0 {
            return None;
        }
        if count == 1 {
            sources[1] = sources[0];
            targets[1] = targets[0];
        }
        Some(Self {
            source0: sources[0],
            target0: targets[0],
            source1: sources[1],
            target1: targets[1],
            translate: select_translate_bytes(),
        })
    }

    pub(super) fn apply(self, input: &mut [u8]) {
        unsafe {
            (self.translate)(
                input,
                self.source0,
                self.target0,
                self.source1,
                self.target1,
            )
        };
    }
}

pub(super) struct AdvancedDecoder {
    pub(super) table: [u8; 256],
    pub(super) ignored: [bool; 256],
    pub(super) strict_mode: bool,
    pub(super) padded: bool,
    pub(super) canonical: bool,
    pub(super) alphanumeric_prefix: AlphanumericPrefix,
    pub(super) strict_specials: StrictSpecials,
    pub(super) strict_forbidden: StrictSpecials,
    pub(super) translation: Option<Translation>,
}

impl AdvancedDecoder {
    pub(super) fn new(py: Python<'_>, options: DecodeOptions<'_, '_>) -> PyResult<Self> {
        let DecodeOptions {
            altchars,
            padded,
            ignorechars,
            canonical,
            ..
        } = options;
        let mut ignored = [false; 256];
        if let Some(ignorechars) = ignorechars {
            let ignorechars = contiguous_bytes_like(py, ignorechars, "ignorechars")?;
            unsafe {
                ignorechars.with_bytes(|bytes| {
                    for &byte in bytes {
                        ignored[usize::from(byte)] = true;
                    }
                })
            };
        }

        let mut table = [64; 256];
        for (value, &byte) in STANDARD_ALPHABET[..62].iter().enumerate() {
            table[usize::from(byte)] = value as u8;
        }
        let custom_alphabet = altchars.is_some() && ignorechars.is_some();
        if !custom_alphabet {
            table[usize::from(b'+')] = 62;
            table[usize::from(b'/')] = 63;
        }
        if let Some([plus, slash]) = altchars {
            if !custom_alphabet || plus != b'=' {
                table[usize::from(plus)] = 62;
            }
            if !custom_alphabet || slash != b'=' {
                table[usize::from(slash)] = 63;
            }
        }

        let strict_specials = StrictSpecials::new(&table, &ignored, padded);
        let strict_forbidden = StrictSpecials::forbidden(&table, &ignored);
        let translation = Translation::new(&table);
        Ok(Self {
            table,
            ignored,
            strict_mode: options.strict_mode(),
            padded,
            canonical,
            alphanumeric_prefix: select_alphanumeric_prefix(),
            strict_specials,
            strict_forbidden,
            translation,
        })
    }

    pub(super) fn preserves_alphanumeric(&self) -> bool {
        STANDARD_ALPHABET[..62]
            .iter()
            .enumerate()
            .all(|(value, &byte)| self.table[usize::from(byte)] == value as u8)
    }
}

#[derive(Clone, Copy)]
pub(super) enum StrictSpecials {
    None,
    One(u8),
    Two(u8, u8),
    Three(u8, u8, u8),
    Many,
}

impl StrictSpecials {
    pub(super) fn new(table: &[u8; 256], ignored: &[bool; 256], padded: bool) -> Self {
        let equals_is_padding = padded && table[usize::from(b'=')] >= 64;
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for byte in u8::MIN..=u8::MAX {
            let value = table[usize::from(byte)];
            let discarded =
                value >= 64 && ignored[usize::from(byte)] && !(equals_is_padding && byte == b'=');
            if discarded {
                if count == bytes.len() {
                    return Self::Many;
                }
                bytes[count] = byte;
                count += 1;
            }
        }
        match (count, bytes) {
            (0, _) => Self::None,
            (1, [first, ..]) => Self::One(first),
            (2, [first, second, _]) => Self::Two(first, second),
            (3, [first, second, third]) => Self::Three(first, second, third),
            _ => unreachable!("strict special-byte count is bounded"),
        }
    }

    pub(super) fn find(self, input: &[u8]) -> Option<usize> {
        match self {
            Self::None => None,
            Self::One(first) => memchr::memchr(first, input),
            Self::Two(first, second) => memchr::memchr2(first, second, input),
            Self::Three(first, second, third) => memchr::memchr3(first, second, third, input),
            Self::Many => unreachable!("many special bytes use the generic scanner"),
        }
    }

    pub(super) fn forbidden(table: &[u8; 256], ignored: &[bool; 256]) -> Self {
        let mut bytes = [0_u8; 3];
        let mut count = 0;
        for (value, &byte) in STANDARD_ALPHABET.iter().enumerate() {
            if table[usize::from(byte)] >= 64 && !ignored[usize::from(byte)] {
                if count == bytes.len() {
                    return Self::Many;
                }
                bytes[count] = byte;
                count += 1;
            } else if table[usize::from(byte)] < 64 {
                debug_assert!(
                    table[usize::from(byte)] == value as u8
                        || STANDARD_ALPHABET[usize::from(table[usize::from(byte)])] != byte
                );
            }
        }
        match (count, bytes) {
            (0, _) => Self::None,
            (1, [first, ..]) => Self::One(first),
            (2, [first, second, _]) => Self::Two(first, second),
            (3, [first, second, third]) => Self::Three(first, second, third),
            _ => unreachable!("strict forbidden-byte count is bounded"),
        }
    }
}

fn translated_strict_decoded_len(
    input: &[u8],
    altchars: [u8; 2],
    padded: bool,
) -> Result<usize, Base64Error> {
    if padded {
        if altchars.contains(&b'=') {
            return (input.len() & 3 == 0)
                .then(|| input.len() / 4 * 3)
                .ok_or(Base64Error::InvalidInput);
        }
        return decode_layout(input).map(|layout| layout.output_len());
    }
    if !altchars.contains(&b'=') && input.contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    decode_unpadded_layout(input).map(|layout| layout.output_len())
}

pub(super) fn decode_advanced_strict_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: [u8; 2],
    padded: bool,
    transactional_errors: bool,
) -> PyResult<Result<usize, Base64Error>> {
    if let Some(input) = input.snapshot_for_output(output)? {
        return decode_advanced_strict_into(
            py,
            &BytesLike::OwnedVec(input),
            output,
            altchars,
            padded,
            transactional_errors,
        );
    }
    let decoder = AdvancedDecoder::new(
        py,
        DecodeOptions::new(Some(altchars), Some(true), padded, None, false),
    )?;
    Ok(unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            if transactional_errors {
                let Some(required) = decoder.decoded_len(input, false) else {
                    return Err(Base64Error::InvalidInput);
                };
                if provided < required {
                    return Err(Base64Error::OutputTooSmall { required, provided });
                }
                let written = decoder.decode_validated_to_ptr(input, output, false);
                debug_assert_eq!(written, required);
                return Ok(written);
            }

            let required = translated_strict_decoded_len(input, altchars, padded)?;
            if provided < required {
                return Err(Base64Error::OutputTooSmall { required, provided });
            }
            decoder
                .decode_checked_to_ptr(input, output, false)
                .ok_or(Base64Error::InvalidInput)
        })
    })
}

pub(in crate::bindings::base64::decode) fn decode_advanced<'py>(
    py: Python<'py>,
    input: &BytesLike<'_, '_>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<Bound<'py, PyBytes>> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_advanced(py, &BytesLike::OwnedVec(input), options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = super::lenient::continues_after_padding(py);
    let writer = BytesWriter::new(py, input.len())?;
    let output_address = unsafe { writer.data() } as usize;
    let detach = input.detach_safe() && input.len() >= BASE64_DETACH_THRESHOLD;
    let result = unsafe {
        input.with_bytes(|input| {
            let decode = move || {
                decoder.decode_checked_to_ptr(
                    input,
                    output_address as *mut u8,
                    continue_after_padding,
                )
            };
            if detach { py.detach(decode) } else { decode() }
        })
    };
    let Some(written) = result else {
        return Err(decoding_error(py, "Incorrect padding"));
    };
    unsafe { writer.finish(py, written) }
}

unsafe fn decode_advanced_slice_into(
    py: Python<'_>,
    input: &[u8],
    output: *mut u8,
    provided: usize,
    decoder: &AdvancedDecoder,
    continue_after_padding: bool,
) -> PyResult<usize> {
    let Some(required) = decoder.decoded_len(input, continue_after_padding) else {
        return Err(decoding_error(py, "Incorrect padding"));
    };
    if provided < required {
        return Err(output_too_small(required, provided));
    }
    let written = unsafe { decoder.decode_validated_to_ptr(input, output, continue_after_padding) };
    debug_assert_eq!(written, required);
    Ok(written)
}

pub(in crate::bindings::base64::decode) fn decode_advanced_into(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    options: DecodeOptions<'_, '_>,
) -> PyResult<usize> {
    #[cfg(Py_GIL_DISABLED)]
    if let Some(input) = input.snapshot_mutable()? {
        return decode_advanced_into(py, &BytesLike::OwnedVec(input), output, options);
    }
    let decoder = AdvancedDecoder::new(py, options)?;
    let continue_after_padding = super::lenient::continues_after_padding(py);
    if let Some(input) = input.snapshot_for_output(output)? {
        return with_bytearray(output, || unsafe {
            decode_advanced_slice_into(
                py,
                &input,
                bytearray_data(output.as_ptr()),
                bytearray_size(output.as_ptr()),
                &decoder,
                continue_after_padding,
            )
        });
    }
    unsafe {
        input.with_bytes_and_output(output, |input, output, provided| {
            decode_advanced_slice_into(
                py,
                input,
                output,
                provided,
                &decoder,
                continue_after_padding,
            )
        })
    }
}

#[cfg(test)]
#[path = "advanced_tests.rs"]
mod tests;
