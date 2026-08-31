//! Base64 encoding API and scalar fallback.

#[cfg(target_arch = "aarch64")]
pub(super) mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx512;
#[cfg(all(target_arch = "x86_64", not(any(kani, miri))))]
pub(super) mod cache;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod ssse3;

use super::output_buffer::{allocate_uninitialized_output, assume_output_initialized};
use super::runtime_dispatch::encode_with_runtime_backend;
use super::{Base64Error, STANDARD_ALPHABET, URLSAFE_ALPHABET};

/// Encodes input with the padded RFC 4648 standard Base64 alphabet.
///
/// Runtime dispatch selects the highest-priority supported backend. The function returns ASCII text.
/// The function adds padding when the final input group is incomplete.
///
/// # Arguments
///
/// * `input` - Contains the bytes to encode.
///
/// # Returns
///
/// The function returns a new Base64 string. The string uses `+` and `/` for values 62 and 63.
///
/// # Examples
///
///     use hashcodecs::base64::b64encode;
///
///     assert_eq!(b64encode(b"hello"), "aGVsbG8=");
///
#[inline]
pub fn b64encode(input: &[u8]) -> String {
    b64encode_with_alphabet(input, false)
}

/// Encodes input with the padded RFC 4648 URL-safe Base64 alphabet.
///
/// The URL-safe alphabet uses `-` and `_` instead of `+` and `/`.
/// Runtime dispatch selects the highest-priority supported backend.
///
/// # Arguments
///
/// * `input` - Contains the bytes to encode.
///
/// # Returns
///
/// The function returns a new padded URL-safe Base64 string.
///
/// # Examples
///
///     use hashcodecs::base64::b64encode_urlsafe;
///
///     assert_eq!(b64encode_urlsafe(&[0xfb, 0xff]), "-_8=");
///
#[inline]
pub fn b64encode_urlsafe(input: &[u8]) -> String {
    b64encode_with_alphabet(input, true)
}

/// Returns the encoded length of padded Base64 without encoding the input.
///
/// # Arguments
///
/// * `input_len` - Specifies the number of input bytes.
///
/// # Returns
///
/// The function returns `Some(length)` for the required output size.
/// It returns `None` if four-character group rounding overflows `usize`.
///
/// # Examples
///
///     use hashcodecs::base64::b64encoded_len;
///
///     assert_eq!(b64encoded_len(0), Some(0));
///     assert_eq!(b64encoded_len(5), Some(8));
///
#[inline]
pub const fn b64encoded_len(input_len: usize) -> Option<usize> {
    let groups = input_len / 3 + if input_len.is_multiple_of(3) { 0 } else { 1 };
    groups.checked_mul(4)
}

/// Encodes input into caller-provided storage with the standard alphabet.
///
/// The destination can contain more space than the result requires.
/// The function returns the number of bytes that it writes. It does not change bytes after this prefix.
///
/// # Arguments
///
/// * `input` - Contains the bytes to encode.
/// * `output` - Provides storage for the complete padded Base64 result.
///
/// # Returns
///
/// The function returns the number of bytes that it writes to the start of `output`.
///
/// # Errors
///
/// The function returns `Base64Error::OutputTooSmall` before it writes if `output` is too short.
/// Use `b64encoded_len` to get the required length.
///
/// # Examples
///
///     use hashcodecs::base64::b64encode_into;
///
///     let mut output = [b'.'; 12];
///     let written = b64encode_into(b"hello", &mut output).unwrap();
///     assert_eq!(&output[..written], b"aGVsbG8=");
///     assert_eq!(&output[written..], b"....");
///
#[inline]
pub fn b64encode_into(input: &[u8], output: &mut [u8]) -> Result<usize, Base64Error> {
    b64encode_into_with_alphabet(input, output, false)
}

/// Encodes input into caller-provided storage with the URL-safe alphabet.
///
/// The destination can contain more space than the result requires.
/// The function returns the number of bytes that it writes. It does not change bytes after this prefix.
///
/// # Arguments
///
/// * `input` - Contains the bytes to encode.
/// * `output` - Provides storage for the complete padded URL-safe Base64 result.
///
/// # Returns
///
/// The function returns the number of bytes that it writes to the start of `output`.
///
/// # Errors
///
/// The function returns `Base64Error::OutputTooSmall` before it writes if `output` is too short.
///
/// # Examples
///
///     use hashcodecs::base64::b64encode_urlsafe_into;
///
///     let mut output = [0; 4];
///     let written = b64encode_urlsafe_into(&[0xfb, 0xff], &mut output).unwrap();
///     assert_eq!(written, 4);
///     assert_eq!(&output, b"-_8=");
///
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
    let mut output = allocate_uninitialized_output(output_len);
    // The output allocation contains exactly `output_len` writable bytes.
    unsafe { encode_to_ptr(input, output.as_mut_ptr().cast(), urlsafe) };
    // `encode_to_ptr` initializes every output byte.
    let output = unsafe { assume_output_initialized(output, output_len) };

    // The encoder writes only ASCII Base64 characters.
    unsafe { String::from_utf8_unchecked(output) }
}

#[inline]
pub(crate) fn encoded_len(input_len: usize) -> usize {
    b64encoded_len(input_len).expect("Base64 input is too large")
}

#[inline]
pub(crate) fn encode_to_slice(input: &[u8], output: &mut [u8], urlsafe: bool) {
    assert_eq!(
        output.len(),
        encoded_len(input.len()),
        "Base64 output slice must have the exact encoded length"
    );
    // The assertion above confirmed the exact output length.
    unsafe { encode_to_ptr(input, output.as_mut_ptr(), urlsafe) };
}

#[inline]
pub(crate) unsafe fn encode_to_ptr(input: &[u8], output: *mut u8, urlsafe: bool) {
    unsafe { encode_to_ptr_with_store_policy(input, output, urlsafe, true) };
}

#[inline]
#[cfg(feature = "python")]
pub(crate) unsafe fn encode_to_ptr_cached(input: &[u8], output: *mut u8, urlsafe: bool) {
    unsafe { encode_to_ptr_with_store_policy(input, output, urlsafe, false) };
}

#[inline]
unsafe fn encode_to_ptr_with_store_policy(
    input: &[u8],
    output: *mut u8,
    urlsafe: bool,
    allow_streaming_stores: bool,
) {
    if input.len() < 16 {
        unsafe { encode_scalar_ptr(input, output, urlsafe) };
        return;
    }

    let input_offset =
        unsafe { encode_with_runtime_backend(input, output, urlsafe, allow_streaming_stores) };
    unsafe {
        encode_scalar_ptr(
            &input[input_offset..],
            output.add(input_offset / 3 * 4),
            urlsafe,
        )
    };
}

#[inline]
#[cfg(test)]
pub(crate) fn encode_scalar(input: &[u8], output: &mut [u8], urlsafe: bool) {
    // The caller provides the exact scalar result length.
    unsafe { encode_scalar_ptr(input, output.as_mut_ptr(), urlsafe) };
}

#[inline]
pub(crate) unsafe fn encode_scalar_ptr(input: &[u8], output: *mut u8, urlsafe: bool) {
    let alphabet = if urlsafe {
        URLSAFE_ALPHABET
    } else {
        STANDARD_ALPHABET
    };
    let input_len = input.len();
    let input_ptr = input.as_ptr();
    let mut source = 0;
    let mut destination = 0;
    // `input_ptr` comes from the live slice above. The loop or tail length check guards each raw read.
    // This structure avoids a separate slice bounds check for each byte.
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
