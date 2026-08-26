use super::long::{LongInput, xxh3_64_long, xxh3_128_long};
use super::short::{
    xxh3_64_medium, xxh3_64_midsize, xxh3_64_small, xxh3_128_len_64, xxh3_128_medium,
    xxh3_128_midsize, xxh3_128_small,
};

/// Computes the canonical XXH3 64-bit hash in one call.
///
/// Runtime CPU dispatch accelerates long inputs where supported while retaining
/// the exact result of the public xxHash algorithm. XXH3 is non-cryptographic.
///
/// # Arguments
///
/// * input - The bytes to hash.
/// * seed - The initial unsigned 64-bit seed.
///
/// # Returns
///
/// The canonical unsigned 64-bit XXH3 value.
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
        0..=16 => xxh3_64_small(input, seed),
        17..=128 => xxh3_64_medium(input, seed),
        129..=240 => xxh3_64_midsize(input, seed),
        _ => xxh3_64_long(LongInput::new(input).unwrap(), seed),
    }
}

/// Computes the canonical XXH3 128-bit hash in one call.
///
/// Runtime CPU dispatch accelerates long inputs where supported while retaining
/// the exact result of the public xxHash algorithm. XXH3 is non-cryptographic.
///
/// # Arguments
///
/// * input - The bytes to hash.
/// * seed - The initial unsigned 64-bit seed.
///
/// # Returns
///
/// Two 64-bit words ordered as `[low64, high64]`. The first element is the low
/// half of the digest and the second element is the high half. The returned pair
/// is not a byte serialization.
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
        0..=16 => xxh3_128_small(input, seed),
        64 => xxh3_128_len_64(input, seed),
        17..=128 => xxh3_128_medium(input, seed),
        129..=240 => xxh3_128_midsize(input, seed),
        _ => xxh3_128_long(LongInput::new(input).unwrap(), seed),
    }
}
