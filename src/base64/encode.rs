//! Base64 encoding API and scalar fallback.

use super::dispatch::encode_simd_ptr;
use super::{
    Base64Error, STANDARD_ALPHABET, URLSAFE_ALPHABET, initialized_output, uninitialized_output,
};

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
