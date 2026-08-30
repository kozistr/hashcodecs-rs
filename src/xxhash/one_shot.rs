//! Select an XXH3 formula from the input length and compute one hash.

use super::long_inputs::{LongInput, xxh3_64_over_240_bytes, xxh3_128_over_240_bytes};
use super::short_inputs::{
    xxh3_64_len_0_to_16, xxh3_64_len_17_to_128, xxh3_64_len_129_to_240, xxh3_128_len_0_to_16,
    xxh3_128_len_17_to_128, xxh3_128_len_64, xxh3_128_len_129_to_240,
};

/// Computes the canonical XXH3 64-bit hash in one call.
///
/// Runtime dispatch selects a supported kernel for inputs longer than 240 bytes.
/// All kernels return the result that the public xxHash algorithm specifies. XXH3 does not provide cryptographic security.
///
/// # Arguments
///
/// * `input` - Contains the bytes to hash.
/// * `seed` - Specifies the initial unsigned 64-bit seed.
///
/// # Returns
///
/// The function returns the canonical unsigned 64-bit XXH3 value.
///
/// # Examples
///
///     use hashcodecs::xxhash::xxh3_64;
///
///     assert_eq!(xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
///     assert_ne!(xxh3_64(b"hello", 0), xxh3_64(b"hello", 1));
///
#[inline]
pub fn xxh3_64(input: &[u8], seed: u64) -> u64 {
    match input.len() {
        0..=16 => xxh3_64_len_0_to_16(input, seed),
        17..=128 => xxh3_64_len_17_to_128(input, seed),
        129..=240 => xxh3_64_len_129_to_240(input, seed),
        _ => xxh3_64_over_240_bytes(LongInput::new(input).unwrap(), seed),
    }
}

/// Computes the canonical XXH3 128-bit hash in one call.
///
/// Runtime dispatch selects a supported kernel for inputs longer than 240 bytes.
/// All kernels return the result that the public xxHash algorithm specifies. XXH3 does not provide cryptographic security.
///
/// # Arguments
///
/// * `input` - Contains the bytes to hash.
/// * `seed` - Specifies the initial unsigned 64-bit seed.
///
/// # Returns
///
/// The function returns two 64-bit words in `[low64, high64]` order.
/// The first element contains the low half. The second element contains the high half.
/// The returned pair does not contain serialized bytes.
///
/// # Examples
///
///     use hashcodecs::xxhash::xxh3_128;
///
///     let [low64, high64] = xxh3_128(b"", 0);
///     assert_eq!(low64, 0x6001_c324_468d_497f);
///     assert_eq!(high64, 0x99aa_06d3_0147_98d8);
///
#[inline]
pub fn xxh3_128(input: &[u8], seed: u64) -> [u64; 2] {
    match input.len() {
        0..=16 => xxh3_128_len_0_to_16(input, seed),
        64 => xxh3_128_len_64(input, seed),
        17..=128 => xxh3_128_len_17_to_128(input, seed),
        129..=240 => xxh3_128_len_129_to_240(input, seed),
        _ => xxh3_128_over_240_bytes(LongInput::new(input).unwrap(), seed),
    }
}
