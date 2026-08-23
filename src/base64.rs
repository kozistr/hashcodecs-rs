//! Padded standard and URL-safe Base64 encoding and decoding.
//!
//! Allocating functions return owned output, while the `_into` variants write
//! into caller-provided storage. Decoders reject missing or malformed padding.

mod alphabet;
mod backend;
mod decode;
mod encode;
mod error;
mod output;

#[cfg(target_arch = "aarch64")]
mod aarch64;
mod dispatch;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

pub use decode::{
    b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into, b64decoded_len,
};
pub use encode::{
    b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into, b64encoded_len,
};
pub use error::Base64Error;

pub(crate) use alphabet::DecodeAlphabet;
use alphabet::{
    DECODE_STORE_PADDING, INVALID_VALUE, MIXED_DECODE, STANDARD_ALPHABET, STANDARD_DECODE,
    URLSAFE_ALPHABET, URLSAFE_DECODE,
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
pub(crate) use encode::encode_scalar;

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
