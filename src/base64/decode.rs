//! Base64 decoding API, layout validation, and scalar fallback.

#[cfg(target_arch = "aarch64")]
pub(super) mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx512;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod sse41;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod ssse3;
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
mod tables;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86_contracts;

use super::output_buffer::{allocate_uninitialized_output, assume_output_initialized};
use super::runtime_dispatch::{ErrorWritePolicy, decode_with_runtime_backend};
use super::{
    Base64Error, DECODE_STORE_PADDING, DecodeAlphabet, INVALID_VALUE, MIXED_DECODE,
    STANDARD_DECODE, URLSAFE_DECODE,
};

/// Decodes padded RFC 4648 Base64 input with the standard alphabet.
///
/// The input must contain complete four-byte groups and valid trailing padding.
/// This Rust API rejects whitespace and bytes outside the standard alphabet.
/// The function accepts nonzero unused bits in the final quantum.
///
/// # Arguments
///
/// * `input` - Contains the padded standard Base64 bytes to decode.
///
/// # Returns
///
/// The function returns a new vector that contains the decoded bytes.
///
/// # Errors
///
/// The function returns `Base64Error::InvalidInput` for invalid characters, padding, or group alignment.
///
/// # Examples
///
///     use hashcodecs::base64::b64decode;
///
///     assert_eq!(b64decode(b"aGVsbG8=").unwrap(), b"hello");
///     assert!(b64decode(b"aGVsbG8").is_err());
///
#[inline]
pub fn b64decode(input: &[u8]) -> Result<Vec<u8>, Base64Error> {
    b64decode_with_alphabet(input, false)
}

/// Decodes padded RFC 4648 Base64 input with the URL-safe alphabet.
///
/// The input must contain complete four-byte groups and valid trailing padding.
/// The input must use the URL-safe alphabet. The function accepts nonzero unused bits in the final quantum.
///
/// # Arguments
///
/// * `input` - Contains the padded URL-safe Base64 bytes to decode.
///
/// # Returns
///
/// The function returns a new vector that contains the decoded bytes.
///
/// # Errors
///
/// The function returns `Base64Error::InvalidInput` for invalid characters, padding, or group alignment.
///
/// # Examples
///
///     use hashcodecs::base64::b64decode_urlsafe;
///
///     assert_eq!(b64decode_urlsafe(b"-_8=").unwrap(), [0xfb, 0xff]);
///
#[inline]
pub fn b64decode_urlsafe(input: &[u8]) -> Result<Vec<u8>, Base64Error> {
    b64decode_with_alphabet(input, true)
}

/// Returns the decoded length for padded Base64 with a valid structure.
///
/// This function checks group alignment and the location and count of trailing `=` bytes.
/// It checks alphabet characters during decoding, not during this length calculation.
///
/// # Arguments
///
/// * `input` - Contains the padded Base64 input to measure.
///
/// # Returns
///
/// The function returns the exact decoded byte length.
///
/// # Errors
///
/// The function returns `Base64Error::InvalidInput` if the length or padding layout is invalid.
///
/// # Examples
///
///     use hashcodecs::base64::b64decoded_len;
///
///     assert_eq!(b64decoded_len(b"aGVsbG8=").unwrap(), 5);
///     assert!(b64decoded_len(b"abc").is_err());
///
#[inline]
pub fn b64decoded_len(input: &[u8]) -> Result<usize, Base64Error> {
    let layout = decode_layout(input)?;
    if input[..input.len() - layout.padding].contains(&b'=') {
        return Err(Base64Error::InvalidInput);
    }
    Ok(layout.output_len)
}

/// Decodes padded standard Base64 input into caller-provided storage.
///
/// The destination can contain more space than the result requires.
/// The function returns the number of bytes that it writes. It does not change bytes after this prefix.
/// Invalid input can change part of the destination prefix.
///
/// # Arguments
///
/// * `input` - Contains the padded standard Base64 bytes to decode.
/// * `output` - Provides storage for the complete decoded result.
///
/// # Returns
///
/// The function returns the number of decoded bytes that it writes to the start of `output`.
///
/// # Errors
///
/// The function returns `Base64Error::OutputTooSmall` before decoding if `output` is too short.
/// It returns `Base64Error::InvalidInput` for invalid Base64. This error can occur after the function changes a prefix.
///
/// # Examples
///
///     use hashcodecs::base64::b64decode_into;
///
///     let mut output = [b'.'; 8];
///     let written = b64decode_into(b"aGVsbG8=", &mut output).unwrap();
///     assert_eq!(&output[..written], b"hello");
///     assert_eq!(&output[written..], b"...");
///
#[inline]
pub fn b64decode_into(input: &[u8], output: &mut [u8]) -> Result<usize, Base64Error> {
    b64decode_into_with_alphabet(input, output, false)
}

/// Decodes padded URL-safe Base64 input into caller-provided storage.
///
/// The destination can contain more space than the result requires.
/// The function returns the number of bytes that it writes. It does not change bytes after this prefix.
/// Invalid input can change part of the destination prefix.
///
/// # Arguments
///
/// * `input` - Contains the padded URL-safe Base64 bytes to decode.
/// * `output` - Provides storage for the complete decoded result.
///
/// # Returns
///
/// The function returns the number of decoded bytes that it writes to the start of `output`.
///
/// # Errors
///
/// The function returns `Base64Error::OutputTooSmall` before decoding if `output` is too short.
/// It returns `Base64Error::InvalidInput` for invalid Base64. This error can occur after the function changes a prefix.
///
/// # Examples
///
///     use hashcodecs::base64::b64decode_urlsafe_into;
///
///     let mut output = [0; 2];
///     let written = b64decode_urlsafe_into(b"-_8=", &mut output).unwrap();
///     assert_eq!(written, 2);
///     assert_eq!(output, [0xfb, 0xff]);
///
#[inline]
pub fn b64decode_urlsafe_into(input: &[u8], output: &mut [u8]) -> Result<usize, Base64Error> {
    b64decode_into_with_alphabet(input, output, true)
}

#[inline]
fn b64decode_into_with_alphabet(
    input: &[u8],
    output: &mut [u8],
    urlsafe: bool,
) -> Result<usize, Base64Error> {
    let layout = decode_layout(input)?;
    if output.len() < layout.output_len {
        return Err(Base64Error::OutputTooSmall {
            required: layout.output_len,
            provided: output.len(),
        });
    }
    decode_to_slice_with_layout(input, &mut output[..layout.output_len], layout, urlsafe)?;
    Ok(layout.output_len)
}

#[inline]
fn b64decode_with_alphabet(input: &[u8], urlsafe: bool) -> Result<Vec<u8>, Base64Error> {
    let layout = decode_layout(input)?;
    if layout.output_len == 0 {
        return Ok(Vec::new());
    }
    let simd_len = input.len() - usize::from(layout.padding != 0) * 4;
    let output_has_store_slack = simd_len >= 16;
    let allocation_len = if output_has_store_slack {
        layout
            .output_len
            .checked_add(DECODE_STORE_PADDING)
            .expect("Base64 output is too large")
    } else {
        layout.output_len
    };
    let mut output = allocate_uninitialized_output(allocation_len);
    let alphabet = if urlsafe {
        DecodeAlphabet::UrlSafe
    } else {
        DecodeAlphabet::Standard
    };
    // The padded store mode may write at most `DECODE_STORE_PADDING` bytes past
    // the initialized result, all within this private allocation.
    unsafe {
        decode_to_ptr_with_layout(
            input,
            output.as_mut_ptr().cast(),
            layout,
            alphabet,
            output_has_store_slack,
        )?
    };
    // The decoder initializes the result prefix. The function discards the private padding.
    Ok(unsafe { assume_output_initialized(output, layout.output_len) })
}

/// Returns the layout for Base64 input without final padding.
///
/// The returned layout models the missing padding without allocating or
/// inspecting the input bytes. Alphabet validation remains part of decoding.
#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) fn decode_unpadded_layout(input: &[u8]) -> Result<DecodeLayout, Base64Error> {
    let complete_quartets = input.len() / 4;
    let tail = input.len() % 4;
    let tail_len = match tail {
        0 => 0,
        2 => 1,
        3 => 2,
        _ => return Err(Base64Error::InvalidInput),
    };
    let output_len = complete_quartets
        .checked_mul(3)
        .and_then(|length| length.checked_add(tail_len))
        .ok_or(Base64Error::InvalidInput)?;
    Ok(DecodeLayout {
        input_len: input.len(),
        padding: 0,
        output_len,
    })
}

#[inline]
pub(crate) fn decode_to_slice_with_layout(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    urlsafe: bool,
) -> Result<(), Base64Error> {
    let alphabet = if urlsafe {
        DecodeAlphabet::UrlSafe
    } else {
        DecodeAlphabet::Standard
    };
    decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet)
}

#[inline]
pub(crate) fn decode_to_slice_with_layout_and_alphabet(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    assert_eq!(input.len(), layout.input_len);
    assert_eq!(output.len(), layout.output_len);
    // The slice contains exactly the required initialized storage.
    // The decoder must keep all stores within this slice.
    unsafe { decode_to_ptr_with_layout(input, output.as_mut_ptr(), layout, alphabet, false) }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) fn decode_to_slice_with_unpadded_layout_and_alphabet(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    assert_eq!(input.len(), layout.input_len);
    assert_eq!(output.len(), layout.output_len);
    unsafe {
        decode_to_ptr_with_unpadded_layout_mode(
            input,
            output.as_mut_ptr(),
            layout,
            alphabet,
            ErrorWritePolicy::Partial,
        )
    }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) fn decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    assert_eq!(input.len(), layout.input_len);
    assert_eq!(output.len(), layout.output_len);
    unsafe {
        decode_to_ptr_with_unpadded_layout_mode(
            input,
            output.as_mut_ptr(),
            layout,
            alphabet,
            ErrorWritePolicy::ValidatedBlocksOnly,
        )
    }
}

#[inline]
pub(crate) unsafe fn decode_to_ptr_with_layout(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    output_has_store_slack: bool,
) -> Result<(), Base64Error> {
    unsafe {
        decode_to_ptr_with_layout_mode(
            input,
            output,
            layout,
            alphabet,
            output_has_store_slack,
            ErrorWritePolicy::Partial,
        )
    }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) unsafe fn decode_to_ptr_with_unpadded_layout(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    unsafe {
        decode_to_ptr_with_unpadded_layout_mode(
            input,
            output,
            layout,
            alphabet,
            ErrorWritePolicy::Partial,
        )
    }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) fn decode_to_slice_with_layout_and_alphabet_validated_blocks(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    assert_eq!(input.len(), layout.input_len);
    assert_eq!(output.len(), layout.output_len);
    // This error policy writes only complete, validated SIMD blocks.
    // A lenient caller can use its fallback without changing the suffix.
    unsafe {
        decode_to_ptr_with_layout_mode(
            input,
            output.as_mut_ptr(),
            layout,
            alphabet,
            false,
            ErrorWritePolicy::ValidatedBlocksOnly,
        )
    }
}

#[inline]
unsafe fn decode_to_ptr_with_layout_mode(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    output_has_store_slack: bool,
    error_write_policy: ErrorWritePolicy,
) -> Result<(), Base64Error> {
    let padding = layout.padding;
    let simd_len = if padding == 0 {
        input.len()
    } else {
        input.len() - 4
    };
    let (input_offset, output_offset) = if simd_len < 16 {
        (0, 0)
    } else {
        unsafe {
            decode_with_runtime_backend(
                &input[..simd_len],
                output,
                alphabet,
                output_has_store_slack,
                error_write_policy,
            )
        }?
    };

    let mut source = input_offset;
    let mut destination = output_offset;
    let table = match alphabet {
        DecodeAlphabet::Standard => &STANDARD_DECODE,
        DecodeAlphabet::UrlSafe => &URLSAFE_DECODE,
        DecodeAlphabet::Mixed => &MIXED_DECODE,
    };
    while source + 8 <= simd_len {
        unsafe { decode_eight_ptr(&input[source..source + 8], output.add(destination), table) }?;
        source += 8;
        destination += 6;
    }
    while source < simd_len {
        unsafe {
            decode_quad_ptr(
                &input[source..source + 4],
                output.add(destination),
                0,
                table,
            )
        }?;
        source += 4;
        destination += 3;
    }
    if padding != 0 {
        unsafe {
            decode_quad_ptr(
                &input[source..source + 4],
                output.add(destination),
                padding,
                table,
            )
        }?;
    }
    Ok(())
}

#[inline]
unsafe fn decode_to_ptr_with_unpadded_layout_mode(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    error_write_policy: ErrorWritePolicy,
) -> Result<(), Base64Error> {
    let prefix_len = input.len() / 4 * 4;
    let prefix_layout = DecodeLayout {
        input_len: prefix_len,
        padding: 0,
        output_len: prefix_len / 4 * 3,
    };
    unsafe {
        decode_to_ptr_with_layout_mode(
            &input[..prefix_len],
            output,
            prefix_layout,
            alphabet,
            false,
            error_write_policy,
        )?
    };

    let tail = &input[prefix_len..];
    if !tail.is_empty() {
        let table = match alphabet {
            DecodeAlphabet::Standard => &STANDARD_DECODE,
            DecodeAlphabet::UrlSafe => &URLSAFE_DECODE,
            DecodeAlphabet::Mixed => &MIXED_DECODE,
        };
        unsafe { decode_unpadded_tail_ptr(tail, output.add(prefix_layout.output_len), table) }?;
    }
    debug_assert_eq!(
        prefix_layout.output_len + tail.len() - tail.len() / 2,
        layout.output_len
    );
    Ok(())
}

#[inline]
pub(crate) fn decode_layout(input: &[u8]) -> Result<DecodeLayout, Base64Error> {
    if input.is_empty() {
        return Ok(DecodeLayout {
            input_len: 0,
            padding: 0,
            output_len: 0,
        });
    }
    if input.len() & 3 != 0 {
        return Err(Base64Error::InvalidInput);
    }

    let padding = match input {
        [.., b'=', b'='] => 2,
        [.., b'='] => 1,
        _ => 0,
    };
    Ok(DecodeLayout {
        input_len: input.len(),
        padding,
        output_len: input.len() / 4 * 3 - padding,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct DecodeLayout {
    input_len: usize,
    padding: usize,
    output_len: usize,
}

impl DecodeLayout {
    #[inline(always)]
    #[cfg(any(feature = "python", test))]
    pub(crate) fn output_len(self) -> usize {
        self.output_len
    }
}

pub(crate) unsafe fn decode_quad_ptr(
    input: &[u8],
    output: *mut u8,
    padding: usize,
    table: &[u8; 256],
) -> Result<(), Base64Error> {
    let first = decode_value(input[0], table).ok_or(Base64Error::InvalidInput)?;
    let second = decode_value(input[1], table).ok_or(Base64Error::InvalidInput)?;
    let third = if padding == 2 {
        0
    } else {
        decode_value(input[2], table).ok_or(Base64Error::InvalidInput)?
    };
    let fourth = if padding == 0 {
        decode_value(input[3], table).ok_or(Base64Error::InvalidInput)?
    } else {
        0
    };

    unsafe { output.write((first << 2) | (second >> 4)) };
    if padding < 2 {
        unsafe { output.add(1).write((second << 4) | (third >> 2)) };
    }
    if padding == 0 {
        unsafe { output.add(2).write((third << 6) | fourth) };
    }
    Ok(())
}

#[inline]
pub(crate) unsafe fn decode_unpadded_tail_ptr(
    input: &[u8],
    output: *mut u8,
    table: &[u8; 256],
) -> Result<(), Base64Error> {
    debug_assert!(matches!(input.len(), 2 | 3));
    let first = decode_value(input[0], table).ok_or(Base64Error::InvalidInput)?;
    let second = decode_value(input[1], table).ok_or(Base64Error::InvalidInput)?;
    let third = if input.len() == 3 {
        decode_value(input[2], table).ok_or(Base64Error::InvalidInput)?
    } else {
        0
    };

    unsafe { output.write((first << 2) | (second >> 4)) };
    if input.len() == 3 {
        unsafe { output.add(1).write((second << 4) | (third >> 2)) };
    }
    Ok(())
}

#[inline]
pub(crate) unsafe fn decode_eight_ptr(
    input: &[u8],
    output: *mut u8,
    table: &[u8; 256],
) -> Result<(), Base64Error> {
    let first = table[input[0] as usize];
    let second = table[input[1] as usize];
    let third = table[input[2] as usize];
    let fourth = table[input[3] as usize];
    let fifth = table[input[4] as usize];
    let sixth = table[input[5] as usize];
    let seventh = table[input[6] as usize];
    let eighth = table[input[7] as usize];
    if first | second | third | fourth | fifth | sixth | seventh | eighth == INVALID_VALUE {
        return Err(Base64Error::InvalidInput);
    }
    let decoded = [
        (first << 2) | (second >> 4),
        (second << 4) | (third >> 2),
        (third << 6) | fourth,
        (fifth << 2) | (sixth >> 4),
        (sixth << 4) | (seventh >> 2),
        (seventh << 6) | eighth,
    ];
    unsafe { output.copy_from_nonoverlapping(decoded.as_ptr(), decoded.len()) };
    Ok(())
}

#[inline(always)]
fn decode_value(byte: u8, table: &[u8; 256]) -> Option<u8> {
    let value = table[byte as usize];
    (value != INVALID_VALUE).then_some(value)
}
