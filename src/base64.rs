//! Padded standard and URL-safe Base64 encoding and decoding.
//!
//! Allocating functions return owned output, while the `_into` variants write
//! into caller-provided storage. Decoders reject missing or malformed padding.
//! They intentionally permit non-zero unused bits in the final quantum, so
//! inputs such as `AB==` decode successfully; protocols requiring a canonical
//! representation must validate those trailing bits separately.

mod alphabet;
mod backend;
mod decode;
mod encode;
mod error;
mod output;

mod dispatch;

pub use decode::{
    b64decode, b64decode_into, b64decode_urlsafe, b64decode_urlsafe_into, b64decoded_len,
};
pub use encode::{
    b64encode, b64encode_into, b64encode_urlsafe, b64encode_urlsafe_into, b64encoded_len,
};
pub use error::Base64Error;

use alphabet::{
    DECODE_STORE_PADDING, INVALID_VALUE, MIXED_DECODE, STANDARD_DECODE, URLSAFE_ALPHABET,
    URLSAFE_DECODE,
};
pub(crate) use alphabet::{DecodeAlphabet, STANDARD_ALPHABET};

#[cfg(feature = "python")]
pub(crate) use decode::DecodeLayout;
#[cfg(any(feature = "python", test))]
pub(crate) use decode::{
    decode_layout, decode_to_ptr_with_layout, decode_to_ptr_with_unpadded_layout,
    decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_transactional,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_transactional, decode_unpadded_layout,
};
#[cfg(feature = "python")]
pub(crate) use encode::encode_to_ptr;
#[cfg(any(feature = "python", all(test, target_arch = "aarch64"), kani))]
pub(crate) use encode::encoded_len;

#[cfg(test)]
pub(crate) use encode::encode_scalar;

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
