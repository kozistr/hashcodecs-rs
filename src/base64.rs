//! This module encodes and decodes padded standard or URL-safe Base64 data.
//!
//! Functions without an `_into` suffix allocate their output. Functions with this suffix write to caller-provided storage.
//! Decoders reject missing or malformed padding. They accept nonzero unused bits in the final quantum.
//! For example, the decoders accept `AB==`. Protocols that require canonical data must check the unused bits.

mod alphabet;
mod backend;
mod decode;
mod encode;
mod error;
mod output_buffer;

mod runtime_dispatch;

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
#[cfg(all(feature = "python", any(target_arch = "x86", target_arch = "x86_64")))]
pub(crate) use decode::{STANDARD_HIGH_CLASSES, STANDARD_LOW_CLASSES_COMPLEMENT};
#[cfg(any(feature = "python", test))]
pub(crate) use decode::{
    decode_layout, decode_standard_validated_to_ptr, decode_to_ptr_with_layout,
    decode_to_ptr_with_unpadded_layout, decode_to_slice_with_layout_and_alphabet,
    decode_to_slice_with_layout_and_alphabet_validated_blocks,
    decode_to_slice_with_unpadded_layout_and_alphabet,
    decode_to_slice_with_unpadded_layout_and_alphabet_validated_blocks, decode_unpadded_layout,
    decode_valid_prefix, validate_alphabet,
};
#[cfg(any(feature = "python", all(test, target_arch = "aarch64"), kani))]
pub(crate) use encode::encoded_len;
#[cfg(feature = "python")]
pub(crate) use encode::{
    CustomEncodeAlphabet, encode_to_ptr, encode_to_ptr_cached, encode_to_ptr_with_custom_alphabet,
    encode_wrapped_to_ptr_cached, encode_wrapped_to_ptr_custom,
};

#[cfg(test)]
pub(crate) use encode::encode_scalar;

#[cfg(all(test, miri))]
mod miri_tests;
#[cfg(kani)]
mod proofs;
#[cfg(test)]
mod tests;
