use core::{fmt, mem::ManuallyDrop, mem::MaybeUninit};

#[cfg(test)]
use dispatch::{
    Backend, backend_supported, decode_with_backend, decode_with_backend_ptr, encode_with_backend,
    select_aarch64_backend, select_x86_backend,
};
use dispatch::{decode_simd_ptr, encode_simd_ptr};

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URLSAFE_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const INVALID_VALUE: u8 = u8::MAX;
const STANDARD_DECODE: [u8; 256] = decode_table(false, false);
const URLSAFE_DECODE: [u8; 256] = decode_table(true, false);
const MIXED_DECODE: [u8; 256] = decode_table(true, true);
const DECODE_STORE_PADDING: usize = 4;

#[inline]
fn uninitialized_output(length: usize) -> Vec<MaybeUninit<u8>> {
    let mut output = Vec::with_capacity(length);
    // `MaybeUninit<u8>` permits every bit pattern, including uninitialized memory.
    unsafe { output.set_len(length) };
    output
}

#[inline]
unsafe fn initialized_output(output: Vec<MaybeUninit<u8>>, length: usize) -> Vec<u8> {
    debug_assert!(length <= output.len());
    let mut output = ManuallyDrop::new(output);
    // The caller guarantees that every byte in the returned prefix was written.
    unsafe { Vec::from_raw_parts(output.as_mut_ptr().cast(), length, output.capacity()) }
}

const fn decode_table(urlsafe: bool, mixed: bool) -> [u8; 256] {
    let mut table = [INVALID_VALUE; 256];
    let mut index = 0;
    while index < 26 {
        table[b'A' as usize + index] = index as u8;
        table[b'a' as usize + index] = index as u8 + 26;
        index += 1;
    }
    index = 0;
    while index < 10 {
        table[b'0' as usize + index] = index as u8 + 52;
        index += 1;
    }
    if urlsafe || mixed {
        table[b'-' as usize] = 62;
        table[b'_' as usize] = 63;
    }
    if !urlsafe || mixed {
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
    }
    table
}

/// An error returned by a Base64 operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Base64Error {
    /// The input is not valid padded Base64 for the selected alphabet.
    InvalidInput,
    /// The destination cannot hold the complete result.
    OutputTooSmall {
        /// The minimum destination length.
        required: usize,
        /// The supplied destination length.
        provided: usize,
    },
}

impl fmt::Display for Base64Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => formatter.write_str("invalid Base64 input"),
            Self::OutputTooSmall { required, provided } => write!(
                formatter,
                "Base64 output requires {required} bytes but the destination has {provided}"
            ),
        }
    }
}

impl std::error::Error for Base64Error {}

#[derive(Clone, Copy, Debug)]
pub(crate) enum DecodeAlphabet {
    Standard,
    UrlSafe,
    #[cfg_attr(not(feature = "python"), allow(dead_code))]
    Mixed,
}

/// Encodes bytes using the RFC 4648 standard alphabet and padding.
#[inline]
pub fn b64encode(input: &[u8]) -> String {
    b64encode_with_alphabet(input, false)
}

/// Encodes bytes using the RFC 4648 URL-safe alphabet and padding.
#[inline]
pub fn b64encode_urlsafe(input: &[u8]) -> String {
    b64encode_with_alphabet(input, true)
}

/// Returns the padded Base64 length, or `None` if the arithmetic overflows.
#[inline]
pub const fn b64encoded_len(input_len: usize) -> Option<usize> {
    let groups = input_len / 3 + if input_len.is_multiple_of(3) { 0 } else { 1 };
    groups.checked_mul(4)
}

/// Encodes into a caller-provided destination using the standard alphabet.
///
/// The destination may be larger than necessary. On success, the returned value
/// is the initialized prefix length; bytes after that prefix are left unchanged.
#[inline]
pub fn b64encode_into(input: &[u8], output: &mut [u8]) -> Result<usize, Base64Error> {
    b64encode_into_with_alphabet(input, output, false)
}

/// Encodes into a caller-provided destination using the URL-safe alphabet.
///
/// The destination may be larger than necessary. On success, the returned value
/// is the initialized prefix length; bytes after that prefix are left unchanged.
#[inline]
pub fn b64encode_urlsafe_into(input: &[u8], output: &mut [u8]) -> Result<usize, Base64Error> {
    b64encode_into_with_alphabet(input, output, true)
}

#[inline]
fn b64encode_into_with_alphabet(
    input: &[u8],
    output: &mut [u8],
    urlsafe: bool,
) -> Result<usize, Base64Error> {
    let required = encoded_len(input.len());
    if output.len() < required {
        return Err(Base64Error::OutputTooSmall {
            required,
            provided: output.len(),
        });
    }
    encode_to_slice(input, &mut output[..required], urlsafe);
    Ok(required)
}

#[inline]
fn b64encode_with_alphabet(input: &[u8], urlsafe: bool) -> String {
    let output_len = encoded_len(input.len());
    let mut output = uninitialized_output(output_len);
    // The output allocation contains exactly `output_len` writable bytes.
    unsafe { encode_to_ptr(input, output.as_mut_ptr().cast(), urlsafe) };
    // Every output byte is initialized by `encode_to_ptr`.
    let output = unsafe { initialized_output(output, output_len) };

    // The encoder writes only ASCII Base64 characters.
    unsafe { String::from_utf8_unchecked(output) }
}

#[inline]
pub(crate) fn encoded_len(input_len: usize) -> usize {
    b64encoded_len(input_len).expect("Base64 input is too large")
}

#[inline]
pub(crate) fn encode_to_slice(input: &[u8], output: &mut [u8], urlsafe: bool) {
    debug_assert_eq!(output.len(), encoded_len(input.len()));
    // The exact output length was checked above.
    unsafe { encode_to_ptr(input, output.as_mut_ptr(), urlsafe) };
}

#[inline]
pub(crate) unsafe fn encode_to_ptr(input: &[u8], output: *mut u8, urlsafe: bool) {
    if input.len() < 16 {
        unsafe { encode_scalar_ptr(input, output, urlsafe) };
        return;
    }

    let input_offset = unsafe { encode_simd_ptr(input, output, urlsafe) };
    unsafe {
        encode_scalar_ptr(
            &input[input_offset..],
            output.add(input_offset / 3 * 4),
            urlsafe,
        )
    };
}

/// Decodes RFC 4648 standard Base64 with required quartet alignment.
#[inline]
pub fn b64decode(input: &[u8]) -> Result<Vec<u8>, Base64Error> {
    b64decode_with_alphabet(input, false)
}

/// Decodes RFC 4648 URL-safe Base64 with required quartet alignment.
#[inline]
pub fn b64decode_urlsafe(input: &[u8]) -> Result<Vec<u8>, Base64Error> {
    b64decode_with_alphabet(input, true)
}

/// Returns the decoded length for structurally valid padded Base64.
///
/// Alphabet validation happens while decoding.
#[inline]
pub fn b64decoded_len(input: &[u8]) -> Result<usize, Base64Error> {
    decoded_len(input)
}

/// Decodes standard padded Base64 into a caller-provided destination.
///
/// The destination may be larger than necessary. On success, the returned value
/// is the initialized prefix length; bytes after that prefix are left unchanged.
/// If the input is invalid, the destination prefix may have been modified.
#[inline]
pub fn b64decode_into(input: &[u8], output: &mut [u8]) -> Result<usize, Base64Error> {
    b64decode_into_with_alphabet(input, output, false)
}

/// Decodes URL-safe padded Base64 into a caller-provided destination.
///
/// The destination may be larger than necessary. On success, the returned value
/// is the initialized prefix length; bytes after that prefix are left unchanged.
/// If the input is invalid, the destination prefix may have been modified.
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

#[inline]
pub(crate) fn decoded_len(input: &[u8]) -> Result<usize, Base64Error> {
    Ok(decode_layout(input)?.output_len)
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
    padding: usize,
    pub(crate) output_len: usize,
}

#[inline]
#[cfg(test)]
fn encode_scalar(input: &[u8], output: &mut [u8], urlsafe: bool) {
    // The caller provides the exact scalar result length.
    unsafe { encode_scalar_ptr(input, output.as_mut_ptr(), urlsafe) };
}

#[inline]
unsafe fn encode_scalar_ptr(input: &[u8], output: *mut u8, urlsafe: bool) {
    let alphabet = if urlsafe {
        URLSAFE_ALPHABET
    } else {
        STANDARD_ALPHABET
    };
    let input_len = input.len();
    let input_ptr = input.as_ptr();
    let mut source = 0;
    let mut destination = 0;
    // `input_ptr` comes from the live slice above. Each raw read is guarded by the
    // loop or tail length check, avoiding a separate slice bounds check per byte.
    while source + 6 <= input_len {
        let (first, second) = unsafe {
            let block = input_ptr.add(source);
            (
                ((block.read() as u32) << 16)
                    | ((block.add(1).read() as u32) << 8)
                    | block.add(2).read() as u32,
                ((block.add(3).read() as u32) << 16)
                    | ((block.add(4).read() as u32) << 8)
                    | block.add(5).read() as u32,
            )
        };
        let encoded = [
            alphabet[((first >> 18) & 0x3f) as usize],
            alphabet[((first >> 12) & 0x3f) as usize],
            alphabet[((first >> 6) & 0x3f) as usize],
            alphabet[(first & 0x3f) as usize],
            alphabet[((second >> 18) & 0x3f) as usize],
            alphabet[((second >> 12) & 0x3f) as usize],
            alphabet[((second >> 6) & 0x3f) as usize],
            alphabet[(second & 0x3f) as usize],
        ];
        unsafe {
            output
                .add(destination)
                .copy_from_nonoverlapping(encoded.as_ptr(), 8)
        };
        source += 6;
        destination += 8;
    }
    while source + 3 <= input_len {
        let block = unsafe {
            let input = input_ptr.add(source);
            ((input.read() as u32) << 16)
                | ((input.add(1).read() as u32) << 8)
                | input.add(2).read() as u32
        };
        unsafe {
            output
                .add(destination)
                .write(alphabet[((block >> 18) & 0x3f) as usize]);
            output
                .add(destination + 1)
                .write(alphabet[((block >> 12) & 0x3f) as usize]);
            output
                .add(destination + 2)
                .write(alphabet[((block >> 6) & 0x3f) as usize]);
            output
                .add(destination + 3)
                .write(alphabet[(block & 0x3f) as usize]);
        }
        source += 3;
        destination += 4;
    }

    let remaining = input_len - source;
    if remaining == 1 {
        let block = (unsafe { input_ptr.add(source).read() } as u32) << 16;
        unsafe {
            output
                .add(destination)
                .write(alphabet[((block >> 18) & 0x3f) as usize]);
            output
                .add(destination + 1)
                .write(alphabet[((block >> 12) & 0x3f) as usize]);
            output.add(destination + 2).write(b'=');
            output.add(destination + 3).write(b'=');
        }
    } else if remaining == 2 {
        let block = unsafe {
            ((input_ptr.add(source).read() as u32) << 16)
                | ((input_ptr.add(source + 1).read() as u32) << 8)
        };
        unsafe {
            output
                .add(destination)
                .write(alphabet[((block >> 18) & 0x3f) as usize]);
            output
                .add(destination + 1)
                .write(alphabet[((block >> 12) & 0x3f) as usize]);
            output
                .add(destination + 2)
                .write(alphabet[((block >> 6) & 0x3f) as usize]);
            output.add(destination + 3).write(b'=');
        }
    }
}

#[inline]
unsafe fn decode_quad_ptr(
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
unsafe fn decode_unpadded_tail_ptr(
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
unsafe fn decode_eight_ptr(
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

#[cfg(target_arch = "aarch64")]
mod aarch64;

mod dispatch;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

#[cfg(all(not(coverage), any(target_arch = "x86", target_arch = "x86_64")))]
mod x86_avx512;

#[cfg(test)]
mod tests;
