use core::{fmt, mem::ManuallyDrop, mem::MaybeUninit};
use std::sync::OnceLock;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Backend {
    Scalar,
    #[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
    Ssse3,
    #[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
    Avx2,
}

#[derive(Clone, Copy)]
pub(crate) enum DecodeAlphabet {
    Standard,
    UrlSafe,
    #[cfg_attr(not(feature = "python"), allow(dead_code))]
    Mixed,
}

static BACKEND: OnceLock<Backend> = OnceLock::new();

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
    let groups = input_len / 3 + if input_len % 3 == 0 { 0 } else { 1 };
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
unsafe fn encode_to_ptr(input: &[u8], output: *mut u8, urlsafe: bool) {
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
    let allocation_len = layout
        .output_len
        .checked_add(DECODE_STORE_PADDING)
        .expect("Base64 output is too large");
    let mut output = uninitialized_output(allocation_len);
    let alphabet = if urlsafe {
        DecodeAlphabet::UrlSafe
    } else {
        DecodeAlphabet::Standard
    };
    // The padded store mode may write at most `DECODE_STORE_PADDING` bytes past
    // the initialized result, all within this private allocation.
    unsafe {
        decode_to_ptr_with_layout(input, output.as_mut_ptr().cast(), layout, alphabet, true)?
    };
    // The result prefix is fully initialized; the private padding is discarded.
    Ok(unsafe { initialized_output(output, layout.output_len) })
}

#[inline]
pub(crate) fn decoded_len(input: &[u8]) -> Result<usize, Base64Error> {
    Ok(decode_layout(input)?.output_len)
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
unsafe fn decode_to_ptr_with_layout(
    input: &[u8],
    output: *mut u8,
    layout: DecodeLayout,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
) -> Result<(), Base64Error> {
    let padding = layout.padding;
    let simd_len = if padding == 0 {
        input.len()
    } else {
        input.len() - 4
    };
    let (input_offset, output_offset) =
        unsafe { decode_simd_ptr(&input[..simd_len], output, alphabet, padded_stores) }?;

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
unsafe fn encode_simd_ptr(input: &[u8], output: *mut u8, urlsafe: bool) -> usize {
    unsafe { encode_with_backend_ptr(input, output, selected_backend(), urlsafe) }
}

#[inline]
unsafe fn decode_simd_ptr(
    input: &[u8],
    output: *mut u8,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
) -> Result<(usize, usize), Base64Error> {
    unsafe { decode_with_backend_ptr(input, output, selected_backend(), alphabet, padded_stores) }
}

#[inline]
fn selected_backend() -> Backend {
    *BACKEND.get_or_init(detect_backend)
}

#[inline]
fn detect_backend() -> Backend {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        select_backend(
            std::is_x86_feature_detected!("avx2"),
            std::is_x86_feature_detected!("ssse3"),
        )
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        Backend::Scalar
    }
}

#[inline]
#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
fn select_backend(avx2: bool, ssse3: bool) -> Backend {
    if avx2 {
        Backend::Avx2
    } else if ssse3 {
        Backend::Ssse3
    } else {
        Backend::Scalar
    }
}

#[inline]
#[cfg(test)]
fn encode_with_backend(input: &[u8], output: &mut [u8], backend: Backend, urlsafe: bool) -> usize {
    // Tests and exact-output callers provide enough initialized storage.
    unsafe { encode_with_backend_ptr(input, output.as_mut_ptr(), backend, urlsafe) }
}

#[inline]
unsafe fn encode_with_backend_ptr(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    urlsafe: bool,
) -> usize {
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let _ = (input, output, backend, urlsafe);
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        match backend {
            Backend::Avx2 => {
                return std::is_x86_feature_detected!("avx2")
                    .then(|| unsafe {
                        if urlsafe {
                            x86::encode_avx2::<true>(input, output)
                        } else {
                            x86::encode_avx2::<false>(input, output)
                        }
                    })
                    .unwrap_or(0);
            }
            Backend::Ssse3 => {
                return std::is_x86_feature_detected!("ssse3")
                    .then(|| unsafe {
                        if urlsafe {
                            x86::encode_ssse3::<true>(input, output)
                        } else {
                            x86::encode_ssse3::<false>(input, output)
                        }
                    })
                    .unwrap_or(0);
            }
            Backend::Scalar => {}
        }
    }
    0
}

#[inline]
#[cfg(test)]
fn decode_with_backend(
    input: &[u8],
    output: &mut [u8],
    backend: Backend,
    alphabet: DecodeAlphabet,
) -> Result<(usize, usize), Base64Error> {
    // The slice-backed path must never write beyond the returned output.
    unsafe { decode_with_backend_ptr(input, output.as_mut_ptr(), backend, alphabet, false) }
}

#[inline]
unsafe fn decode_with_backend_ptr(
    input: &[u8],
    output: *mut u8,
    backend: Backend,
    alphabet: DecodeAlphabet,
    padded_stores: bool,
) -> Result<(usize, usize), Base64Error> {
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let _ = (input, output, backend, alphabet, padded_stores);
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        match backend {
            Backend::Avx2 => {
                return std::is_x86_feature_detected!("avx2")
                    .then(|| unsafe {
                        match alphabet {
                            DecodeAlphabet::Standard => {
                                if padded_stores {
                                    x86::decode_avx2::<x86::StandardDecoder, x86::PaddedStore>(
                                        input, output,
                                    )
                                } else {
                                    x86::decode_avx2::<x86::StandardDecoder, x86::ExactStore>(
                                        input, output,
                                    )
                                }
                            }
                            DecodeAlphabet::UrlSafe => {
                                if padded_stores {
                                    x86::decode_avx2::<x86::UrlSafeDecoder, x86::PaddedStore>(
                                        input, output,
                                    )
                                } else {
                                    x86::decode_avx2::<x86::UrlSafeDecoder, x86::ExactStore>(
                                        input, output,
                                    )
                                }
                            }
                            DecodeAlphabet::Mixed => x86::decode_avx2::<
                                x86::MixedDecoder,
                                x86::ExactStore,
                            >(input, output),
                        }
                    })
                    .unwrap_or(Ok((0, 0)));
            }
            Backend::Ssse3 => {
                return std::is_x86_feature_detected!("ssse3")
                    .then(|| unsafe {
                        match alphabet {
                            DecodeAlphabet::Standard => {
                                if padded_stores {
                                    x86::decode_ssse3::<x86::StandardDecoder, x86::PaddedStore>(
                                        input, output,
                                    )
                                } else {
                                    x86::decode_ssse3::<x86::StandardDecoder, x86::ExactStore>(
                                        input, output,
                                    )
                                }
                            }
                            DecodeAlphabet::UrlSafe => {
                                if padded_stores {
                                    x86::decode_ssse3::<x86::UrlSafeDecoder, x86::PaddedStore>(
                                        input, output,
                                    )
                                } else {
                                    x86::decode_ssse3::<x86::UrlSafeDecoder, x86::ExactStore>(
                                        input, output,
                                    )
                                }
                            }
                            DecodeAlphabet::Mixed => x86::decode_ssse3::<
                                x86::MixedDecoder,
                                x86::ExactStore,
                            >(input, output),
                        }
                    })
                    .unwrap_or(Ok((0, 0)));
            }
            Backend::Scalar => {}
        }
    }
    Ok((0, 0))
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
    let mut source = 0;
    let mut destination = 0;
    while source + 6 <= input.len() {
        let first = ((input[source] as u32) << 16)
            | ((input[source + 1] as u32) << 8)
            | input[source + 2] as u32;
        let second = ((input[source + 3] as u32) << 16)
            | ((input[source + 4] as u32) << 8)
            | input[source + 5] as u32;
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
    while source + 3 <= input.len() {
        let block = ((input[source] as u32) << 16)
            | ((input[source + 1] as u32) << 8)
            | input[source + 2] as u32;
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

    let remaining = input.len() - source;
    if remaining == 1 {
        let block = (input[source] as u32) << 16;
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
        let block = ((input[source] as u32) << 16) | ((input[source + 1] as u32) << 8);
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

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86 {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    use super::Base64Error;

    pub(super) struct StandardDecoder;
    pub(super) struct UrlSafeDecoder;
    pub(super) struct MixedDecoder;
    pub(super) struct ExactStore;
    pub(super) struct PaddedStore;

    pub(super) trait Decoder {
        unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i);
        unsafe fn decode_16(input: *const u8) -> Option<__m128i>;
    }

    pub(super) trait Store {
        unsafe fn store_12(output: *mut u8, value: __m128i);
        unsafe fn store_24(output: *mut u8, value: __m256i);
    }

    impl Store for ExactStore {
        #[inline(always)]
        unsafe fn store_12(output: *mut u8, value: __m128i) {
            unsafe { store_12_exact(output, value) };
        }

        #[inline(always)]
        unsafe fn store_24(output: *mut u8, value: __m256i) {
            unsafe { store_24_exact(output, value) };
        }
    }

    impl Store for PaddedStore {
        #[inline(always)]
        unsafe fn store_12(output: *mut u8, value: __m128i) {
            unsafe { store_12_padded(output, value) };
        }

        #[inline(always)]
        unsafe fn store_24(output: *mut u8, value: __m256i) {
            unsafe { store_24_padded(output, value) };
        }
    }

    impl Decoder for StandardDecoder {
        #[inline(always)]
        unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
            unsafe { decode_indices_32_standard(input) }
        }

        #[inline(always)]
        unsafe fn decode_16(input: *const u8) -> Option<__m128i> {
            unsafe { decode_16_standard(input) }
        }
    }

    impl Decoder for UrlSafeDecoder {
        #[inline(always)]
        unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
            unsafe { decode_indices_32_urlsafe(input) }
        }

        #[inline(always)]
        unsafe fn decode_16(input: *const u8) -> Option<__m128i> {
            unsafe { decode_16_urlsafe(input) }
        }
    }

    impl Decoder for MixedDecoder {
        #[inline(always)]
        unsafe fn decode_32(input: *const u8) -> (__m256i, __m256i) {
            unsafe { decode_indices_32_mixed(input) }
        }

        #[inline(always)]
        unsafe fn decode_16(input: *const u8) -> Option<__m128i> {
            unsafe { decode_16_mixed(input) }
        }
    }

    #[target_feature(enable = "ssse3")]
    pub unsafe fn encode_ssse3<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
        let mut source = 0;
        let mut destination = 0;
        // Loading a vector reads 16 bytes, so leave enough bytes for the load.
        while source + 16 <= input.len() {
            let encoded = unsafe { encode_12::<URLSAFE>(input.as_ptr().add(source)) };
            unsafe { _mm_storeu_si128(output.add(destination).cast(), encoded) };
            source += 12;
            destination += 16;
        }
        source
    }

    #[target_feature(enable = "avx2")]
    pub unsafe fn encode_avx2<const URLSAFE: bool>(input: &[u8], output: *mut u8) -> usize {
        let mut source = 0;
        let mut destination = 0;
        while source + 104 <= input.len() {
            let first = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source)) };
            let second = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source + 24)) };
            let third = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source + 48)) };
            let fourth = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source + 72)) };
            unsafe { _mm256_storeu_si256(output.add(destination).cast(), first) };
            unsafe { _mm256_storeu_si256(output.add(destination + 32).cast(), second) };
            unsafe { _mm256_storeu_si256(output.add(destination + 64).cast(), third) };
            unsafe { _mm256_storeu_si256(output.add(destination + 96).cast(), fourth) };
            source += 96;
            destination += 128;
        }
        while source + 32 <= input.len() {
            let encoded = unsafe { encode_24::<URLSAFE>(input.as_ptr().add(source)) };
            unsafe { _mm256_storeu_si256(output.add(destination).cast(), encoded) };
            source += 24;
            destination += 32;
        }
        // The remainder still benefits from the SSSE3 kernel.
        source + unsafe { encode_ssse3::<URLSAFE>(&input[source..], output.add(destination)) }
    }

    #[target_feature(enable = "avx2")]
    pub(super) unsafe fn decode_avx2<A: Decoder, S: Store>(
        input: &[u8],
        output: *mut u8,
    ) -> Result<(usize, usize), Base64Error> {
        let mut source = 0;
        let mut destination = 0;
        while source + 128 <= input.len() {
            let (first, first_error) = unsafe { A::decode_32(input.as_ptr().add(source)) };
            let (second, second_error) = unsafe { A::decode_32(input.as_ptr().add(source + 32)) };
            let (third, third_error) = unsafe { A::decode_32(input.as_ptr().add(source + 64)) };
            let (fourth, fourth_error) = unsafe { A::decode_32(input.as_ptr().add(source + 96)) };
            let errors = _mm256_or_si256(
                _mm256_or_si256(first_error, second_error),
                _mm256_or_si256(third_error, fourth_error),
            );
            if _mm256_testz_si256(errors, errors) == 0 {
                return Err(Base64Error::InvalidInput);
            }
            unsafe { S::store_24(output.add(destination), pack_32(first)) };
            unsafe { S::store_24(output.add(destination + 24), pack_32(second)) };
            unsafe { S::store_24(output.add(destination + 48), pack_32(third)) };
            unsafe { S::store_24(output.add(destination + 72), pack_32(fourth)) };
            source += 128;
            destination += 96;
        }
        while source + 32 <= input.len() {
            let (indices, errors) = unsafe { A::decode_32(input.as_ptr().add(source)) };
            if _mm256_testz_si256(errors, errors) == 0 {
                return Err(Base64Error::InvalidInput);
            }
            unsafe { S::store_24(output.add(destination), pack_32(indices)) };
            source += 32;
            destination += 24;
        }
        let (tail_source, tail_destination) =
            unsafe { decode_ssse3::<A, S>(&input[source..], output.add(destination)) }?;
        Ok((source + tail_source, destination + tail_destination))
    }

    #[target_feature(enable = "ssse3")]
    pub(super) unsafe fn decode_ssse3<A: Decoder, S: Store>(
        input: &[u8],
        output: *mut u8,
    ) -> Result<(usize, usize), Base64Error> {
        let mut source = 0;
        let mut destination = 0;
        while source + 16 <= input.len() {
            let decoded = unsafe { A::decode_16(input.as_ptr().add(source)) }
                .ok_or(Base64Error::InvalidInput)?;
            unsafe { S::store_12(output.add(destination), decoded) };
            source += 16;
            destination += 12;
        }
        Ok((source, destination))
    }

    #[target_feature(enable = "ssse3")]
    unsafe fn encode_12<const URLSAFE: bool>(input: *const u8) -> __m128i {
        let shuffle = _mm_setr_epi8(1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10);
        let mut value = unsafe { _mm_loadu_si128(input.cast()) };
        value = _mm_shuffle_epi8(value, shuffle);

        let higher = _mm_and_si128(value, _mm_set1_epi32(0x0fc0_fc00));
        let higher = _mm_mulhi_epu16(higher, _mm_set1_epi32(0x0400_0040));
        let lower = _mm_and_si128(value, _mm_set1_epi32(0x003f_03f0));
        let lower = _mm_mullo_epi16(lower, _mm_set1_epi32(0x0100_0010));
        ascii_from_indices::<URLSAFE>(_mm_or_si128(higher, lower))
    }

    #[target_feature(enable = "avx2")]
    unsafe fn encode_24<const URLSAFE: bool>(input: *const u8) -> __m256i {
        let value = unsafe { _mm256_loadu_si256(input.cast()) };
        let value = _mm256_permutevar8x32_epi32(value, _mm256_setr_epi32(0, 1, 2, 3, 3, 4, 5, 6));
        let shuffle = _mm256_broadcastsi128_si256(_mm_setr_epi8(
            1, 0, 2, 1, 4, 3, 5, 4, 7, 6, 8, 7, 10, 9, 11, 10,
        ));
        let value = _mm256_shuffle_epi8(value, shuffle);

        let higher = _mm256_and_si256(value, _mm256_set1_epi32(0x0fc0_fc00));
        let higher = _mm256_mulhi_epu16(higher, _mm256_set1_epi32(0x0400_0040));
        let lower = _mm256_and_si256(value, _mm256_set1_epi32(0x003f_03f0));
        let lower = _mm256_mullo_epi16(lower, _mm256_set1_epi32(0x0100_0010));
        ascii_from_indices_avx2::<URLSAFE>(_mm256_or_si256(higher, lower))
    }

    #[target_feature(enable = "ssse3")]
    unsafe fn decode_16_standard(input: *const u8) -> Option<__m128i> {
        let value = unsafe { _mm_loadu_si128(input.cast()) };
        let (indices, valid) = ascii_to_indices_standard(value);
        decode_16_indices(indices, valid)
    }

    #[target_feature(enable = "ssse3")]
    unsafe fn decode_16_urlsafe(input: *const u8) -> Option<__m128i> {
        let value = unsafe { _mm_loadu_si128(input.cast()) };
        let (indices, valid) = ascii_to_indices_urlsafe(value);
        decode_16_indices(indices, valid)
    }

    #[target_feature(enable = "ssse3")]
    unsafe fn decode_16_mixed(input: *const u8) -> Option<__m128i> {
        let value = unsafe { _mm_loadu_si128(input.cast()) };
        let (indices, valid) = ascii_to_indices_mixed(value);
        decode_16_indices(indices, valid)
    }

    #[target_feature(enable = "ssse3")]
    fn decode_16_indices(indices: __m128i, valid: __m128i) -> Option<__m128i> {
        if _mm_movemask_epi8(valid) != 0xffff {
            return None;
        }

        let merged = _mm_maddubs_epi16(indices, _mm_set1_epi32(0x0140_0140));
        let packed = _mm_madd_epi16(merged, _mm_set1_epi32(0x0001_1000));
        let shuffle = _mm_setr_epi8(2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1);
        Some(_mm_shuffle_epi8(packed, shuffle))
    }

    #[target_feature(enable = "avx2")]
    unsafe fn decode_indices_32_standard(input: *const u8) -> (__m256i, __m256i) {
        let (value, mut indices) = unsafe { decode_indices_32_base(input) };
        let special_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'+' as i8));
        let special_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
        indices = _mm256_add_epi8(indices, _mm256_and_si256(special_63, _mm256_set1_epi8(-3)));
        decode_indices_32_finish(value, indices, special_62, special_63)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn decode_indices_32_urlsafe(input: *const u8) -> (__m256i, __m256i) {
        let (value, mut indices) = unsafe { decode_indices_32_base(input) };
        let special_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'-' as i8));
        let special_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
        let corrections = _mm256_or_si256(
            _mm256_and_si256(special_62, _mm256_set1_epi8(-2)),
            _mm256_and_si256(special_63, _mm256_set1_epi8(33)),
        );
        indices = _mm256_add_epi8(indices, corrections);
        decode_indices_32_finish(value, indices, special_62, special_63)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn decode_indices_32_mixed(input: *const u8) -> (__m256i, __m256i) {
        let (value, mut indices) = unsafe { decode_indices_32_base(input) };
        let standard_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'+' as i8));
        let standard_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'/' as i8));
        let urlsafe_62 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'-' as i8));
        let urlsafe_63 = _mm256_cmpeq_epi8(value, _mm256_set1_epi8(b'_' as i8));
        let special_62 = _mm256_or_si256(standard_62, urlsafe_62);
        let special_63 = _mm256_or_si256(standard_63, urlsafe_63);
        let corrections = _mm256_or_si256(
            _mm256_and_si256(urlsafe_62, _mm256_set1_epi8(-2)),
            _mm256_or_si256(
                _mm256_and_si256(standard_63, _mm256_set1_epi8(-3)),
                _mm256_and_si256(urlsafe_63, _mm256_set1_epi8(33)),
            ),
        );
        indices = _mm256_add_epi8(indices, corrections);
        decode_indices_32_finish(value, indices, special_62, special_63)
    }

    #[target_feature(enable = "avx2")]
    unsafe fn decode_indices_32_base(input: *const u8) -> (__m256i, __m256i) {
        let value = unsafe { _mm256_loadu_si256(input.cast()) };
        let high_nibbles = _mm256_and_si256(_mm256_srli_epi16(value, 4), _mm256_set1_epi8(0x0f));
        let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
            0, 0, 19, 4, -65, -65, -71, -71, 0, 0, 0, 0, 0, 0, 0, 0,
        ));
        (
            value,
            _mm256_add_epi8(value, _mm256_shuffle_epi8(offsets, high_nibbles)),
        )
    }

    #[target_feature(enable = "avx2")]
    fn decode_indices_32_finish(
        value: __m256i,
        indices: __m256i,
        special_62: __m256i,
        special_63: __m256i,
    ) -> (__m256i, __m256i) {
        let digits = range_errors_avx2(value, b'0', 9);
        let uppercase = range_errors_avx2(value, b'A', 25);
        let lowercase = range_errors_avx2(value, b'a', 25);
        let range_errors = _mm256_min_epu8(digits, _mm256_min_epu8(uppercase, lowercase));
        let symbols = _mm256_or_si256(special_62, special_63);
        (indices, _mm256_andnot_si256(symbols, range_errors))
    }

    #[target_feature(enable = "avx2")]
    fn range_errors_avx2(value: __m256i, start: u8, length: i8) -> __m256i {
        _mm256_subs_epu8(
            _mm256_sub_epi8(value, _mm256_set1_epi8(start as i8)),
            _mm256_set1_epi8(length),
        )
    }

    #[target_feature(enable = "avx2")]
    fn pack_32(indices: __m256i) -> __m256i {
        let merged = _mm256_maddubs_epi16(indices, _mm256_set1_epi32(0x0140_0140));
        let packed = _mm256_madd_epi16(merged, _mm256_set1_epi32(0x0001_1000));
        let shuffle = _mm256_broadcastsi128_si256(_mm_setr_epi8(
            2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, -1, -1, -1, -1,
        ));
        _mm256_shuffle_epi8(packed, shuffle)
    }

    #[target_feature(enable = "ssse3")]
    fn ascii_from_indices<const URLSAFE: bool>(indices: __m128i) -> __m128i {
        let upper = _mm_add_epi8(indices, _mm_set1_epi8(b'A' as i8));
        let lower = _mm_add_epi8(indices, _mm_set1_epi8((b'a' - 26) as i8));
        let digit = _mm_add_epi8(indices, _mm_set1_epi8(-4));
        let mut output = select(_mm_cmpgt_epi8(indices, _mm_set1_epi8(25)), lower, upper);
        output = select(_mm_cmpgt_epi8(indices, _mm_set1_epi8(51)), digit, output);
        output = select(
            _mm_cmpeq_epi8(indices, _mm_set1_epi8(62)),
            _mm_set1_epi8(if URLSAFE { b'-' } else { b'+' } as i8),
            output,
        );
        select(
            _mm_cmpeq_epi8(indices, _mm_set1_epi8(63)),
            _mm_set1_epi8(if URLSAFE { b'_' } else { b'/' } as i8),
            output,
        )
    }

    #[target_feature(enable = "avx2")]
    fn ascii_from_indices_avx2<const URLSAFE: bool>(indices: __m256i) -> __m256i {
        let reduced = _mm256_subs_epu8(indices, _mm256_set1_epi8(51));
        let less = _mm256_cmpgt_epi8(_mm256_set1_epi8(26), indices);
        let reduced = _mm256_or_si256(reduced, _mm256_and_si256(less, _mm256_set1_epi8(13)));
        let offsets = _mm256_broadcastsi128_si256(_mm_setr_epi8(
            b'G' as i8,
            -4,
            -4,
            -4,
            -4,
            -4,
            -4,
            -4,
            -4,
            -4,
            -4,
            if URLSAFE { -17 } else { -19 },
            if URLSAFE { 32 } else { -16 },
            b'A' as i8,
            0,
            0,
        ));
        _mm256_add_epi8(_mm256_shuffle_epi8(offsets, reduced), indices)
    }

    #[target_feature(enable = "ssse3")]
    fn ascii_to_indices_standard(value: __m128i) -> (__m128i, __m128i) {
        ascii_to_indices_with_symbols(
            value,
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'+' as i8)),
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8)),
        )
    }

    #[target_feature(enable = "ssse3")]
    fn ascii_to_indices_urlsafe(value: __m128i) -> (__m128i, __m128i) {
        ascii_to_indices_with_symbols(
            value,
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8)),
            _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8)),
        )
    }

    #[target_feature(enable = "ssse3")]
    fn ascii_to_indices_mixed(value: __m128i) -> (__m128i, __m128i) {
        ascii_to_indices_with_symbols(
            value,
            _mm_or_si128(
                _mm_cmpeq_epi8(value, _mm_set1_epi8(b'+' as i8)),
                _mm_cmpeq_epi8(value, _mm_set1_epi8(b'-' as i8)),
            ),
            _mm_or_si128(
                _mm_cmpeq_epi8(value, _mm_set1_epi8(b'/' as i8)),
                _mm_cmpeq_epi8(value, _mm_set1_epi8(b'_' as i8)),
            ),
        )
    }

    #[target_feature(enable = "ssse3")]
    fn ascii_to_indices_with_symbols(
        value: __m128i,
        special_62: __m128i,
        special_63: __m128i,
    ) -> (__m128i, __m128i) {
        let upper = between(value, b'A', b'Z');
        let lower = between(value, b'a', b'z');
        let digit = between(value, b'0', b'9');

        let mut indices = _mm_sub_epi8(value, _mm_set1_epi8(b'A' as i8));
        indices = select(
            lower,
            _mm_sub_epi8(value, _mm_set1_epi8((b'a' - 26) as i8)),
            indices,
        );
        indices = select(digit, _mm_add_epi8(value, _mm_set1_epi8(4)), indices);
        indices = select(special_62, _mm_set1_epi8(62), indices);
        indices = select(special_63, _mm_set1_epi8(63), indices);
        (
            indices,
            _mm_or_si128(
                _mm_or_si128(upper, lower),
                _mm_or_si128(digit, _mm_or_si128(special_62, special_63)),
            ),
        )
    }

    #[target_feature(enable = "ssse3")]
    fn between(value: __m128i, lower: u8, upper: u8) -> __m128i {
        let above_lower = _mm_cmpgt_epi8(value, _mm_set1_epi8((lower - 1) as i8));
        let below_upper = _mm_cmpgt_epi8(_mm_set1_epi8((upper + 1) as i8), value);
        _mm_and_si128(above_lower, below_upper)
    }

    #[target_feature(enable = "ssse3")]
    fn select(mask: __m128i, yes: __m128i, no: __m128i) -> __m128i {
        _mm_or_si128(_mm_and_si128(mask, yes), _mm_andnot_si128(mask, no))
    }

    #[target_feature(enable = "ssse3")]
    unsafe fn store_12_exact(output: *mut u8, value: __m128i) {
        unsafe { _mm_storel_epi64(output.cast(), value) };
        let remaining = _mm_cvtsi128_si32(_mm_srli_si128(value, 8));
        unsafe { output.add(8).cast::<i32>().write_unaligned(remaining) };
    }

    #[target_feature(enable = "avx2")]
    unsafe fn store_24_exact(output: *mut u8, value: __m256i) {
        let lower = _mm256_castsi256_si128(value);
        let upper = _mm256_extracti128_si256(value, 1);
        // The first store's four lane-padding bytes are replaced by the second.
        unsafe { _mm_storeu_si128(output.cast(), lower) };
        unsafe { _mm_storel_epi64(output.add(12).cast(), upper) };
        let remaining = _mm_cvtsi128_si32(_mm_srli_si128(upper, 8));
        unsafe { output.add(20).cast::<i32>().write_unaligned(remaining) };
    }

    #[target_feature(enable = "ssse3")]
    unsafe fn store_12_padded(output: *mut u8, value: __m128i) {
        unsafe { _mm_storeu_si128(output.cast(), value) };
    }

    #[target_feature(enable = "avx2")]
    unsafe fn store_24_padded(output: *mut u8, value: __m256i) {
        unsafe { _mm_storeu_si128(output.cast(), _mm256_castsi256_si128(value)) };
        unsafe { _mm_storeu_si128(output.add(12).cast(), _mm256_extracti128_si256(value, 1)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    #[test]
    fn standard_and_url_safe_round_trip() {
        let input = b"the quick brown fox jumps over the lazy dog";
        assert_eq!(
            b64encode(input),
            "dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw=="
        );
        assert_eq!(
            b64decode(b"dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVyIHRoZSBsYXp5IGRvZw==").unwrap(),
            input
        );
        assert_eq!(b64encode_urlsafe(b"\xfb\xff"), "-_8=");
        assert_eq!(b64decode_urlsafe(b"-_8=").unwrap(), b"\xfb\xff");
        assert_eq!(b64decode(b"YQ==").unwrap(), b"a");
        assert_eq!(b64decode(b"YWI=").unwrap(), b"ab");
    }

    #[test]
    fn rejects_invalid_input() {
        assert_eq!(b64decode(b"AAAA!AAA"), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode(b"abc"), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode(b"A"), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode(b"A=AA"), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode(b"AA=A"), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode(b"===="), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode(b"Y!=="), Err(Base64Error::InvalidInput));
        assert_eq!(
            b64decode(b"AAAAAAAAAAAAAAA!"),
            Err(Base64Error::InvalidInput)
        );
        assert_eq!(
            b64decode(b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA!"),
            Err(Base64Error::InvalidInput)
        );
        let mut invalid_wide = [b'A'; 128];
        invalid_wide[127] = b'!';
        assert_eq!(b64decode(&invalid_wide), Err(Base64Error::InvalidInput));
    }

    #[test]
    fn matches_the_standard_engine_for_all_short_lengths() {
        for length in 0..=1024 {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
                .collect();
            let expected = base64::engine::general_purpose::STANDARD.encode(&input);
            assert_eq!(b64encode(&input), expected, "length={length}");
            assert_eq!(
                b64decode(expected.as_bytes()).unwrap(),
                input,
                "length={length}"
            );

            let url_safe = base64::engine::general_purpose::URL_SAFE.encode(&input);
            assert_eq!(b64encode_urlsafe(&input), url_safe, "length={length}");
            assert_eq!(
                b64decode_urlsafe(url_safe.as_bytes()).unwrap(),
                input,
                "url-safe length={length}"
            );
        }
    }

    #[test]
    fn backend_selection_and_kernels_match_scalar_output() {
        assert_eq!(select_backend(false, false), Backend::Scalar);
        assert_eq!(select_backend(false, true), Backend::Ssse3);
        assert_eq!(select_backend(true, false), Backend::Avx2);
        assert_eq!(select_backend(true, true), Backend::Avx2);
        assert_eq!(
            Base64Error::InvalidInput.to_string(),
            "invalid Base64 input"
        );

        let input: Vec<u8> = (0..96).map(|value| value as u8).collect();
        let expected = b64encode(&input);
        let mut scalar = vec![0; expected.len()];
        assert_eq!(
            encode_with_backend(&input, &mut scalar, Backend::Scalar, false),
            0
        );
        encode_scalar(&input, &mut scalar, false);
        assert_eq!(scalar, expected.as_bytes());
        let mut scalar_decoded = vec![0; input.len()];
        assert_eq!(
            decode_with_backend(
                expected.as_bytes(),
                &mut scalar_decoded,
                Backend::Scalar,
                DecodeAlphabet::Standard,
            )
            .unwrap(),
            (0, 0)
        );

        let mut ssse3 = vec![0; expected.len()];
        let consumed = encode_with_backend(&input, &mut ssse3, Backend::Ssse3, false);
        encode_scalar(&input[consumed..], &mut ssse3[consumed / 3 * 4..], false);
        assert_eq!(ssse3, expected.as_bytes());

        let expected_urlsafe = b64encode_urlsafe(&input);
        let mut ssse3_urlsafe = vec![0; expected_urlsafe.len()];
        let consumed_urlsafe =
            encode_with_backend(&input, &mut ssse3_urlsafe, Backend::Ssse3, true);
        encode_scalar(
            &input[consumed_urlsafe..],
            &mut ssse3_urlsafe[consumed_urlsafe / 3 * 4..],
            true,
        );
        assert_eq!(ssse3_urlsafe, expected_urlsafe.as_bytes());

        let mut ssse3_decoded = vec![0; input.len()];
        let decoded = decode_with_backend(
            expected.as_bytes(),
            &mut ssse3_decoded,
            Backend::Ssse3,
            DecodeAlphabet::Standard,
        )
        .unwrap();
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let has_ssse3 = std::is_x86_feature_detected!("ssse3");
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        let has_ssse3 = false;
        let mut expected_decoded = (0, 0);
        if has_ssse3 {
            expected_decoded = (expected.len(), input.len());
        }
        assert_eq!(decoded, expected_decoded);
        assert!(!has_ssse3 || ssse3_decoded == input);

        let mut ssse3_urlsafe_decoded = vec![0; input.len()];
        let urlsafe_decoded = decode_with_backend(
            expected_urlsafe.as_bytes(),
            &mut ssse3_urlsafe_decoded,
            Backend::Ssse3,
            DecodeAlphabet::UrlSafe,
        )
        .unwrap();
        assert_eq!(urlsafe_decoded, expected_decoded);
        assert!(!has_ssse3 || ssse3_urlsafe_decoded == input);

        let mixed = b"-///".repeat(32);
        let mixed_expected = [0xfb, 0xff, 0xff].repeat(32);
        let mut mixed_decoded = vec![0; mixed_expected.len()];
        let mixed_offsets = decode_with_backend(
            &mixed,
            &mut mixed_decoded,
            selected_backend(),
            DecodeAlphabet::Mixed,
        )
        .unwrap();
        assert_eq!(mixed_offsets, expected_decoded);
        assert!(!has_ssse3 || mixed_decoded == mixed_expected);

        let mut scalar_mixed = [0; 3];
        decode_to_slice_with_layout_and_alphabet(
            b"-///",
            &mut scalar_mixed,
            decode_layout(b"-///").unwrap(),
            DecodeAlphabet::Mixed,
        )
        .unwrap();
        assert_eq!(scalar_mixed, [0xfb, 0xff, 0xff]);

        let mut ssse3_mixed = vec![0; mixed_expected.len()];
        let ssse3_mixed_offsets = decode_with_backend(
            &mixed,
            &mut ssse3_mixed,
            Backend::Ssse3,
            DecodeAlphabet::Mixed,
        )
        .unwrap();
        assert_eq!(ssse3_mixed_offsets, expected_decoded);
        assert!(!has_ssse3 || ssse3_mixed == mixed_expected);
    }

    #[test]
    fn length_helpers_and_buffer_errors_are_precise() {
        assert_eq!(b64encoded_len(0), Some(0));
        assert_eq!(b64encoded_len(1), Some(4));
        assert_eq!(b64encoded_len(2), Some(4));
        assert_eq!(b64encoded_len(3), Some(4));
        assert_eq!(b64encoded_len(4), Some(8));
        assert_eq!(b64encoded_len(usize::MAX), None);
        assert_eq!(b64decoded_len(b""), Ok(0));
        assert_eq!(b64decoded_len(b"YQ=="), Ok(1));
        assert_eq!(b64decoded_len(b"YWI="), Ok(2));
        assert_eq!(b64decoded_len(b"YWJj"), Ok(3));
        assert_eq!(b64decoded_len(b"abc"), Err(Base64Error::InvalidInput));

        let error = Base64Error::OutputTooSmall {
            required: 8,
            provided: 3,
        };
        assert_eq!(
            error.to_string(),
            "Base64 output requires 8 bytes but the destination has 3"
        );

        let mut encoded = [0xa5; 3];
        assert_eq!(
            b64encode_into(b"hello", &mut encoded),
            Err(Base64Error::OutputTooSmall {
                required: 8,
                provided: 3,
            })
        );
        assert_eq!(encoded, [0xa5; 3]);

        let mut decoded = [0xa5; 2];
        assert_eq!(
            b64decode_into(b"aGVsbG8=", &mut decoded),
            Err(Base64Error::OutputTooSmall {
                required: 5,
                provided: 2,
            })
        );
        assert_eq!(decoded, [0xa5; 2]);
    }

    #[test]
    fn decode_tables_cover_both_alphabets() {
        for (urlsafe, mixed) in [(false, false), (true, false), (true, true)] {
            let table = decode_table(std::hint::black_box(urlsafe), std::hint::black_box(mixed));
            for (index, &byte) in STANDARD_ALPHABET.iter().enumerate() {
                let expected = if urlsafe && !mixed && index >= 62 {
                    INVALID_VALUE
                } else {
                    index as u8
                };
                assert_eq!(table[byte as usize], expected);
            }
            for (index, &byte) in URLSAFE_ALPHABET.iter().enumerate() {
                let expected = if !urlsafe && !mixed && index >= 62 {
                    INVALID_VALUE
                } else {
                    index as u8
                };
                assert_eq!(table[byte as usize], expected);
            }
            assert_eq!(table[b'!' as usize], INVALID_VALUE);
        }
    }

    #[test]
    fn buffer_apis_respect_exact_slice_boundaries() {
        const GUARD: usize = 32;
        const CANARY: u8 = 0xa5;

        for length in 0..=1024 {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
                .collect();

            for urlsafe in [false, true] {
                let expected = if urlsafe {
                    base64::engine::general_purpose::URL_SAFE.encode(&input)
                } else {
                    base64::engine::general_purpose::STANDARD.encode(&input)
                };
                let encoded_len = expected.len();
                let mut encoded = vec![CANARY; encoded_len + GUARD * 2];
                let written = if urlsafe {
                    b64encode_urlsafe_into(&input, &mut encoded[GUARD..])
                } else {
                    b64encode_into(&input, &mut encoded[GUARD..])
                }
                .unwrap();
                assert_eq!(written, encoded_len, "encode length={length}");
                assert_eq!(
                    &encoded[GUARD..GUARD + encoded_len],
                    expected.as_bytes(),
                    "encode length={length} urlsafe={urlsafe}"
                );
                assert!(encoded[..GUARD].iter().all(|&byte| byte == CANARY));
                assert!(
                    encoded[GUARD + encoded_len..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );

                let mut decoded = vec![CANARY; length + GUARD * 2];
                let written = if urlsafe {
                    b64decode_urlsafe_into(expected.as_bytes(), &mut decoded[GUARD..])
                } else {
                    b64decode_into(expected.as_bytes(), &mut decoded[GUARD..])
                }
                .unwrap();
                assert_eq!(written, length, "decode length={length}");
                assert_eq!(
                    &decoded[GUARD..GUARD + length],
                    input,
                    "decode length={length} urlsafe={urlsafe}"
                );
                assert!(decoded[..GUARD].iter().all(|&byte| byte == CANARY));
                assert!(decoded[GUARD + length..].iter().all(|&byte| byte == CANARY));
            }
        }
    }

    #[test]
    fn padded_decoder_stores_stay_within_four_bytes_of_slack() {
        const GUARD: usize = 32;
        const CANARY: u8 = 0xa5;

        for length in 0..=1024 {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
                .collect();

            for (encoded, alphabet) in [
                (
                    base64::engine::general_purpose::STANDARD.encode(&input),
                    DecodeAlphabet::Standard,
                ),
                (
                    base64::engine::general_purpose::URL_SAFE.encode(&input),
                    DecodeAlphabet::UrlSafe,
                ),
            ] {
                let layout = decode_layout(encoded.as_bytes()).unwrap();
                let mut output =
                    vec![CANARY; GUARD + layout.output_len + DECODE_STORE_PADDING + GUARD];
                unsafe {
                    decode_to_ptr_with_layout(
                        encoded.as_bytes(),
                        output.as_mut_ptr().add(GUARD),
                        layout,
                        alphabet,
                        true,
                    )
                }
                .unwrap();

                assert_eq!(&output[GUARD..GUARD + length], input);
                assert!(output[..GUARD].iter().all(|&byte| byte == CANARY));
                assert!(
                    output[GUARD + length + DECODE_STORE_PADDING..]
                        .iter()
                        .all(|&byte| byte == CANARY)
                );
            }
        }

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let has_ssse3 = std::is_x86_feature_detected!("ssse3");
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        let has_ssse3 = false;
        let input: Vec<u8> = (0..96).map(|value| value as u8).collect();
        for (encoded, alphabet) in [
            (b64encode(&input), DecodeAlphabet::Standard),
            (b64encode_urlsafe(&input), DecodeAlphabet::UrlSafe),
        ] {
            let mut output = vec![CANARY; input.len() + DECODE_STORE_PADDING + GUARD];
            let offsets = unsafe {
                decode_with_backend_ptr(
                    encoded.as_bytes(),
                    output.as_mut_ptr(),
                    Backend::Ssse3,
                    alphabet,
                    true,
                )
            }
            .unwrap();
            let mut expected_offsets = (0, 0);
            if has_ssse3 {
                expected_offsets = (encoded.len(), input.len());
            }
            assert_eq!(offsets, expected_offsets);
            assert!(!has_ssse3 || output[..input.len()] == input);
            assert!(
                output[input.len() + DECODE_STORE_PADDING..]
                    .iter()
                    .all(|&byte| byte == CANARY)
            );
        }
    }
}
