use core::{ptr, slice};

use pyo3::exceptions::PyMemoryError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::super::{STANDARD_ALPHABET, output_too_small, with_output_ptr};
use super::fallback::{canonical_padding, decode_with_binascii, decoding_error};
use super::plan::{DecodeAttempt, DecodeOptions};
use super::{
    decode_advanced, decode_advanced_into, decode_strict, decode_strict_into,
    decode_strict_into_with_altchars, decode_strict_with_altchars,
    decode_unpadded_into_with_altchars, decode_unpadded_with_altchars, try_decode_lenient,
    try_decode_lenient_into, try_decode_strict, try_decode_urlsafe_315,
    try_decode_urlsafe_315_into,
};
use crate::base64::{Base64Error, DecodeAlphabet};
use crate::bindings::buffer::BytesLike;

pub(super) struct Execution<T> {
    pub(super) value: T,
    pub(super) skips_legacy_warning: bool,
}

pub(super) struct AllocatingExecutor;

impl AllocatingExecutor {
    pub(super) fn execute<'py>(
        py: Python<'py>,
        input: &BytesLike<'_, 'py>,
        options: DecodeOptions<'_, '_>,
        attempts: &[DecodeAttempt],
    ) -> PyResult<Execution<Bound<'py, PyBytes>>> {
        let DecodeOptions {
            altchars,
            padded,
            canonical,
            ..
        } = options;
        let strict_mode = options.strict_mode();

        for &attempt in attempts {
            match attempt {
                DecodeAttempt::Urlsafe315 => {
                    if let Some(value) = try_decode_urlsafe_315(py, input, strict_mode, padded)? {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: true,
                        });
                    }
                }
                DecodeAttempt::StandardStrict => {
                    match decode_strict(py, input, DecodeAlphabet::Standard) {
                        Ok(value) => {
                            let canonical_input = !canonical
                                || unsafe {
                                    input.with_bytes(|input| {
                                        // A successful strict decode guarantees that padding is
                                        // confined to the final two bytes.
                                        let padding = usize::from(input.ends_with(b"="))
                                            + usize::from(input.ends_with(b"=="));
                                        canonical_padding(&input[..input.len() - padding])
                                    })
                                };
                            if canonical_input {
                                return Ok(Execution {
                                    value,
                                    skips_legacy_warning: false,
                                });
                            }
                            return Err(decoding_error(py, "Non-zero padding bits"));
                        }
                        Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                            return Err(error);
                        }
                        Err(_) => {}
                    }
                }
                DecodeAttempt::Strict => match decode_strict_with_altchars(py, input, altchars) {
                    Ok(value) => {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                    Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                        return Err(error);
                    }
                    Err(_) => {}
                },
                DecodeAttempt::StrictProbe => {
                    let alphabet = match altchars {
                        None => DecodeAlphabet::Standard,
                        Some([b'-', b'_']) => DecodeAlphabet::Mixed,
                        Some(_) => unreachable!("the planner limits direct lenient decoding"),
                    };
                    if let Some(value) = try_decode_strict(py, input, alphabet)? {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::MimeWhitespace => {
                    if unsafe {
                        input.with_bytes(|input| {
                            is_mime_whitespace_input(input, altchars == Some(*b"-_"))
                        })
                    } {
                        let value = decode_advanced(py, input, options)?;
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::Unpadded => {
                    match decode_unpadded_with_altchars(py, input, altchars) {
                        Ok(value) => {
                            return Ok(Execution {
                                value,
                                skips_legacy_warning: false,
                            });
                        }
                        Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                            return Err(error);
                        }
                        Err(_) => {}
                    }
                }
                DecodeAttempt::Lenient => {
                    if let Some(value) = try_decode_lenient(py, input, altchars, padded)? {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::CanonicalUnpadded => {
                    if unsafe {
                        input.with_bytes(|input| canonical_unpadded_input(input, altchars))
                    } {
                        match decode_unpadded_with_altchars(py, input, altchars) {
                            Ok(value) => {
                                return Ok(Execution {
                                    value,
                                    skips_legacy_warning: false,
                                });
                            }
                            Err(error) if error.is_instance_of::<PyMemoryError>(py) => {
                                return Err(error);
                            }
                            Err(_) => {}
                        }
                    }
                }
                DecodeAttempt::Advanced => {
                    let value = decode_advanced(py, input, options)?;
                    return Ok(Execution {
                        value,
                        skips_legacy_warning: false,
                    });
                }
                DecodeAttempt::Binascii => {
                    let value = decode_with_binascii(py, input, altchars, strict_mode, padded)?;
                    return Ok(Execution {
                        value,
                        skips_legacy_warning: false,
                    });
                }
            }
        }
        unreachable!("every decode plan ends in an infallible routing attempt")
    }
}

pub(super) struct IntoExecutor;

impl IntoExecutor {
    pub(super) fn execute(
        py: Python<'_>,
        input: &BytesLike<'_, '_>,
        output: &Bound<'_, PyByteArray>,
        options: DecodeOptions<'_, '_>,
        attempts: &[DecodeAttempt],
    ) -> PyResult<Execution<usize>> {
        let DecodeOptions {
            altchars,
            padded,
            canonical,
            ..
        } = options;
        let strict_mode = options.strict_mode();
        let transactional_errors = !strict_mode;

        for &attempt in attempts {
            match attempt {
                DecodeAttempt::Urlsafe315 => {
                    if let Some(value) =
                        try_decode_urlsafe_315_into(input, output, strict_mode, padded)?
                    {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: true,
                        });
                    }
                }
                DecodeAttempt::StandardStrict => {
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
                        && let Ok(value) = decode_strict_into(
                            input,
                            output,
                            DecodeAlphabet::Standard,
                            // A validated prefix is safe for the advanced fallback to overwrite.
                            true,
                        )?
                    {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::Strict => {
                    match decode_strict_into_with_altchars(py, input, output, altchars, false)? {
                        Ok(value) => {
                            return Ok(Execution {
                                value,
                                skips_legacy_warning: false,
                            });
                        }
                        Err(Base64Error::OutputTooSmall { required, provided }) => {
                            return Err(output_too_small(required, provided));
                        }
                        Err(Base64Error::InvalidInput) => {}
                    }
                }
                DecodeAttempt::StrictProbe => {
                    match decode_strict_into_with_altchars(py, input, output, altchars, true)? {
                        Ok(value) => {
                            return Ok(Execution {
                                value,
                                skips_legacy_warning: false,
                            });
                        }
                        Err(Base64Error::OutputTooSmall { .. } | Base64Error::InvalidInput) => {}
                    }
                }
                DecodeAttempt::MimeWhitespace => {
                    if unsafe {
                        input.with_bytes(|input| {
                            is_mime_whitespace_input(input, altchars == Some(*b"-_"))
                        })
                    } {
                        let value = decode_advanced_into(py, input, output, options)?;
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::Unpadded => {
                    match decode_unpadded_into_with_altchars(
                        py,
                        input,
                        output,
                        altchars,
                        transactional_errors,
                    )? {
                        Ok(value) => {
                            return Ok(Execution {
                                value,
                                skips_legacy_warning: false,
                            });
                        }
                        Err(Base64Error::OutputTooSmall { required, provided }) if strict_mode => {
                            return Err(output_too_small(required, provided));
                        }
                        Err(Base64Error::OutputTooSmall { .. } | Base64Error::InvalidInput) => {}
                    }
                }
                DecodeAttempt::Lenient => {
                    if let Some(value) =
                        try_decode_lenient_into(py, input, output, altchars, padded)?
                    {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::CanonicalUnpadded => {
                    if unsafe {
                        input.with_bytes(|input| canonical_unpadded_input(input, altchars))
                    } && let Ok(value) =
                        decode_unpadded_into_with_altchars(py, input, output, altchars, true)?
                    {
                        return Ok(Execution {
                            value,
                            skips_legacy_warning: false,
                        });
                    }
                }
                DecodeAttempt::Advanced => {
                    let value = decode_advanced_into(py, input, output, options)?;
                    return Ok(Execution {
                        value,
                        skips_legacy_warning: false,
                    });
                }
                DecodeAttempt::Binascii => {
                    let decoded = decode_with_binascii(py, input, altchars, strict_mode, padded)?;
                    return Ok(Execution {
                        value: copy_decoded_into(&decoded, output)?,
                        skips_legacy_warning: false,
                    });
                }
            }
        }
        unreachable!("every decode plan ends in an infallible routing attempt")
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
