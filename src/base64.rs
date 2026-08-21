use core::{fmt, mem::ManuallyDrop, mem::MaybeUninit};

#[cfg(test)]
use crate::backend::{Capabilities, SimdBackend};
#[cfg(test)]
use backend::{Backend, is_supported as backend_supported};
#[cfg(test)]
use dispatch::{decode_with_backend, decode_with_backend_ptr, encode_with_backend};

#[cfg(test)]
fn select_backend(backend: SimdBackend) -> Backend {
    backend::select(Capabilities::for_backends(&[backend]))
}
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

mod backend;
mod decode;
mod encode;

pub use decode::{
    b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into, b64decoded_len,
};
pub use encode::{
    b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into, b64encoded_len,
};

#[allow(unused_imports)]
pub(crate) use decode::{
    DecodeLayout, decode_layout, decode_to_ptr_with_layout, decode_to_ptr_with_unpadded_layout,
    decode_to_slice_with_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
    decoded_len,
};
#[allow(unused_imports)]
pub(crate) use encode::{encode_to_ptr, encode_to_slice, encoded_len};

#[cfg(test)]
#[allow(unused_imports)]
pub(crate) use decode::{decode_eight_ptr, decode_quad_ptr, decode_unpadded_tail_ptr};
#[cfg(kani)]
pub(crate) use decode::{decode_eight_ptr, decode_quad_ptr, decode_unpadded_tail_ptr};
#[cfg(test)]
pub(crate) use encode::encode_scalar;
#[cfg(kani)]
pub(crate) use encode::encode_scalar_ptr;

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    #[kani::unwind(13)]
    fn scalar_encoder_stays_within_the_exact_output_prefix() {
        let input: [u8; 8] = kani::any();
        let length: usize = kani::any();
        let urlsafe: bool = kani::any();
        kani::assume(length <= input.len());
        let required = encoded_len(length);
        let mut output = [0xa5_u8; 12];

        unsafe { encode_scalar_ptr(&input[..length], output.as_mut_ptr(), urlsafe) };

        for byte in &output[required..] {
            assert_eq!(*byte, 0xa5);
        }
    }

    #[kani::proof]
    fn scalar_decoders_stay_within_exact_destinations() {
        let quad: [u8; 4] = kani::any();
        let padding: usize = kani::any();
        kani::assume(padding <= 2);
        let mut quad_output = [0xa5_u8; 3];
        let result =
            unsafe { decode_quad_ptr(&quad, quad_output.as_mut_ptr(), padding, &STANDARD_DECODE) };
        if result.is_ok() {
            if padding >= 1 {
                assert_eq!(quad_output[2], 0xa5);
            }
            if padding == 2 {
                assert_eq!(quad_output[1], 0xa5);
            }
        }

        let octet: [u8; 8] = kani::any();
        let mut octet_output = [0_u8; 6];
        let _ = unsafe { decode_eight_ptr(&octet, octet_output.as_mut_ptr(), &STANDARD_DECODE) };

        let tail: [u8; 3] = kani::any();
        let tail_length: usize = kani::any();
        kani::assume(matches!(tail_length, 2 | 3));
        let mut tail_output = [0xa5_u8; 2];
        let result = unsafe {
            decode_unpadded_tail_ptr(
                &tail[..tail_length],
                tail_output.as_mut_ptr(),
                &STANDARD_DECODE,
            )
        };
        if result.is_ok() && tail_length == 2 {
            assert_eq!(tail_output[1], 0xa5);
        }
    }
}

#[cfg(all(test, miri))]
mod miri_tests {
    use super::*;

    #[test]
    fn scalar_allocations_and_exact_buffers_are_defined() {
        const LENGTHS: &[usize] = &[
            0, 1, 2, 3, 4, 5, 6, 7, 8, 15, 16, 17, 31, 32, 47, 48, 63, 64, 65, 95, 96, 97, 255,
            256, 257, 1023, 1024, 1025, 4097,
        ];
        for &length in LENGTHS {
            let input = (0..length)
                .map(|index| (index as u8).wrapping_mul(53).wrapping_add(7))
                .collect::<Vec<_>>();
            for urlsafe in [false, true] {
                let encoded = if urlsafe {
                    b64encode_urlsafe(&input)
                } else {
                    b64encode(&input)
                };
                let decoded = if urlsafe {
                    b64decode_urlsafe(encoded.as_bytes())
                } else {
                    b64decode(encoded.as_bytes())
                };
                assert_eq!(decoded.as_deref(), Ok(input.as_slice()));

                let mut encoded_into = vec![0xa5; encoded.len() + 8];
                let written = if urlsafe {
                    b64encode_urlsafe_into(&input, &mut encoded_into)
                } else {
                    b64encode_into(&input, &mut encoded_into)
                };
                assert_eq!(written, Ok(encoded.len()));
                assert_eq!(&encoded_into[..encoded.len()], encoded.as_bytes());
                assert!(
                    encoded_into[encoded.len()..]
                        .iter()
                        .all(|byte| *byte == 0xa5)
                );

                let mut decoded_into = vec![0xa5; input.len() + 8];
                let written = if urlsafe {
                    b64decode_urlsafe_into(encoded.as_bytes(), &mut decoded_into)
                } else {
                    b64decode_into(encoded.as_bytes(), &mut decoded_into)
                };
                assert_eq!(written, Ok(input.len()));
                assert_eq!(&decoded_into[..input.len()], input);
                assert!(decoded_into[input.len()..].iter().all(|byte| *byte == 0xa5));
            }
        }

        assert_eq!(b64decode(b"!!!!"), Err(Base64Error::InvalidInput));
        assert_eq!(b64decode_urlsafe(b"!!!!"), Err(Base64Error::InvalidInput));
    }
}

#[cfg(target_arch = "aarch64")]
mod aarch64;

mod dispatch;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

#[cfg(test)]
mod tests;
