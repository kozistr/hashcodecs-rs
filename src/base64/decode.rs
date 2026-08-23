//! Base64 decoding API, layout validation, and scalar fallback.

use super::dispatch::decode_simd_ptr;
use super::output::{initialized_output, uninitialized_output};
use super::{
    Base64Error, DECODE_STORE_PADDING, DecodeAlphabet, INVALID_VALUE, MIXED_DECODE,
    STANDARD_DECODE, URLSAFE_DECODE,
};

/// Decodes padded RFC 4648 Base64 with the standard alphabet.
///
/// The input must be quartet-aligned, use only the standard alphabet, and have
/// valid trailing padding. Unlike Python's lenient decoder, this Rust API does
/// not ignore whitespace or other non-alphabet bytes.
///
/// # Arguments
///
/// * input - The padded standard Base64 bytes to decode.
///
/// # Returns
///
/// A newly allocated vector containing the decoded bytes.
///
/// # Errors
///
/// Returns Base64Error::InvalidInput for invalid characters, padding, or
/// quartet alignment.
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

/// Decodes padded RFC 4648 Base64 with the URL-safe alphabet.
///
/// The input must be quartet-aligned, use only the URL-safe alphabet, and have
/// valid trailing padding.
///
/// # Arguments
///
/// * input - The padded URL-safe Base64 bytes to decode.
///
/// # Returns
///
/// A newly allocated vector containing the decoded bytes.
///
/// # Errors
///
/// Returns Base64Error::InvalidInput for invalid characters, padding, or
/// quartet alignment.
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

/// Calculates the decoded length of structurally valid padded Base64.
///
/// This function validates quartet alignment and the placement and count of
/// trailing = bytes. It does not validate alphabet characters; that happens
/// while decoding.
///
/// # Arguments
///
/// * input - Padded Base64 whose output length is required.
///
/// # Returns
///
/// The exact decoded byte length.
///
/// # Errors
///
/// Returns Base64Error::InvalidInput when the length or padding layout is
/// invalid.
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
    Ok(decode_layout(input)?.output_len)
}

/// Decodes standard padded Base64 into a caller-provided destination.
///
/// The destination may be larger than necessary. On success, the returned value
/// is the initialized prefix length; bytes after that prefix are left unchanged.
/// If the input is invalid, the destination prefix may have been modified.
///
/// # Arguments
///
/// * input - The padded standard Base64 bytes to decode.
/// * output - Storage for the complete decoded result.
///
/// # Returns
///
/// The number of decoded bytes written to the start of output.
///
/// # Errors
///
/// Returns Base64Error::OutputTooSmall before decoding if output is too short.
/// Returns Base64Error::InvalidInput for invalid Base64; in that case a prefix
/// of output may already have changed.
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

/// Decodes URL-safe padded Base64 into a caller-provided destination.
///
/// The destination may be larger than necessary. On success, the returned value
/// is the initialized prefix length; bytes after that prefix are left unchanged.
/// If the input is invalid, the destination prefix may have been modified.
///
/// # Arguments
///
/// * input - The padded URL-safe Base64 bytes to decode.
/// * output - Storage for the complete decoded result.
///
/// # Returns
///
/// The number of decoded bytes written to the start of output.
///
/// # Errors
///
/// Returns Base64Error::OutputTooSmall before decoding if output is too short.
/// Returns Base64Error::InvalidInput for invalid Base64; in that case a prefix
/// of output may already have changed.
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
    let padded_stores = simd_len >= 16;
    let allocation_len = if padded_stores {
        layout
            .output_len
            .checked_add(DECODE_STORE_PADDING)
            .expect("Base64 output is too large")
    } else {
        layout.output_len
    };
    let mut output = uninitialized_output(allocation_len);
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
            padded_stores,
        )?
    };
    // The result prefix is fully initialized; the private padding is discarded.
    Ok(unsafe { initialized_output(output, layout.output_len) })
}

/// Returns the layout for Base64 input whose final padding is omitted.
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
    debug_assert_eq!(output.len(), layout.output_len);
    // The slice has exactly the required initialized storage, so only bounded
    // stores are permitted.
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
    debug_assert_eq!(output.len(), layout.output_len);
    unsafe {
        decode_to_ptr_with_unpadded_layout_mode(input, output.as_mut_ptr(), layout, alphabet, false)
    }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) fn decode_to_slice_with_unpadded_layout_and_alphabet_transactional(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    debug_assert_eq!(output.len(), layout.output_len);
    unsafe {
        decode_to_ptr_with_unpadded_layout_mode(input, output.as_mut_ptr(), layout, alphabet, true)
    }
}

#[inline]
pub(crate) unsafe fn decode_to_ptr_with_layout(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
) -> Result<(), Base64Error> {
    unsafe { decode_to_ptr_with_layout_mode(input, output, layout, alphabet, padded_stores, false) }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) unsafe fn decode_to_ptr_with_unpadded_layout(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    unsafe { decode_to_ptr_with_unpadded_layout_mode(input, output, layout, alphabet, false) }
}

#[inline]
#[cfg_attr(not(feature = "python"), allow(dead_code))]
pub(crate) fn decode_to_slice_with_layout_and_alphabet_transactional(
    input: &[u8],
    output: &mut [u8],
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
) -> Result<(), Base64Error> {
    debug_assert_eq!(output.len(), layout.output_len);
    // Transactional SIMD error handling writes only complete, validated blocks,
    // so a lenient caller can safely fall back without modifying the suffix.
    unsafe {
        decode_to_ptr_with_layout_mode(input, output.as_mut_ptr(), layout, alphabet, false, true)
    }
}

#[inline]
unsafe fn decode_to_ptr_with_layout_mode(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
    transactional_errors: bool,
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
            decode_simd_ptr(
                &input[..simd_len],
                output,
                alphabet,
                padded_stores,
                transactional_errors,
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
    transactional_errors: bool,
) -> Result<(), Base64Error> {
    let prefix_len = input.len() / 4 * 4;
    let prefix_layout = DecodeLayout {
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
            transactional_errors,
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
        padding,
        output_len: input.len() / 4 * 3 - padding,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct DecodeLayout {
    pub(crate) padding: usize,
    pub(crate) output_len: usize,
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
